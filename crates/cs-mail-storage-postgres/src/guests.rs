//! Guest senders in `PostgreSQL`: the sender-subject registry, mailbox verifications and
//! sessions, guest relationships with their request-history links and merges, guest
//! payment instruments, guest command ingress and content stored on arrival, and the
//! notices owed to guests.
use super::{
    Json, PostgresDeployment, PostgresEngine, StorageError, Transaction, bootstrap_in, domain,
    ingress, scoped_key, to_i64,
};
use cs_mail_application::guests::{
    GuestApproach, GuestRelationshipContext, GuestRelationshipPlan, GuestStore,
};
use cs_mail_content::EncryptedContentRecord;
use cs_mail_finance::{FinancialScope, FundingSource, PaymentOperation};
use cs_mail_guests::{
    CodeCheck, GuestError, GuestMailbox, GuestSession, HistoryEvidence, MailboxVerification,
    SenderSubject, SessionTokenDigest,
};
use cs_mail_primitives::{
    CanonicalTime, DeclarationAuthority, Duration, ProtocolIdentity, RequestHistoryRef,
    SettlementUnit,
};
use cs_mail_protocol::{ProtocolState, RequestHistory};
use cs_mail_security::{SignedContentKeyCertificate, SigningScope};
use sha2::{Digest as _, Sha256};

/// How many verification codes one mailbox may request in [`VERIFICATION_WINDOW`].
pub const VERIFICATION_LIMIT: i64 = 5;
/// The window [`VERIFICATION_LIMIT`] applies to.
pub const VERIFICATION_WINDOW: Duration = Duration(60 * 60 * 1000);
/// Unresolved captures one guest instrument may hold at once.
const GUEST_MAXIMUM_UNRESOLVED: u32 = 1;

impl PostgresDeployment {
    /// Reserve the immutable inputs before any preparation effects. Concurrent retries
    /// serialize on the unique operation; only the domain's identical-input replay passes.
    /// # Errors
    /// Rejects altered retries, or returns persistence failures without starting a charge.
    pub fn bind_guest_request(
        &self,
        session: &GuestSession,
        deployment: &[u8; 32],
        operation: cs_mail_primitives::IdempotencyKey,
        fingerprint: cs_mail_guests::GuestRequestFingerprint,
        clock: impl FnOnce() -> CanonicalTime,
    ) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        let sender = session
            .mailbox()
            .synthetic_protocol_identity(deployment)
            .0
            .to_string();
        let operation = operation.0.to_string();
        tx.execute("INSERT INTO cs_guest_request_inputs(sender,operation,fingerprint) VALUES($1,$2,$3) ON CONFLICT DO NOTHING", &[&sender, &operation, &Json(fingerprint)])?;
        let stored = tx
            .query_one(
                "SELECT fingerprint FROM cs_guest_request_inputs WHERE sender=$1 AND operation=$2",
                &[&sender, &operation],
            )?
            .get::<_, Json<cs_mail_guests::GuestRequestFingerprint>>(0)
            .0;
        session.authorize(clock())?;
        fingerprint.check_retry(stored)?;
        tx.commit()?;
        Ok(())
    }
}

