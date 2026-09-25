//! Local custody operations share the account/retention/consent commit boundary.
use super::{PostgresAccountRepository, StorageError, ingress};
use cs_mail_application::{
    accounts::operations::AccountState,
    consent::{
        self, ConsentCommand, ConsentContext, ConsentDecision, ConsentReceipt, ConsentResponse,
        ConsentStore, ProcessorRegistration, RecoveryCopy, RetainedRecovery, SignedConsentRequest,
    },
    correspondence::MessageRecord,
};
use cs_mail_consent::{ConsentError, DecryptionActor, DecryptionGrant};
use cs_mail_content::EndpointPublicKey;
use cs_mail_primitives::{MessageId, OperationalKeyRef};
use postgres::{Transaction, types::Json};
use std::collections::BTreeMap;

impl PostgresAccountRepository {
    /// Trusted host configuration, never a user-selected decryption root.
    /// # Errors
    /// Rejects empty or replaced keys; root rotation needs an explicit rewrapping protocol.
    pub fn configure_custody(&self, key: EndpointPublicKey) -> Result<(), StorageError> {
        if key.reference.0 == 0 || key.bytes == [0; 32] {
            return Err(ConsentError::Invalid.into());
        }
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        tx.execute("INSERT INTO cs_custody_configuration(singleton,public_key) VALUES(TRUE,$1) ON CONFLICT DO NOTHING", &[&Json(key)])?;
        if custody_key(&mut tx)? != key {
            return Err(ConsentError::Conflict.into());
        }
        tx.commit()?;
        Ok(())
    }
    /// Publishes only the configured public encryption key. Authenticate the host transport.
    /// # Errors
    /// Rejects an unconfigured custody host.
    pub fn custody_public_key(&self) -> Result<EndpointPublicKey, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        custody_key(&mut tx)
    }
    /// Trusted operator registration; this cannot create a user's consent grant.
    /// # Errors
    /// Rejects key reuse or a malformed processor binding.
    pub fn configure_content_processor(
        &self,
        processor: &ProcessorRegistration,
    ) -> Result<(), StorageError> {
        processor.validate()?;
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        tx.execute(
            "INSERT INTO cs_content_processors(key,record) VALUES($1,$2) ON CONFLICT DO NOTHING",
            &[&processor.key.0.to_string(), &Json(processor)],
        )?;
        if load_processor(&mut tx, processor.key)?.as_ref() != Some(processor) {
            return Err(ConsentError::Conflict.into());
        }
        tx.commit()?;
        Ok(())
    }
    /// # Errors
    /// Disables new use without altering historical grants or their audit evidence.
    pub fn disable_content_processor(&self, key: OperationalKeyRef) -> Result<(), StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        let mut processor = load_processor(&mut tx, key)?.ok_or(ConsentError::Unavailable)?;
        processor.active = false;
        tx.execute(
            "UPDATE cs_content_processors SET record=$2 WHERE key=$1",
            &[&key.0.to_string(), &Json(processor)],
        )?;
        tx.commit()?;
        Ok(())
    }
}
pub(super) fn custody_key(tx: &mut Transaction<'_>) -> Result<EndpointPublicKey, StorageError> {
    let key = tx
        .query_opt(
            "SELECT public_key FROM cs_custody_configuration WHERE singleton FOR SHARE",
            &[],
        )?
        .ok_or(ConsentError::Unavailable)?
        .try_get::<_, Json<EndpointPublicKey>>(0)?
        .0;
    if key.reference.0 == 0 || key.bytes == [0; 32] {
        return Err(ConsentError::Invalid.into());
    }
    Ok(key)
}
fn load_processor(
    tx: &mut Transaction<'_>,
    key: OperationalKeyRef,
) -> Result<Option<ProcessorRegistration>, StorageError> {
    let result = tx
        .query_opt(
            "SELECT record FROM cs_content_processors WHERE key=$1 FOR SHARE",
            &[&key.0.to_string()],
        )?
        .map(|r| r.try_get::<_, Json<ProcessorRegistration>>(0).map(|v| v.0))
        .transpose()?;
    if let Some(p) = &result {
        p.validate()?;
        if p.key != key {
            return Err(ConsentError::Invalid.into());
        }
    }
    Ok(result)
}
fn load_context(
    tx: &mut Transaction<'_>,
    r: &SignedConsentRequest,
    clock: &dyn cs_mail_application::accounts::AccountClock,
) -> Result<ConsentContext, StorageError> {
    let row = tx.query_opt("SELECT a.control,a.registry,a.product FROM cs_product_accounts a JOIN cs_persona_owners p ON p.account=a.id WHERE a.id=$1 AND p.identity=$2 FOR UPDATE OF a", &[&r.account.0.to_string(),&r.persona.0.to_string()])?.ok_or(ConsentError::Unauthorized)?;
    let authority = AccountState {
        control: row.try_get::<_, Json<_>>(0)?.0,
        registry: row.try_get::<_, Json<_>>(1)?.0,
        product: row.try_get(2)?,
    };
    let processor_key = match (&r.actor, &r.command) {
        (DecryptionActor::CsqdProcessor(k), _) => Some(*k),
        (_, ConsentCommand::Authorize { usage, .. }) => match usage.actor {
            DecryptionActor::CsqdProcessor(k) => Some(k),
            DecryptionActor::UserDevice(_) => None,
        },
        _ => None,
    };
    let processor = processor_key
        .map(|k| load_processor(tx, k))
        .transpose()?
        .flatten();
    consent::authenticate(r, &authority, processor.as_ref(), clock.now())?;
    let mut context = ConsentContext {
        authority,
        custody_key: custody_key(tx)?,
        processor,
        grant: None,
        retained: BTreeMap::new(),
        prior: None,
    };
    context.prior = tx
        .query_opt(
            "SELECT record FROM cs_consent_receipts WHERE account=$1 AND operation=$2",
            &[&r.account.0.to_string(), &r.operation.0.to_string()],
        )?
        .map(|row| row.try_get::<_, Json<ConsentReceipt>>(0).map(|v| v.0))
        .transpose()?;
    if context.prior.is_some() {
        return Ok(context);
    }
    context.grant = tx
        .query_opt(
            "SELECT record FROM cs_decryption_grants WHERE id=$1 FOR UPDATE",
            &[&r.command.grant_id().value().to_string()],
        )?
        .map(|row| row.try_get::<_, Json<DecryptionGrant>>(0).map(|v| v.0))
        .transpose()?;
    if context
        .grant
        .as_ref()
        .is_some_and(|g| g.terms().id != r.command.grant_id())
    {
        return Err(ConsentError::Invalid.into());
    }
    let messages: Vec<MessageId> = match &r.command {
        ConsentCommand::Authorize { scope, .. } => scope.messages().iter().copied().collect(),
        ConsentCommand::Read { message, .. } => vec![*message],
        _ => vec![],
    };
    for message in messages {
        // Both the actor-owned original copy and recovery copy must still exist.
        let Some(row) = tx.query_opt("SELECT m.record,r.record,c.ciphertext->>'expires_at' FROM cs_correspondence_messages m JOIN cs_correspondence_copies c ON c.message=m.id JOIN cs_correspondence_recovery r ON r.message=c.message AND r.owner=c.owner WHERE m.id=$1 AND c.owner=$2 FOR SHARE OF c,r", &[&message.0.to_string(),&r.persona.0.to_string()])? else { continue; };
        let record = row.try_get::<_, Json<MessageRecord>>(0)?.0;
        let recovery = row.try_get::<_, Json<RecoveryCopy>>(1)?.0;
        let expires_at = cs_mail_primitives::CanonicalTime(
            row.try_get::<_, String>(2)?
                .parse()
                .map_err(|_| ConsentError::Invalid)?,
        );
        context.retained.insert(
            message,
            RetainedRecovery {
                record,
                recovery,
                expires_at,
            },
        );
    }
    Ok(context)
}
impl ConsentStore for PostgresAccountRepository {
    type Error = StorageError;
    fn consent_transaction(
        &self,
        r: &SignedConsentRequest,
        decide: impl FnOnce(
            ConsentContext,
            &dyn cs_mail_application::accounts::AccountClock,
        ) -> Result<ConsentDecision, StorageError>,
    ) -> Result<ConsentResponse, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        let context = load_context(&mut tx, r, &*self.clock)?;
        let (response, effects) = decide(context, &*self.clock)?.into_parts();
        if let Some(effects) = effects {
            if let Some(grant) = effects.grant {
                let t = grant.terms();
                tx.execute("INSERT INTO cs_decryption_grants(id,account,persona,record) VALUES($1,$2,$3,$4) ON CONFLICT(id) DO UPDATE SET record=EXCLUDED.record", &[&t.id.value().to_string(),&t.account.0.to_string(),&t.persona.0.to_string(),&Json(&grant)])?;
            }
            tx.execute(
                "INSERT INTO cs_consent_receipts(account,operation,record) VALUES($1,$2,$3)",
                &[
                    &r.account.0.to_string(),
                    &r.operation.0.to_string(),
                    &Json(effects.receipt),
                ],
            )?;
        }
        // Until this succeeds the sealed result stays local and cannot be delivered.
        tx.commit()?;
        Ok(response)
    }
}
