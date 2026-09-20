//! Durable product ownership and cross-service enrollment. No remote calls inside transactions.
use super::*;
use cs_mail_accounts::EnrollmentInput;
use cs_mail_application::accounts::{ConfirmationFailure, PendingEnrollment};
use cs_mail_primitives::{AccountId, PrincipalRef, ProtocolIdentity};
use identity_contract::{self as contract, DecisionVerifier, SignedDecision};

/// An enrolled account always has a principal and shared identity binding.
#[derive(Debug, Clone)]
pub struct ProductAccountSnapshot {
    pub id: AccountId,
    pub principal: PrincipalRef,
    pub billing: cs_mail_primitives::BillingAccountId,
    pub member: cs_mail_primitives::MemberId,
    pub membership_identity: [u8; 32],
    pub identity: ProductIdentitySnapshot,
}
/// Persisted identity binding; the subject reference is opaque and product scoped.
#[derive(Debug, Clone)]
pub struct ProductIdentitySnapshot {
    pub issuer: String,
    pub product: String,
    pub subject_ref: contract::ProductSubjectRef,
    pub binding_version: u64,
}
fn configuration(tx: &mut Transaction<'_>) -> Result<DecisionVerifier, StorageError> {
    let row = tx
        .query_opt(
            "SELECT issuer,product,decision_keys FROM cs_identity_configuration WHERE singleton",
            &[],
        )?
        .ok_or(contract::Error::Unavailable)?;
    Ok(DecisionVerifier::with_keys(
        row.get(0),
        row.get(1),
        row.get::<_, Json<Vec<contract::DecisionKey>>>(2).0,
    )?)
}
fn pending(tx: &mut Transaction<'_>, operation: &str) -> Result<PendingEnrollment, StorageError> {
    let value = tx
        .query_opt(
            "SELECT pending FROM cs_enrollment_operations WHERE operation=$1 FOR UPDATE",
            &[&operation],
        )?
        .ok_or(contract::Error::Invalid)?
        .get::<_, Json<PendingEnrollment>>(0)
        .0;
    value.validate()?;
    Ok(value)
}
/// Owns account transactions without requiring a relationship aggregate.
#[derive(Clone)]
pub struct PostgresAccountRepository {
    pub(super) client: Arc<Mutex<Client>>,
    pub(super) clock: Arc<dyn cs_mail_application::accounts::AccountClock>,
}
impl PostgresAccountRepository {
    /// # Errors
    /// Returns database connection or schema initialization failures.
    pub fn connect(
        url: &str,
        clock: impl cs_mail_application::accounts::AccountClock + 'static,
    ) -> Result<Self, StorageError> {
        Self::from_client(Client::connect(url, NoTls)?, clock)
    }
    /// # Errors
    /// Returns schema initialization failures. The caller configures connection TLS.
    pub fn from_client(
        mut client: Client,
        clock: impl cs_mail_application::accounts::AccountClock + 'static,
    ) -> Result<Self, StorageError> {
        migrate(&mut client)?;
        Ok(Self {
            client: Arc::new(Mutex::new(client)),
            clock: Arc::new(clock),
        })
    }
    /// Revokes an account-owned key without consulting relationship registries.
    /// # Errors
    /// Rejects missing accounts, foreign keys, stale versions, or storage failures.
    pub fn revoke_account_key(
        &self,
        account: cs_mail_primitives::AccountId,
        reference: OperationalKeyRef,
        expected_version: Version,
        now: CanonicalTime,
    ) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        let row = tx
            .query_opt(
                "SELECT registry FROM cs_product_accounts WHERE id=$1 FOR UPDATE",
                &[&account.0.to_string()],
            )?
            .ok_or(SecurityError::UnknownKey)?;
        let mut registry = row.get::<_, Json<KeyRegistry>>(0).0;
        registry.revoke(reference, expected_version, now)?;
        tx.execute(
            "UPDATE cs_product_accounts SET registry=$2 WHERE id=$1",
            &[&account.0.to_string(), &Json(registry)],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Immutable trusted deployment configuration; enrollment callers cannot select trust roots.
    /// # Errors
    /// Rejects invalid keys or conflicting deployment configuration.
    pub fn configure_identity_service(
        &self,
        issuer: &str,
        product: &str,
        key: [u8; 32],
    ) -> Result<(), StorageError> {
        DecisionVerifier::new(issuer.into(), product.into(), key)?;
        let keys = vec![contract::DecisionKey::new(key, 0, None)?];
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        tx.execute("INSERT INTO cs_identity_configuration(singleton,issuer,product,decision_keys) VALUES(TRUE,$1,$2,$3) ON CONFLICT DO NOTHING",&[&issuer,&product,&Json(&keys)])?;
        let r = tx.query_one(
            "SELECT issuer,product,decision_keys FROM cs_identity_configuration WHERE singleton",
            &[],
        )?;
        if r.get::<_, String>(0) != issuer
            || r.get::<_, String>(1) != product
            || r.get::<_, Json<Vec<contract::DecisionKey>>>(2).0 != keys
        {
            return Err(contract::Error::Conflict.into());
        }
        tx.commit()?;
        Ok(())
    }
    /// Operator trust rotation is versioned and ordered with account authorization.
    /// # Errors
    /// Rejects invalid trust entries or a stale configuration revision.
    pub fn rotate_identity_trust(
        &self,
        expected_revision: u64,
        keys: Vec<contract::DecisionKey>,
    ) -> Result<(), StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        let row = tx.query_one("SELECT issuer,product,revision FROM cs_identity_configuration WHERE singleton FOR UPDATE", &[])?;
        if row.get::<_, i64>(2) != to_i64(expected_revision)? {
            return Err(StorageError::VersionConflict);
        }
        DecisionVerifier::with_keys(row.get(0), row.get(1), keys.clone())?;
        tx.execute("UPDATE cs_identity_configuration SET decision_keys=$1,revision=revision+1 WHERE singleton", &[&Json(keys)])?;
        tx.commit()?;
        Ok(())
    }
    /// # Errors
    /// Rejects missing/corrupt account records.
    pub fn product_account(&self, id: AccountId) -> Result<ProductAccountSnapshot, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let r=db.query_opt("SELECT principal,billing,member,issuer,product,subject_ref,membership_identity,binding_version FROM cs_product_accounts WHERE id=$1",&[&id.0.to_string()])?.ok_or(contract::Error::Invalid)?;
        let parse = |s: String| s.parse::<u128>().map_err(|_| StorageError::NumericRange);
        Ok(ProductAccountSnapshot {
            id,
            membership_identity: r
                .get::<_, Vec<u8>>(6)
                .try_into()
                .map_err(|_| contract::Error::Invalid)?,
            principal: PrincipalRef(parse(r.get(0))?),
            billing: cs_mail_primitives::BillingAccountId(parse(r.get(1))?),
            member: cs_mail_primitives::MemberId(parse(r.get(2))?),
            identity: ProductIdentitySnapshot {
                issuer: r.get(3),
                product: r.get(4),
                subject_ref: r.get::<_, String>(5).try_into()?,
                binding_version: u64::try_from(r.get::<_, i64>(7))
                    .map_err(|_| StorageError::NumericRange)?,
            },
        })
    }
    /// Resolves a persona to its owning product account.
    /// # Errors
    /// Rejects missing ownership, invalid IDs, or storage failures.
    pub fn persona_account(&self, persona: ProtocolIdentity) -> Result<AccountId, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let id = db
            .query_opt(
                "SELECT account FROM cs_persona_owners WHERE identity=$1",
                &[&persona.0.to_string()],
            )?
            .ok_or(contract::Error::Invalid)?
            .get::<_, String>(0);
        Ok(AccountId(
            id.parse().map_err(|_| StorageError::NumericRange)?,
        ))
    }
    /// Stable persona-to-principal resolution for privacy history derivation.
    /// # Errors
    /// Rejects unknown personas or invalid stored identifiers.
    pub fn persona_principal(
        &self,
        persona: ProtocolIdentity,
    ) -> Result<PrincipalRef, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let principal=db.query_opt("SELECT a.principal FROM cs_persona_owners p JOIN cs_product_accounts a ON a.id=p.account WHERE p.identity=$1",&[&persona.0.to_string()])?.map(|r|r.get::<_,String>(0)).ok_or(contract::Error::Invalid)?;
        Ok(PrincipalRef(
            principal.parse().map_err(|_| StorageError::NumericRange)?,
        ))
    }
    /// Authoritative account transparency log and key state.
    /// # Errors
    /// Rejects missing accounts.
    pub fn account_key_registry(&self, id: AccountId) -> Result<KeyRegistry, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        Ok(db
            .query_opt(
                "SELECT registry FROM cs_product_accounts WHERE id=$1",
                &[&id.0.to_string()],
            )?
            .ok_or(contract::Error::Invalid)?
            .get::<_, Json<KeyRegistry>>(0)
            .0)
    }
}
impl cs_mail_application::accounts::EnrollmentRepository for PostgresAccountRepository {
    type Error = StorageError;
    fn reserve(
        &self,
        operation: &str,
        input: &EnrollmentInput,
        decide: impl FnOnce(
            cs_mail_application::accounts::enrollment::ReservationContext,
            &dyn cs_mail_application::accounts::AccountClock,
        ) -> Result<PendingEnrollment, StorageError>,
    ) -> Result<PendingEnrollment, StorageError> {
        use cs_mail_application::accounts::enrollment::ReservationContext;
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        // All account ownership writers take receipt ordering before their row locks.
        ingress::lock_receipt_order(&mut tx)?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtext($1))",
            &[&format!("cs-enrollment:{operation}")],
        )?;
        let prior = tx
            .query_opt(
                "SELECT pending FROM cs_enrollment_operations WHERE operation=$1 FOR UPDATE",
                &[&operation],
            )?
            .map(|r| r.get::<_, Json<PendingEnrollment>>(0).0);
        let existed = prior.is_some();
        let verifier = configuration(&mut tx)?;
        let (_, bank_key) = billing::arrangement(&mut tx, input.bank.scope, input.bank.unit)?;
        let pending = decide(
            ReservationContext {
                prior,
                verifier,
                bank_key,
            },
            self.clock.as_ref(),
        )?;
        if !existed {
            tx.execute("INSERT INTO cs_enrollment_operations(operation,pending,billing,member,persona,key_reference,bank_token) VALUES($1,$2,$3,$4,$5,$6,$7)", &[&operation,&Json(&pending),&pending.input().bank.account.0.to_string(),&pending.input().bank.member.0.to_string(),&pending.input().persona.0.to_string(),&pending.input().key_ref.0.to_string(),&pending.input().bank.bank_token.as_slice()])?;
            super::key_claims::reserve(&mut tx, &pending)?;
        }
        tx.commit()?;
        Ok(pending)
    }
    fn renewal(
        &self,
        operation: &str,
        decide: impl FnOnce(
            PendingEnrollment,
            bool,
            &dyn cs_mail_application::accounts::AccountClock,
        ) -> Result<PendingEnrollment, StorageError>,
    ) -> Result<PendingEnrollment, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        let pending = pending(&mut tx, operation)?;
        let committed = tx
            .query_one(
                "SELECT decision IS NOT NULL FROM cs_enrollment_operations WHERE operation=$1",
                &[&operation],
            )?
            .get(0);
        let renewed = decide(pending, committed, self.clock.as_ref())?;
        tx.execute(
            "UPDATE cs_enrollment_operations SET pending=$2 WHERE operation=$1",
            &[&operation, &Json(&renewed)],
        )?;
        tx.commit()?;
        Ok(renewed)
    }
    fn pending(&self, operation: &str) -> Result<PendingEnrollment, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        let value = pending(&mut tx, operation)?;
        tx.commit()?;
        Ok(value)
    }
    fn activation(
        &self,
        decision: &SignedDecision,
        decide: impl FnOnce(
            cs_mail_application::accounts::enrollment::ActivationContext,
            &dyn cs_mail_application::accounts::AccountClock,
        ) -> Result<
            cs_mail_application::accounts::enrollment::EnrollmentDecision,
            StorageError,
        >,
    ) -> Result<AccountId, StorageError> {
        use cs_mail_application::accounts::enrollment::ActivationContext;
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        let operation = &decision.claims.intent.operation;
        let pending = pending(&mut tx, operation)?;
        let prior = tx
            .query_one(
                "SELECT decision FROM cs_enrollment_operations WHERE operation=$1",
                &[&operation],
            )?
            .get::<_, Option<Json<SignedDecision>>>(0)
            .map(|v| v.0);
        let verifier = configuration(&mut tx)?;
        let (_, bank_key) = billing::arrangement(
            &mut tx,
            pending.input().bank.scope,
            pending.input().bank.unit,
        )?;
        let key_claim = tx
            .query_opt(
                "SELECT claim FROM cs_key_claims WHERE reference=$1 FOR UPDATE",
                &[&pending.input().key_ref.0.to_string()],
            )?
            .ok_or(SecurityError::UnknownKey)?
            .get::<_, Json<cs_mail_accounts::keys::KeyClaim>>(0)
            .0;
        let (account, records) = decide(
            ActivationContext {
                pending: pending.clone(),
                prior,
                verifier,
                bank_key,
                key_claim,
            },
            self.clock.as_ref(),
        )?
        .into_parts();
        if let Some(records) = records {
            let at = records.at;
            billing::insert_enrollment(&mut tx, &pending, records)?;
            tx.execute("UPDATE cs_enrollment_operations SET decision=$2,account=$3,authorized_at=$4 WHERE operation=$1", &[&operation,&Json(decision),&account.0.to_string(),&to_i64(at.0)?])?;
            tx.execute(
                "INSERT INTO cs_identity_outbox(operation) VALUES($1)",
                &[&operation],
            )?;
        }
        tx.commit()?;
        Ok(account)
    }
    fn claim_confirmations(
        &self,
        limit: u32,
        at: CanonicalTime,
    ) -> Result<Vec<cs_mail_application::accounts::ConfirmationClaim>, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let until = to_i64(
            at.0.checked_add(cs_mail_application::accounts::CONFIRMATION_LEASE_MILLIS)
                .ok_or(StorageError::NumericRange)?,
        )?;
        let rows = db.query("WITH work AS (SELECT operation FROM cs_identity_outbox WHERE NOT confirmed AND intervention IS NULL AND next_attempt <= $2 AND (lease_until IS NULL OR lease_until <= $2) ORDER BY next_attempt,operation LIMIT $1 FOR UPDATE SKIP LOCKED), claimed AS (UPDATE cs_identity_outbox o SET generation=o.generation+1,lease_until=$3 FROM work w WHERE o.operation=w.operation RETURNING o.operation,o.generation) SELECT e.decision,c.generation FROM claimed c JOIN cs_enrollment_operations e USING(operation)", &[&i64::from(limit), &to_i64(at.0)?, &until])?;
        rows.into_iter()
            .map(|row| {
                Ok(cs_mail_application::accounts::ConfirmationClaim {
                    decision: row.get::<_, Json<SignedDecision>>(0).0,
                    generation: u64::try_from(row.get::<_, i64>(1))
                        .map_err(|_| StorageError::NumericRange)?,
                })
            })
            .collect()
    }
    fn mark_confirmed(
        &self,
        claim: &cs_mail_application::accounts::ConfirmationClaim,
    ) -> Result<(), StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        tx.query_opt(
            "SELECT operation FROM cs_identity_outbox WHERE operation=$1 FOR UPDATE",
            &[&claim.operation()],
        )?
        .ok_or(contract::Error::Invalid)?;
        let at = to_i64(self.clock.now().0)?;
        if tx.execute("UPDATE cs_identity_outbox SET confirmed=TRUE,lease_until=NULL WHERE operation=$1 AND generation=$2 AND lease_until > $3 AND NOT confirmed", &[&claim.operation(), &to_i64(claim.generation)?, &at])? != 1 { return Err(contract::Error::Conflict.into()); }
        tx.commit()?;
        Ok(())
    }
    fn retry_confirmation(&self, operation: &str, at: CanonicalTime) -> Result<(), StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        if db.execute(
            "UPDATE cs_identity_outbox SET next_attempt=$2,intervention=NULL,lease_until=NULL,generation=generation+1 WHERE operation=$1 AND NOT confirmed",
            &[&operation, &to_i64(at.0)?],
        )? != 1
        {
            return Err(contract::Error::Invalid.into());
        }
        Ok(())
    }
    fn confirmation_failed(
        &self,
        claim: &cs_mail_application::accounts::ConfirmationClaim,
        failure: ConfirmationFailure,
    ) -> Result<(), StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        tx.query_opt(
            "SELECT operation FROM cs_identity_outbox WHERE operation=$1 FOR UPDATE",
            &[&claim.operation()],
        )?
        .ok_or(contract::Error::Invalid)?;
        let at = self.clock.now();
        let intervention = match failure {
            ConfirmationFailure::Retryable => None,
            ConfirmationFailure::Intervention(error) => Some(Json(error)),
        };
        let next = to_i64(failure.next_attempt(at)?.0)?;
        if tx.execute("UPDATE cs_identity_outbox SET attempts=attempts+1,next_attempt=$2,intervention=$3,lease_until=NULL WHERE operation=$1 AND generation=$4 AND lease_until > $5 AND NOT confirmed", &[&claim.operation(),&next,&intervention,&to_i64(claim.generation)?,&to_i64(at.0)?])? != 1 { return Err(contract::Error::Conflict.into()); }
        tx.commit()?;
        Ok(())
    }
}