/// Registers `subject` as the only sender subject of `identity`, or confirms it. Refuses an
/// identity held by another subject, and a subject key held by another identity.
pub(super) fn register_subject(
    tx: &mut Transaction<'_>,
    identity: ProtocolIdentity,
    subject: &SenderSubject,
    at: CanonicalTime,
) -> Result<(), StorageError> {
    let key = subject.key(identity);
    tx.execute(
        "INSERT INTO cs_sender_subjects(identity,subject_key,subject,registered_at) \
         VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
        &[
            &identity.0.to_string(),
            &key,
            &Json(subject),
            &to_i64(at.0)?,
        ],
    )?;
    let rows = tx.query(
        "SELECT identity,subject FROM cs_sender_subjects WHERE identity=$1 OR subject_key=$2",
        &[&identity.0.to_string(), &key],
    )?;
    if rows.len() != 1
        || rows[0].get::<_, String>(0) != identity.0.to_string()
        || rows[0].get::<_, Json<SenderSubject>>(1).0 != *subject
    {
        return Err(StorageError::SenderSubjectConflict);
    }
    Ok(())
}
pub(super) fn subject_of(
    tx: &mut Transaction<'_>,
    identity: ProtocolIdentity,
) -> Result<Option<SenderSubject>, StorageError> {
    Ok(tx
        .query_opt(
            "SELECT subject FROM cs_sender_subjects WHERE identity=$1",
            &[&identity.0.to_string()],
        )?
        .map(|row| row.get::<_, Json<SenderSubject>>(0).0))
}
pub(super) fn is_guest(
    tx: &mut Transaction<'_>,
    identity: ProtocolIdentity,
) -> Result<bool, StorageError> {
    Ok(subject_of(tx, identity)?.is_some_and(|s| s.is_guest()))
}
fn guest_source_key(scope: FinancialScope, guest: ProtocolIdentity, token: &[u8; 32]) -> String {
    let digest = Sha256::new()
        .chain_update(b"cs-mail/guest-funding-source/v1")
        .chain_update(scope.canonical_bytes())
        .chain_update(guest.0.to_be_bytes())
        .chain_update(token)
        .finalize();
    digest.iter().fold(String::with_capacity(64), |mut key, b| {
        use std::fmt::Write as _;
        let _ = write!(key, "{b:02x}");
        key
    })
}
/// Reserves a guest's capture on the instrument the guest registered with this token.
pub(super) fn reserve_guest_source(
    tx: &mut Transaction<'_>,
    guest: ProtocolIdentity,
    operation: &PaymentOperation,
) -> Result<(), StorageError> {
    let key = guest_source_key(operation.scope, guest, &operation.destination);
    let row = tx
        .query_opt(
            "SELECT guest,record FROM cs_funding_sources WHERE source_key=$1 FOR UPDATE",
            &[&key],
        )?
        .ok_or(cs_mail_billing::BillingError::Payment(
            cs_mail_finance::PaymentError::FundingRestricted,
        ))?;
    if row.get::<_, Option<String>>(0) != Some(guest.0.to_string()) {
        return Err(cs_mail_billing::BillingError::Conflict.into());
    }
    let mut source = row.get::<_, Json<FundingSource>>(1).0;
    source
        .reserve(operation)
        .map_err(|e| StorageError::Billing(cs_mail_billing::BillingError::Payment(e)))?;
    tx.execute(
        "UPDATE cs_funding_sources SET record=$2 WHERE source_key=$1",
        &[&key, &Json(source)],
    )?;
    tx.execute(
        "INSERT INTO cs_funding_source_operations(operation,source_key) VALUES($1,$2) \
         ON CONFLICT (operation) DO NOTHING",
        &[&operation.id.0.to_string(), &key],
    )?;
    Ok(())
}
fn history_key(history: &RequestHistoryRef) -> Vec<u8> {
    scoped_key(history.derivation_version(), history.as_bytes())
}
fn history_text(history: &RequestHistoryRef) -> String {
    history_key(history)
        .iter()
        .fold(String::with_capacity(68), |mut text, b| {
            use std::fmt::Write as _;
            let _ = write!(text, "{b:02x}");
            text
        })
}
pub(super) fn linked_history(
    tx: &mut Transaction<'_>,
    recipient: ProtocolIdentity,
    evidence: &HistoryEvidence,
) -> Result<Option<RequestHistory>, StorageError> {
    let Some(text) = tx
        .query_opt(
            "SELECT history FROM cs_guest_history_links WHERE recipient=$1 AND evidence=$2 FOR UPDATE",
            &[&recipient.0.to_string(), &evidence.key()],
        )?
        .map(|row| row.get::<_, String>(0))
    else {
        return Ok(None);
    };
    let key = (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .map_err(|_| StorageError::NumericRange)?;
    Ok(Some(
        tx.query_one(
            "SELECT history_state FROM cs_request_histories WHERE request_history=$1 FOR UPDATE",
            &[&key],
        )?
        .get::<_, Json<RequestHistory>>(0)
        .0,
    ))
}

/// A guest writing to a member, as stored.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuestRelationship {
    pub aggregate_key: String,
    pub guest: ProtocolIdentity,
    pub mailbox: GuestMailbox,
    pub recipient: ProtocolIdentity,
}

