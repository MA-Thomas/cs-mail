//! Deployment-level storage of the member request loop: protocol policy versions,
//! endpoint key publications, sender content-key certificates, and finding relationships.
use super::{Json, PostgresDeployment, PostgresEngine, StorageError, Transaction, ingress, to_i64};
use cs_mail_application::members::{EndpointKeyContext, EndpointKeyStore};
use cs_mail_content::EncryptedContentRecord;
use cs_mail_primitives::{CanonicalTime, PolicyVersion, ProtocolIdentity};
use cs_mail_protocol::policy::ProtocolPolicy;
use cs_mail_security::{
    AuthoritySnapshot, EndpointKeyPublication, KeyRegistry, SignedContentKeyCertificate,
    SignedEndpointKey,
};

/// The current protocol policy, locked for update when `update` is set, else shared.
pub(super) fn current_protocol_policy(
    tx: &mut Transaction<'_>,
    update: bool,
) -> Result<Option<ProtocolPolicy>, StorageError> {
    // Lock the pointer first, then read its version in a separate statement, so a
    // concurrent publication is never observed half-applied.
    let Some(version) = tx
        .query_opt(
            if update {
                "SELECT version FROM cs_protocol_policy_current WHERE singleton FOR UPDATE"
            } else {
                "SELECT version FROM cs_protocol_policy_current WHERE singleton FOR SHARE"
            },
            &[],
        )?
        .map(|row| row.get::<_, i64>(0))
    else {
        return Ok(None);
    };
    Ok(Some(
        tx.query_one(
            "SELECT policy FROM cs_protocol_policies WHERE version=$1",
            &[&version],
        )?
        .get::<_, Json<ProtocolPolicy>>(0)
        .0,
    ))
}
pub(super) fn protocol_policy_version(
    tx: &mut Transaction<'_>,
    version: PolicyVersion,
) -> Result<Option<ProtocolPolicy>, StorageError> {
    Ok(tx
        .query_opt(
            "SELECT policy FROM cs_protocol_policies WHERE version=$1",
            &[&to_i64(version.0)?],
        )?
        .map(|row| row.get::<_, Json<ProtocolPolicy>>(0).0))
}
/// Records a new protocol policy version and makes it current.
pub(super) fn publish_protocol_policy(
    tx: &mut Transaction<'_>,
    policy: &ProtocolPolicy,
    at: CanonicalTime,
) -> Result<(), StorageError> {
    let version = to_i64(policy.version().0)?;
    tx.execute(
        "INSERT INTO cs_protocol_policies(version,policy,published_at) VALUES($1,$2,$3)",
        &[&version, &Json(policy), &to_i64(at.0)?],
    )?;
    tx.execute(
        "INSERT INTO cs_protocol_policy_current(singleton,version) VALUES(TRUE,$1) \
         ON CONFLICT(singleton) DO UPDATE SET version=EXCLUDED.version",
        &[&version],
    )?;
    Ok(())
}
pub(super) fn current_endpoint_key(
    tx: &mut Transaction<'_>,
    persona: ProtocolIdentity,
    update: bool,
) -> Result<Option<EndpointKeyPublication>, StorageError> {
    Ok(tx
        .query_opt(
            if update {
                "SELECT p.publication FROM cs_endpoint_keys k JOIN cs_endpoint_key_publications p \
                 USING (persona, version) WHERE k.persona=$1 FOR UPDATE OF k"
            } else {
                "SELECT p.publication FROM cs_endpoint_keys k JOIN cs_endpoint_key_publications p \
                 USING (persona, version) WHERE k.persona=$1"
            },
            &[&persona.0.to_string()],
        )?
        .map(|row| row.get::<_, Json<SignedEndpointKey>>(0).0.publication))
}
/// Records a verified sender certificate under its digest, as bound into ciphertext.
pub(super) fn record_certificate(
    tx: &mut Transaction<'_>,
    digest: &[u8; 32],
    certificate: &SignedContentKeyCertificate,
) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO cs_content_certificates(digest,certificate) VALUES($1,$2) \
         ON CONFLICT (digest) DO NOTHING",
        &[&digest.as_slice(), &Json(certificate)],
    )?;
    Ok(())
}

/// One sender writing to a recipient, as stored.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredRelationship {
    pub aggregate_key: String,
    pub sender: ProtocolIdentity,
    pub recipient: ProtocolIdentity,
}