impl PostgresAccountRepository {
    /// # Errors
    /// Rejects unknown accounts or invalid stored control state.
    pub fn account_control(
        &self,
        account: AccountId,
    ) -> Result<cs_mail_accounts::control::AccountControl, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        Ok(db
            .query_opt(
                "SELECT control FROM cs_product_accounts WHERE id=$1",
                &[&account.0.to_string()],
            )?
            .ok_or(contract::Error::Invalid)?
            .get::<_, Json<cs_mail_accounts::control::AccountControl>>(0)
            .0)
    }
}

use cs_mail_accounts::control::{AccountControl, SignedAccountCommand};
use cs_mail_application::accounts::operations::{
    AccountDecision, AccountEffects, AccountState, AccountStore, BankChangeContext, CommandContext,
    OperationConflict, SecurityContext,
};
impl From<OperationConflict> for StorageError {
    fn from(error: OperationConflict) -> Self {
        match error {
            OperationConflict::Version => Self::VersionConflict,
            OperationConflict::Duplicate => Self::DuplicateConflict,
        }
    }
}
fn persist_effects(
    tx: &mut Transaction<'_>,
    account: &str,
    effects: AccountEffects,
) -> Result<CanonicalTime, StorageError> {
    let cs_mail_application::accounts::operations::AccountEffectRecords {
        state,
        key,
        persona,
        bank,
        authorized_at: at,
    } = effects.into_records();
    if let Some((reference, claim)) = key {
        super::key_claims::insert(tx, reference, &claim)?;
    }
    if let Some(persona) = persona {
        if tx.query_opt("SELECT 1 FROM cs_enrollment_operations WHERE persona=$1 AND (account IS NULL OR account<>$2)", &[&persona.0.to_string(),&account])?.is_some() {return Err(contract::Error::Conflict.into());}
        tx.execute(
            "INSERT INTO cs_persona_owners(identity,account) VALUES($1,$2)",
            &[&persona.0.to_string(), &account],
        )?;
    }
    if let Some((financial, source)) = bank {
        let bank = financial.bank().evidence();
        // Protect ownership against pending reservations, including absent source rows.
        if tx.query_opt("SELECT 1 FROM cs_enrollment_operations WHERE bank_token=$1 AND (account IS NULL OR account<>$2)", &[&bank.bank_token.as_slice(), &account])?.is_some() { return Err(contract::Error::Conflict.into()); }
        let source_id = billing::source_key(bank.scope, &bank.bank_token);
        tx.execute("INSERT INTO cs_funding_sources(source_key,account,record) VALUES($1,$2,$3) ON CONFLICT DO NOTHING", &[&source_id,&bank.account.0.to_string(),&Json(source)])?;
        let owner: String = tx
            .query_one(
                "SELECT account FROM cs_funding_sources WHERE source_key=$1 FOR UPDATE",
                &[&source_id],
            )?
            .get(0);
        if owner != bank.account.0.to_string() {
            return Err(contract::Error::Conflict.into());
        }
        tx.execute(
            "UPDATE cs_billing_accounts SET record=$2 WHERE id=$1",
            &[&bank.account.0.to_string(), &Json(financial)],
        )?;
    }
    tx.execute(
        "UPDATE cs_product_accounts SET control=$2,registry=$3 WHERE id=$1",
        &[&account, &Json(state.control), &Json(state.registry)],
    )?;
    Ok(at)
}
impl AccountStore for PostgresAccountRepository {
    type Error = StorageError;
    fn command(
        &self,
        signed: &SignedAccountCommand,
        decide: impl FnOnce(CommandContext) -> Result<AccountDecision<AccountControl>, StorageError>,
    ) -> Result<AccountControl, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        let account = signed.account.0.to_string();
        let row = tx
            .query_opt(
                "SELECT registry,control,product FROM cs_product_accounts WHERE id=$1 FOR UPDATE",
                &[&account],
            )?
            .ok_or(contract::Error::Invalid)?;
        let state = AccountState {
            registry: row.get::<_, Json<KeyRegistry>>(0).0,
            control: row.get::<_, Json<AccountControl>>(1).0,
            product: row.get(2),
        };
        let command_id = signed.idempotency_key.0.to_string();
        let prior = tx.query_opt("SELECT command,outcome FROM cs_product_account_commands WHERE account=$1 AND id=$2", &[&account, &command_id])?.map(|row| (row.get::<_,Json<SignedAccountCommand>>(0).0, row.get::<_,Json<AccountControl>>(1).0));
        let (outcome, effects) = decide(CommandContext { state, prior })?.into_parts();
        if let Some(effects) = effects {
            let at = persist_effects(&mut tx, &account, effects)?;
            tx.execute("INSERT INTO cs_product_account_commands(account,id,command,outcome,authorized_at) VALUES($1,$2,$3,$4,$5)", &[&account,&command_id,&Json(signed),&Json(&outcome),&to_i64(at.0)?])?;
        }
        tx.commit()?;
        Ok(outcome)
    }
    fn security_change(
        &self,
        signed: &contract::changes::SignedSecurityEvent,
        bank: Option<&cs_mail_finance::BankVerification>,
        decide: impl FnOnce(SecurityContext) -> Result<AccountDecision<()>, StorageError>,
    ) -> Result<(), StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        let account = &signed.event.account;
        let row = tx.query_opt("SELECT control,registry,subject_ref,security_version,billing,product FROM cs_product_accounts WHERE id=$1 FOR UPDATE", &[account])?.ok_or(contract::Error::Invalid)?;
        let state = AccountState {
            control: row.get::<_, Json<AccountControl>>(0).0,
            registry: row.get::<_, Json<KeyRegistry>>(1).0,
            product: row.get(5),
        };
        let subject = row.get::<_, String>(2).try_into()?;
        let version =
            u64::try_from(row.get::<_, i64>(3)).map_err(|_| StorageError::NumericRange)?;
        let prior = tx
            .query_opt(
                "SELECT event FROM cs_identity_security_events WHERE account=$1 AND version=$2",
                &[account, &to_i64(signed.event.security_version)?],
            )?
            .map(|r| {
                r.get::<_, Json<contract::changes::SignedSecurityEvent>>(0)
                    .0
            });
        let trust = configuration(&mut tx)?;
        let bank = if bank.is_some() {
            let financial = tx
                .query_one(
                    "SELECT record FROM cs_billing_accounts WHERE id=$1 FOR UPDATE",
                    &[&row.get::<_, String>(4)],
                )?
                .get::<_, Json<cs_mail_billing::BillingAccount>>(0)
                .0;
            let (_, authority) =
                billing::arrangement(&mut tx, financial.scope(), financial.unit())?;
            let pending = tx
                .query_one(
                    "SELECT pending FROM cs_enrollment_operations WHERE account=$1",
                    &[account],
                )?
                .get::<_, Json<PendingEnrollment>>(0)
                .0;
            Some(BankChangeContext {
                account: financial,
                authority,
                maximum_unresolved: pending.input().maximum_unresolved,
            })
        } else {
            None
        };
        let ((), effects) = decide(SecurityContext {
            state,
            subject,
            version,
            trust,
            prior,
            bank,
        })?
        .into_parts();
        if let Some(effects) = effects {
            let at = persist_effects(&mut tx, account, effects)?;
            tx.execute(
                "UPDATE cs_product_accounts SET security_version=$2 WHERE id=$1",
                &[account, &to_i64(signed.event.security_version)?],
            )?;
            tx.execute("INSERT INTO cs_identity_security_events(account,version,event,applied_at) VALUES($1,$2,$3,$4)", &[account,&to_i64(signed.event.security_version)?,&Json(signed),&to_i64(at.0)?])?;
        }
        tx.commit()?;
        Ok(())
    }
}
impl cs_mail_application::accounts::IdentitySecurityRepository for PostgresAccountRepository {
    type Error = StorageError;
    fn security_cursor(
        &self,
        account: AccountId,
    ) -> Result<cs_mail_application::accounts::IdentitySecurityCursor, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let row = db
            .query_opt(
                "SELECT product,subject_ref,security_version FROM cs_product_accounts WHERE id=$1",
                &[&account.0.to_string()],
            )?
            .ok_or(contract::Error::Invalid)?;
        Ok(cs_mail_application::accounts::IdentitySecurityCursor {
            product: row.get(0),
            subject: row.get::<_, String>(1).try_into()?,
            version: u64::try_from(row.get::<_, i64>(2)).map_err(|_| StorageError::NumericRange)?,
        })
    }
}