impl PostgresDeployment {
    /// The sender subject registered under `identity`, if any.
    /// # Errors
    /// Returns database or decoding errors.
    pub fn sender_subject(
        &self,
        identity: ProtocolIdentity,
    ) -> Result<Option<SenderSubject>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        let subject = subject_of(&mut tx, identity)?;
        tx.commit()?;
        Ok(subject)
    }
    /// Records a new mailbox verification, unless the mailbox asked for too many recently.
    /// # Errors
    /// `Guest(RateLimited)` over the limit; database errors.
    pub fn begin_guest_verification(
        &self,
        deployment: &[u8; 32],
        id: &str,
        mailbox: &GuestMailbox,
        code: &str,
        salt: &[u8; 32],
        clock: impl FnOnce() -> CanonicalTime,
    ) -> Result<cs_mail_application::notices::NoticeRecord, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        let address = mailbox.address();
        // Serializes verifications of one mailbox so concurrent requests cannot pass the
        // limit together.
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtext('cs-mail-guest-verification:' || $1))",
            &[&address],
        )?;
        let now = clock();
        let since = to_i64(now.0.saturating_sub(VERIFICATION_WINDOW.0))?;
        let recent: i64 = tx
            .query_one(
                "SELECT count(*) FROM cs_guest_verifications \
                 WHERE verification->>'mailbox'=$1 AND created_at>=$2",
                &[&address, &since],
            )?
            .get(0);
        if recent >= VERIFICATION_LIMIT {
            return Err(GuestError::RateLimited.into());
        }
        let verification = MailboxVerification::issue(mailbox.clone(), code, salt, now)?;
        let notice = cs_mail_application::notices::NoticeRecord::new(
            deployment,
            mailbox.clone(),
            cs_mail_application::notices::NoticePurpose::Verification {
                challenge: id.to_owned(),
            },
            now,
        )?;
        // The mailbox lock serializes challenges and their one-shot delivery attempt.
        super::notices::reserve_in(&mut tx, &notice, now)?;
        tx.execute(
            "INSERT INTO cs_guest_verifications(id,verification,created_at) VALUES($1,$2,$3)",
            &[&id, &Json(verification), &to_i64(now.0)?],
        )?;
        tx.commit()?;
        Ok(notice)
    }
    /// Checks a code; on success opens a session under `token_digest`. A wrong code is
    /// recorded as an attempt.
    /// # Errors
    /// The verification's refusals, `Guest(WrongCode)`, database errors.
    pub fn confirm_guest_verification(
        &self,
        id: &str,
        code: &str,
        salt: &[u8; 32],
        token_digest: SessionTokenDigest,
        clock: impl FnOnce() -> CanonicalTime,
    ) -> Result<GuestSession, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        let verification = tx
            .query_opt(
                "SELECT verification FROM cs_guest_verifications WHERE id=$1 FOR UPDATE",
                &[&id],
            )?
            .ok_or(GuestError::VerificationClosed)?
            .try_get::<_, Json<MailboxVerification>>(0)?
            .0;
        let (next, session) = match verification.check(code, salt, token_digest, clock())? {
            CodeCheck::Verified(completion) => {
                let (next, session) = completion.into_parts();
                (next, Some(session))
            }
            CodeCheck::Wrong(next) => (next, None),
        };
        tx.execute(
            "UPDATE cs_guest_verifications SET verification=$2 WHERE id=$1",
            &[&id, &Json(&next)],
        )?;
        if session.is_some() {
            tx.execute(
                "INSERT INTO cs_guest_sessions(token_digest,verification_id) VALUES($1,$2)",
                &[&token_digest.as_bytes().as_slice(), &id],
            )?;
        }
        tx.commit()?;
        session.ok_or_else(|| GuestError::WrongCode.into())
    }
    /// The live session with this token digest.
    /// # Errors
    /// `Guest(SessionExpired)` for an unknown or expired session.
    pub fn guest_session(
        &self,
        token_digest: SessionTokenDigest,
        clock: impl FnOnce() -> CanonicalTime,
    ) -> Result<GuestSession, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let verification = client
            .query_opt(
                "SELECT v.verification FROM cs_guest_sessions s \
                 JOIN cs_guest_verifications v ON v.id=s.verification_id WHERE s.token_digest=$1",
                &[&token_digest.as_bytes().as_slice()],
            )?
            .ok_or(GuestError::SessionExpired)?
            .try_get::<_, Json<MailboxVerification>>(0)?
            .0;
        Ok(verification.restore_session(token_digest, clock())?)
    }
    /// Every guest writing to a member.
    /// # Errors
    /// Returns database or decoding errors.
    pub fn guest_relationships(&self) -> Result<Vec<GuestRelationship>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        client
            .query(
                "SELECT a.aggregate_key, s.identity, s.subject, \
                        a.relationship_state#>>'{relationship,key,recipient}' \
                 FROM cs_relationship_aggregates a JOIN cs_sender_subjects s \
                   ON s.identity = a.relationship_state#>>'{relationship,key,sender}' \
                 WHERE s.subject ? 'Guest' ORDER BY a.aggregate_key",
                &[],
            )?
            .into_iter()
            .map(|row| {
                let SenderSubject::Guest(mailbox) = row.get::<_, Json<SenderSubject>>(2).0 else {
                    return Err(StorageError::NumericRange);
                };
                let parse = |s: String| {
                    s.parse::<u128>()
                        .map(ProtocolIdentity)
                        .map_err(|_| StorageError::NumericRange)
                };
                Ok(GuestRelationship {
                    aggregate_key: row.get(0),
                    guest: parse(row.get(1))?,
                    mailbox,
                    recipient: parse(row.get(3))?,
                })
            })
            .collect()
    }
    /// Opens a guest's relationship with a recipient under `unit`, as `decide` plans it.
    /// # Errors
    /// The plan's refusals and persistence failures.
    pub fn open_guest_relationship(
        &self,
        approach: &GuestApproach<'_>,
        scope: FinancialScope,
        unit: SettlementUnit,
        clock: impl FnOnce() -> CanonicalTime,
        decide: impl FnOnce(GuestRelationshipContext) -> Result<GuestRelationshipPlan, StorageError>,
    ) -> Result<PostgresEngine, StorageError> {
        GuestTransaction {
            deployment: self,
            scope,
            unit,
        }
        .transact_guest_relationship(approach, clock, decide)?;
        Ok(PostgresEngine {
            aggregate_key: approach.address.aggregate_key().to_owned(),
            client: self.client.clone(),
            deployment: self.clone(),
        })
    }
}