impl PostgresDeployment {
    /// The current protocol policy, if one has been published.
    /// # Errors
    /// Returns database or decoding errors.
    pub fn protocol_policy(&self) -> Result<Option<ProtocolPolicy>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        let policy = current_protocol_policy(&mut tx, false)?;
        tx.commit()?;
        Ok(policy)
    }
    /// The endpoint key currently published for `persona`, if any.
    /// # Errors
    /// Returns database or decoding errors.
    pub fn endpoint_key(
        &self,
        persona: ProtocolIdentity,
    ) -> Result<Option<EndpointKeyPublication>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        let key = current_endpoint_key(&mut tx, persona, false)?;
        tx.commit()?;
        Ok(key)
    }
    /// The live authority of one account: its control record, keys and product.
    /// # Errors
    /// `Identity(Invalid)` for an unknown account; database or decoding errors.
    pub fn account_state(
        &self,
        account: cs_mail_primitives::AccountId,
    ) -> Result<cs_mail_application::accounts::operations::AccountState, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let row = client
            .query_opt(
                "SELECT control,registry,product FROM cs_product_accounts WHERE id=$1",
                &[&account.0.to_string()],
            )?
            .ok_or(identity_contract::Error::Invalid)?;
        Ok(cs_mail_application::accounts::operations::AccountState {
            control: row.try_get::<_, Json<_>>(0)?.0,
            registry: row.try_get::<_, Json<_>>(1)?.0,
            product: row.try_get(2)?,
        })
    }
    /// Opens an existing relationship without creating it.
    /// # Errors
    /// Returns database errors.
    pub fn existing_relationship(
        &self,
        aggregate_key: &str,
    ) -> Result<Option<PostgresEngine>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let exists = client
            .query_opt(
                "SELECT 1 FROM cs_relationship_aggregates WHERE aggregate_key=$1",
                &[&aggregate_key],
            )?
            .is_some();
        drop(client);
        Ok(exists.then(|| PostgresEngine {
            aggregate_key: aggregate_key.to_owned(),
            client: self.client.clone(),
            deployment: self.clone(),
        }))
    }
    /// Every stored relationship in which someone writes to `recipient`.
    /// # Errors
    /// Returns database or decoding errors.
    pub fn relationships_to(
        &self,
        recipient: ProtocolIdentity,
    ) -> Result<Vec<StoredRelationship>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        client
            .query(
                "SELECT aggregate_key, relationship_state#>>'{relationship,key,sender}' \
                 FROM cs_relationship_aggregates \
                 WHERE relationship_state#>>'{relationship,key,recipient}'=$1 \
                 ORDER BY aggregate_key",
                &[&recipient.0.to_string()],
            )?
            .into_iter()
            .map(|row| {
                Ok(StoredRelationship {
                    aggregate_key: row.get(0),
                    sender: ProtocolIdentity(
                        row.get::<_, String>(1)
                            .parse()
                            .map_err(|_| StorageError::NumericRange)?,
                    ),
                    recipient,
                })
            })
            .collect()
    }
    /// Relationships with work a worker could do now: ready or expired-lease relationship
    /// work, or due schedules. Received commands are processed globally in receipt order.
    /// # Errors
    /// Returns database or numeric errors.
    pub fn relationships_with_work(
        &self,
        now: CanonicalTime,
        limit: i64,
    ) -> Result<Vec<String>, StorageError> {
        let now = to_i64(now.0)?;
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        Ok(client
            .query(
                "SELECT aggregate_key FROM ( \
                   SELECT aggregate_key FROM cs_work WHERE aggregate_key IS NOT NULL \
                    AND ((status='ready' AND available_at<=$1) \
                      OR (status='running' AND claim_until<=$1)) \
                   UNION \
                   SELECT aggregate_key FROM cs_schedules WHERE due_at<=$1 \
                    AND (status='pending' OR (status='processing' AND claim_until<=$1)) \
                 ) due ORDER BY aggregate_key LIMIT $2",
                &[&now, &limit],
            )?
            .into_iter()
            .map(|row| row.get(0))
            .collect())
    }
    /// Whether any received command awaits processing.
    /// # Errors
    /// Returns database errors.
    pub fn received_pending(&self) -> Result<Option<String>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        Ok(client
            .query_opt(
                "SELECT aggregate_key FROM cs_received_commands WHERE outcome IS NULL \
                 ORDER BY position LIMIT 1",
                &[],
            )?
            .map(|row| row.get(0)))
    }
}

impl PostgresEngine {
    /// Every ciphertext stored in this relationship, with the verified sender certificate
    /// each was bound to, in content order.
    /// # Errors
    /// Returns database or decoding errors.
    pub fn content_with_certificates(
        &self,
    ) -> Result<Vec<(EncryptedContentRecord, Option<SignedContentKeyCertificate>)>, StorageError>
    {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let records = client
            .query(
                "SELECT record FROM cs_encrypted_content WHERE aggregate_key=$1 \
                 ORDER BY created_at, content_ref",
                &[&self.aggregate_key],
            )?
            .into_iter()
            .map(|row| row.get::<_, Json<EncryptedContentRecord>>(0).0)
            .collect::<Vec<_>>();
        records
            .into_iter()
            .map(|record| {
                let certificate = client
                    .query_opt(
                        "SELECT certificate FROM cs_content_certificates WHERE digest=$1",
                        &[&record.binding.sender_certificate.0.as_slice()],
                    )?
                    .map(|row| row.get::<_, Json<SignedContentKeyCertificate>>(0).0);
                Ok((record, certificate))
            })
            .collect()
    }
}

impl EndpointKeyStore for PostgresDeployment {
    type Error = StorageError;
    fn transact_endpoint_key(
        &self,
        signed: &SignedEndpointKey,
        decide: impl FnOnce(
            EndpointKeyContext,
        )
            -> Result<Option<(EndpointKeyPublication, CanonicalTime)>, StorageError>,
    ) -> Result<(), StorageError> {
        let persona = signed.publication.persona;
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        // Ordered with receipts like other account-authority checks.
        ingress::lock_receipt_order(&mut tx)?;
        let mut authority = AuthoritySnapshot::new(KeyRegistry::default())?;
        ingress::add_persona_authority(&mut tx, persona, &mut authority)?;
        let current = current_endpoint_key(&mut tx, persona, true)?;
        if let Some((publication, at)) = decide(EndpointKeyContext { authority, current })? {
            let version = to_i64(publication.version)?;
            tx.execute(
                "INSERT INTO cs_endpoint_key_publications(persona,version,publication,received_at) \
                 VALUES($1,$2,$3,$4)",
                &[
                    &persona.0.to_string(),
                    &version,
                    &Json(signed),
                    &to_i64(at.0)?,
                ],
            )?;
            tx.execute(
                "INSERT INTO cs_endpoint_keys(persona,version) VALUES($1,$2) \
                 ON CONFLICT(persona) DO UPDATE SET version=EXCLUDED.version",
                &[&persona.0.to_string(), &version],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}