struct GuestTransaction<'a> {
    deployment: &'a PostgresDeployment,
    scope: FinancialScope,
    unit: SettlementUnit,
}
impl GuestStore for GuestTransaction<'_> {
    type Error = StorageError;
    #[allow(clippy::too_many_lines)] // One transaction: every effect of opening commits together.
    fn transact_guest_relationship(
        &self,
        approach: &GuestApproach<'_>,
        clock: impl FnOnce() -> CanonicalTime,
        decide: impl FnOnce(GuestRelationshipContext) -> Result<GuestRelationshipPlan, StorageError>,
    ) -> Result<RequestHistoryRef, StorageError> {
        let recipient = approach.address.recipient();
        let mut client = self
            .deployment
            .client
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        // Ordered with receipts: history merges change what later commands see.
        ingress::require_drained(&mut tx)?;
        let registered = subject_of(&mut tx, approach.identity)?;
        let mailbox_history = linked_history(
            &mut tx,
            recipient,
            &HistoryEvidence::Mailbox(approach.session.mailbox().clone()),
        )?;
        let fingerprint_history = linked_history(
            &mut tx,
            recipient,
            &HistoryEvidence::Fingerprint(approach.fingerprint),
        )?;
        let now = clock();
        if approach
            .session
            .authorize(now)?
            .synthetic_protocol_identity(&self.scope.deployment_domain)
            != approach.identity
            || approach.address.sender() != approach.identity
        {
            return Err(StorageError::SenderSubjectConflict);
        }
        let GuestRelationshipPlan {
            register,
            history,
            at,
        } = decide(GuestRelationshipContext {
            now,
            registered,
            mailbox_history,
            fingerprint_history,
        })?;
        if let Some(subject) = register {
            register_subject(&mut tx, approach.identity, &subject, at)?;
        }
        let now = to_i64(at.0)?;
        if let Some(created) = &history.create {
            tx.execute(
                "INSERT INTO cs_request_histories (request_history, history_state, updated_at) \
                 VALUES ($1, $2, $3)",
                &[&history_key(&created.reference), &Json(created), &now],
            )?;
        }
        if let Some(sources) = history.merged {
            let merged = history_text(&history.history);
            for source in &sources {
                let text = history_text(source);
                tx.execute(
                    "UPDATE cs_guest_history_links SET history=$2 WHERE history=$1",
                    &[&text, &merged],
                )?;
                // Every guest relationship on a merged history moves to the merged one.
                for row in tx.query(
                    "SELECT a.aggregate_key, a.relationship_state FROM cs_relationship_aggregates a \
                     JOIN cs_sender_subjects s \
                       ON s.identity = a.relationship_state#>>'{relationship,key,sender}' \
                     WHERE s.subject ? 'Guest' \
                       AND a.relationship_state#>>'{relationship,key,recipient}'=$1 FOR UPDATE OF a",
                    &[&recipient.0.to_string()],
                )? {
                    let key: String = row.get(0);
                    let record = row.get::<_, Json<domain::RelationshipRecord>>(1).0;
                    if record.history() == *source {
                        tx.execute(
                            "UPDATE cs_relationship_aggregates SET relationship_state=$2 \
                             WHERE aggregate_key=$1",
                            &[&key, &Json(record.with_history(history.history))],
                        )?;
                    }
                }
                // The source keeps its record for provenance; nothing refers to it now.
            }
            tx.execute(
                "INSERT INTO cs_guest_history_merges(merged,sources,evidence,merged_at) \
                 VALUES($1,$2,$3,$4)",
                &[
                    &merged,
                    &Json(sources.map(|s| history_text(&s))),
                    &Json(&history.link),
                    &now,
                ],
            )?;
        }
        for evidence in &history.link {
            tx.execute(
                "INSERT INTO cs_guest_history_links(recipient,evidence,history,linked_at) \
                 VALUES($1,$2,$3,$4) ON CONFLICT (recipient,evidence) DO UPDATE SET history=EXCLUDED.history",
                &[
                    &recipient.0.to_string(),
                    &evidence.key(),
                    &history_text(&history.history),
                    &now,
                ],
            )?;
        }
        let key = guest_source_key(self.scope, approach.identity, &approach.token);
        tx.execute(
            "INSERT INTO cs_funding_sources(source_key,guest,fingerprint,record) VALUES($1,$2,$3,$4) \
             ON CONFLICT DO NOTHING",
            &[
                &key,
                &approach.identity.0.to_string(),
                &approach.fingerprint.0.as_slice(),
                &Json(
                    FundingSource::guest_instrument(approach.token, GUEST_MAXIMUM_UNRESOLVED)
                        .map_err(|e| StorageError::Billing(cs_mail_billing::BillingError::Payment(e)))?,
                ),
            ],
        )?;
        bootstrap_in(
            &mut tx,
            approach.address.aggregate_key(),
            &approach.address.initial_state(history.history, at),
            self.unit,
        )?;
        tx.execute("INSERT INTO cs_guest_email_bindings(aggregate_key,binding) VALUES($1,$2) ON CONFLICT DO NOTHING",
            &[&approach.address.aggregate_key(), &Json(approach.address.public())])?;
        let state: ProtocolState = tx
            .query_one(
                "SELECT relationship_state FROM cs_relationship_aggregates WHERE aggregate_key=$1",
                &[&approach.address.aggregate_key()],
            )?
            .get::<_, Json<domain::RelationshipRecord>>(0)
            .0
            .into_state();
        if state.relationship.history != history.history {
            return Err(StorageError::VersionConflict);
        }
        tx.commit()?;
        Ok(history.history)
    }
}

impl PostgresEngine {
    /// Receives a guest's command, authorized by the guest's verified mailbox session at
    /// the provider's edge. The provider's own key stands for the edge that verified it.
    /// # Errors
    /// Rejects a relationship the guest is not the sender of, and conflicting replay.
    pub fn receive_guest(
        &self,
        command: &cs_mail_guests::GuestCommand,
        provider_key: cs_mail_primitives::OperationalKeyRef,
        deployment: [u8; 32],
        clock: impl FnOnce() -> CanonicalTime,
        policy: cs_mail_protocol::PolicySnapshot,
    ) -> Result<ingress::ReceivedCommand, StorageError> {
        let bytes = serde_json::to_vec(command)?;
        let replay = ingress::ReplayIdentity::trusted(command.idempotency(), &bytes);
        self.receive(
            deployment,
            clock,
            policy,
            replay,
            |tx, key, now, _policy| {
                let state = super::load_locked_aggregate(tx, key)?.state;
                if state.relationship.key.sender != command.identity()
                    || !is_guest(tx, command.identity())?
                {
                    return Err(StorageError::SenderSubjectConflict);
                }
                command.authorize_receipt(&deployment, now)?;
                Ok((
                    command.idempotency(),
                    Sha256::digest(&bytes).into(),
                    ingress::Operation::Protocol(cs_mail_protocol::KernelCommand::new(
                        command.command().clone(),
                        cs_mail_protocol::ActorRef::Sender(command.identity()),
                        provider_key,
                        command.idempotency(),
                    )),
                ))
            },
        )
    }
    /// Stores content the provider encrypted on a guest's behalf, under the provider's
    /// certificate for its guest content key.
    /// # Errors
    /// Rejects a certificate or binding that does not match this guest relationship.
    pub fn store_guest_content(
        &self,
        session: &GuestSession,
        record: &EncryptedContentRecord,
        certificate: &SignedContentKeyCertificate,
        clock: impl FnOnce() -> CanonicalTime,
        policy: &cs_mail_protocol::PolicySnapshot,
        deployment: [u8; 32],
    ) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        ingress::validate_ingress_scope(&mut tx, &self.aggregate_key, deployment, policy)?;
        let relationship = super::load_locked_aggregate(&mut tx, &self.aggregate_key)?
            .state
            .relationship
            .key;
        if !is_guest(&mut tx, relationship.sender)? {
            return Err(StorageError::ContentScopeMismatch);
        }
        let authority = ingress::registry_locked(&mut tx, &self.aggregate_key)?;
        let now = clock();
        if session
            .authorize(now)?
            .synthetic_protocol_identity(&deployment)
            != relationship.sender
        {
            return Err(StorageError::SenderSubjectConflict);
        }
        let digest = authority.verify_guest_content_certificate(
            certificate,
            now,
            policy.protocol_version,
            SigningScope {
                deployment_domain: deployment,
                intended_provider: policy.recipient_provider,
                relationship: relationship.reference,
            },
        )?;
        let c = &certificate.certificate;
        if c.key.reference != record.envelope.sender_key
            || c.owner != relationship.sender
            || record.binding.sender != relationship.sender
            || record.binding.recipient != relationship.recipient
            || record.binding.relationship != relationship.reference
            || record.binding.sender_certificate != digest
            || record.binding.declarations.origin.authority
                != DeclarationAuthority::GuestGateway(policy.recipient_provider)
        {
            return Err(StorageError::ContentScopeMismatch);
        }
        super::persist_content(
            &mut tx,
            &self.aggregate_key,
            record,
            policy.retention_policy_version,
        )?;
        super::members::record_certificate(&mut tx, &digest.0, certificate)?;
        tx.commit()?;
        Ok(())
    }
    /// Whether ciphertext is stored under `content_ref` in this relationship.
    /// # Errors
    /// Returns database errors.
    pub fn has_content(
        &self,
        content_ref: cs_mail_primitives::ContentRef,
    ) -> Result<bool, StorageError> {
        Ok(self.content(content_ref)?.is_some())
    }
}
