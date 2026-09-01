//! `PostgreSQL` durability for the cs-mail transition kernel.
//!
//! Each command locks one relationship aggregate, evaluates the pure kernel,
//! and atomically commits protocol state, ledger projections, journal records,
//! idempotency evidence, schedules, and outbox intents.
//!
//! Caller-constructed authorization cannot be executed through the durable API:
//!
//! ```compile_fail
//! use cs_mail_primitives::CanonicalTime;
//! use cs_mail_protocol::{Authorized, PolicySnapshot, ProtocolCommand};
//! use cs_mail_storage_postgres::PostgresEngine;
//!
//! fn bypass(
//!     engine: &PostgresEngine,
//!     command: Authorized<ProtocolCommand>,
//!     now: CanonicalTime,
//!     policy: PolicySnapshot,
//! ) {
//!     engine.execute(command, now, policy).unwrap();
//! }
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

use cs_mail_capabilities::{
    BondFreeAdmission, CapabilityError, Lane, LaneControl, LaneControlAction, LaneHorizonEffect,
    LaneState, SignedLaneGrant,
};
use cs_mail_content::{ContentBinding, EncryptedContentRecord, message_declaration_digest};
use cs_mail_ledger::{Account, LedgerError, LedgerView};
use cs_mail_primitives::{
    CanonicalTime, ContentRef, Duration, IdempotencyKey, JournalPosition, LaneId, MessageId, Money,
    OperationalKeyRef, ProtocolVersion, ProviderRef, RetentionPolicyVersion, ScheduleChange,
    ScheduleTask, SettlementUnit, Version,
};
use cs_mail_protocol::{
    ActorRef, Authorized, EffectIntent, PolicySnapshot, ProtocolCommand, ProtocolError,
    ProtocolState, RelationshipState, RepeatedAttemptState, SettlementSnapshot, TermsOutcome,
    TransitionContext, TransitionManifest, transition,
};
use cs_mail_security::{
    KeyRegistry, SecurityError, SignedCommandBytes, SignedContactTerms, SignedReceipt, SigningScope,
};
use postgres::types::Json;
use postgres::{Client, IsolationLevel, NoTls, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

const MIGRATION_1: &str = include_str!("../migrations/0001_initial.sql");
const MIGRATION_2: &str = include_str!("../migrations/0002_encrypted_content.sql");
const MIGRATION_3: &str = include_str!("../migrations/0003_outbox_content_retention.sql");
const MIGRATION_4: &str = include_str!("../migrations/0004_express_lanes.sql");
const MIGRATION_5: &str = include_str!("../migrations/0005_protocol_foundations.sql");
const MIGRATION_6: &str = include_str!("../migrations/0006_authenticated_message_envelopes.sql");
const CURRENT_PROTOCOL_FORMAT_VERSION: i16 = 2;

#[derive(Debug)]
pub enum StorageError {
    Database(postgres::Error),
    Protocol(ProtocolError),
    Ledger(LedgerError),
    Serialization(serde_json::Error),
    DuplicateConflict,
    VersionConflict,
    NumericRange,
    LockPoisoned,
    Security(SecurityError),
    ContentMissing,
    ContentScopeMismatch,
    ContentExpired,
    MessageValidityClosed,
    QuoteMissing,
    QuoteNotSigned,
    RegistryMissing,
    Capability(CapabilityError),
    BondFreeNotAuthorized,
    InvalidScheduleClaim,
    UnsupportedStoredProtocolFormat(i16),
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "database error: {error}"),
            Self::Protocol(error) => write!(formatter, "protocol error: {error:?}"),
            Self::Ledger(error) => write!(formatter, "ledger error: {error:?}"),
            Self::Serialization(error) => write!(formatter, "serialization error: {error}"),
            Self::DuplicateConflict => {
                formatter.write_str("idempotency key reused with another request")
            }
            Self::VersionConflict => formatter.write_str("aggregate revision changed"),
            Self::NumericRange => {
                formatter.write_str("value exceeds the PostgreSQL representation")
            }
            Self::LockPoisoned => formatter.write_str("database client lock poisoned"),
            Self::Security(error) => write!(formatter, "security error: {error}"),
            Self::ContentMissing => formatter.write_str("encrypted content is not durable"),
            Self::ContentScopeMismatch => {
                formatter.write_str("encrypted content does not match the admitted attempt")
            }
            Self::ContentExpired => formatter.write_str("encrypted content has expired"),
            Self::MessageValidityClosed => {
                formatter.write_str("message validity closed before admission")
            }
            Self::QuoteMissing => {
                formatter.write_str("contact terms were not issued by this provider")
            }
            Self::QuoteNotSigned => {
                formatter.write_str("contact terms do not have provider evidence")
            }
            Self::RegistryMissing => formatter.write_str("durable key registry is not initialized"),
            Self::Capability(error) => write!(formatter, "capability error: {error}"),
            Self::BondFreeNotAuthorized => formatter
                .write_str("no accepted relationship or valid express lane authorizes delivery"),
            Self::InvalidScheduleClaim => {
                formatter.write_str("scheduled work was not claimed from this aggregate")
            }
            Self::UnsupportedStoredProtocolFormat(version) => write!(
                formatter,
                "stored protocol format version {version} requires an explicit migration"
            ),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<postgres::Error> for StorageError {
    fn from(value: postgres::Error) -> Self {
        Self::Database(value)
    }
}

impl From<ProtocolError> for StorageError {
    fn from(value: ProtocolError) -> Self {
        Self::Protocol(value)
    }
}

impl From<LedgerError> for StorageError {
    fn from(value: LedgerError) -> Self {
        Self::Ledger(value)
    }
}

impl From<serde_json::Error> for StorageError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialization(value)
    }
}

impl From<SecurityError> for StorageError {
    fn from(value: SecurityError) -> Self {
        Self::Security(value)
    }
}

impl From<CapabilityError> for StorageError {
    fn from(value: CapabilityError) -> Self {
        Self::Capability(value)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct StoredRequest {
    actor: ActorRef,
    operational_key: OperationalKeyRef,
    command: ProtocolCommand,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurableExecutionOutcome {
    pub manifest: TransitionManifest,
    pub replayed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboxItem {
    pub id: i64,
    pub aggregate_key: String,
    pub journal_position: JournalPosition,
    pub payload: EffectIntent,
}

#[derive(Debug, Eq, PartialEq)]
pub struct ScheduledItem {
    aggregate_key: String,
    task: ScheduleTask,
    due_at: CanonicalTime,
    claim_until: CanonicalTime,
}

impl ScheduledItem {
    pub const fn task(&self) -> ScheduleTask {
        self.task
    }

    pub const fn due_at(&self) -> CanonicalTime {
        self.due_at
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BondFreeAuthority {
    AcceptedRelationship,
    ExpressLane(LaneId),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BondFreeAdmissionOutcome {
    pub authority: BondFreeAuthority,
    pub journal_position: JournalPosition,
    pub replayed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneOperationOutcome {
    pub lane: Lane,
    pub replayed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
enum CapabilityEvent {
    LaneGranted(LaneId),
    LaneRevoked(LaneId),
    LaneReconfirmed(LaneId),
    LaneMessageAdmitted(LaneId, MessageId),
    AcceptedMessageAdmitted(MessageId),
    LaneReconfirmationRequired(LaneId),
    LaneReviewReminder(LaneId),
}

pub struct PostgresEngine {
    aggregate_key: String,
    client: Mutex<Client>,
}

impl PostgresEngine {
    /// Connects without transport TLS, migrates, and bootstraps an aggregate.
    /// Production callers should create a TLS-configured `Client` and use `from_client`.
    ///
    /// # Errors
    ///
    /// Returns an error for connection, migration, serialization, or numeric
    /// conversion failures.
    pub fn connect(
        database_url: &str,
        aggregate_key: impl Into<String>,
        initial_state: &ProtocolState,
        unit: SettlementUnit,
        sender_balance: Money,
    ) -> Result<Self, StorageError> {
        let client = Client::connect(database_url, NoTls)?;
        Self::from_client(client, aggregate_key, initial_state, unit, sender_balance)
    }

    /// Builds an engine from a caller-configured client, including a TLS client
    /// created outside this crate for production deployments.
    ///
    /// # Errors
    ///
    /// Returns an error for migration, bootstrap, serialization, or database failures.
    pub fn from_client(
        mut client: Client,
        aggregate_key: impl Into<String>,
        initial_state: &ProtocolState,
        unit: SettlementUnit,
        sender_balance: Money,
    ) -> Result<Self, StorageError> {
        migrate(&mut client)?;
        let aggregate_key = aggregate_key.into();
        bootstrap(
            &mut client,
            &aggregate_key,
            initial_state,
            unit,
            sender_balance,
        )?;
        Ok(Self {
            aggregate_key,
            client: Mutex::new(client),
        })
    }

    /// Installs the initial durable key authority without replacing an existing registry.
    ///
    /// # Errors
    ///
    /// Returns an error for database, serialization, or numeric failures.
    pub fn initialize_key_registry(
        &self,
        registry: &KeyRegistry,
        now: CanonicalTime,
    ) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        client.execute(
            "INSERT INTO cs_key_registries \
             (aggregate_key, registry_version, registry, updated_at) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (aggregate_key) DO NOTHING",
            &[
                &self.aggregate_key,
                &to_i64(registry.version().0)?,
                &Json(registry),
                &to_i64(now.0)?,
            ],
        )?;
        Ok(())
    }

    /// Loads the durable operational-key authority and transparency projection.
    ///
    /// # Errors
    ///
    /// Returns an error when the registry is absent or cannot be decoded.
    pub fn key_registry(&self) -> Result<KeyRegistry, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        client
            .query_opt(
                "SELECT registry FROM cs_key_registries WHERE aggregate_key = $1",
                &[&self.aggregate_key],
            )?
            .map(|row| row.get::<_, Json<KeyRegistry>>("registry").0)
            .ok_or(StorageError::RegistryMissing)
    }

    /// Registers a key under a row lock so restarts and competing service instances agree.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid or duplicate keys, lock failures, or database failures.
    pub fn register_operational_key(
        &self,
        reference: OperationalKeyRef,
        actor: ActorRef,
        verifying_key: [u8; 32],
        now: CanonicalTime,
    ) -> Result<(), StorageError> {
        self.update_key_registry(now, |registry| {
            registry.register(reference, actor, verifying_key, now)
        })
    }

    /// Revokes a key under the same durable authority used by command verification.
    ///
    /// # Errors
    ///
    /// Returns an error for missing/stale keys, lock failures, or database failures.
    pub fn revoke_operational_key(
        &self,
        reference: OperationalKeyRef,
        expected_version: Version,
        now: CanonicalTime,
    ) -> Result<(), StorageError> {
        self.update_key_registry(now, |registry| {
            registry.revoke(reference, expected_version, now)
        })
    }

    fn update_key_registry<F>(&self, now: CanonicalTime, update: F) -> Result<(), StorageError>
    where
        F: FnOnce(&mut KeyRegistry) -> Result<(), SecurityError>,
    {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut transaction = client.transaction()?;
        let row = transaction
            .query_opt(
                "SELECT registry FROM cs_key_registries \
                 WHERE aggregate_key = $1 FOR UPDATE",
                &[&self.aggregate_key],
            )?
            .ok_or(StorageError::RegistryMissing)?;
        let mut registry = row.get::<_, Json<KeyRegistry>>("registry").0;
        update(&mut registry)?;
        transaction.execute(
            "UPDATE cs_key_registries SET registry_version = $2, registry = $3, updated_at = $4 \
             WHERE aggregate_key = $1",
            &[
                &self.aggregate_key,
                &to_i64(registry.version().0)?,
                &Json(&registry),
                &to_i64(now.0)?,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Adds the provider signature to terms already committed by `IssueContactTerms`.
    ///
    /// # Errors
    ///
    /// Returns an error if the quote is absent, differs, or cannot be stored.
    pub fn attach_signed_quote(&self, quote: &SignedContactTerms) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let changed = client.execute(
            "UPDATE cs_contact_quotes SET provider_operational_key = $3, signature = $4 \
             WHERE aggregate_key = $1 AND quote_id = $2 AND terms = $5",
            &[
                &self.aggregate_key,
                &quote.terms.quote_id.0.to_string(),
                &quote.provider_operational_key.0.to_string(),
                &&quote.signature[..],
                &Json(&quote.terms),
            ],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            Err(StorageError::QuoteMissing)
        }
    }

    /// Stores an immutable signed receipt; exact replay is idempotent.
    ///
    /// # Errors
    ///
    /// Returns an error for a conflicting receipt or database failure.
    pub fn store_receipt(&self, receipt: &SignedReceipt) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let inserted = client.execute(
            "INSERT INTO cs_provider_receipts \
             (aggregate_key, receipt_id, journal_position, payload, provider_operational_key, \
              signature, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (aggregate_key, receipt_id) DO NOTHING",
            &[
                &self.aggregate_key,
                &receipt.payload.receipt_id.0.to_string(),
                &to_i64(receipt.payload.journal_position.0)?,
                &Json(&receipt.payload),
                &receipt.provider_operational_key.0.to_string(),
                &&receipt.signature[..],
                &to_i64(receipt.payload.received_at.0)?,
            ],
        )?;
        if inserted == 1 {
            return Ok(());
        }
        let row = client.query_one(
            "SELECT payload, provider_operational_key, signature FROM cs_provider_receipts \
             WHERE aggregate_key = $1 AND receipt_id = $2",
            &[
                &self.aggregate_key,
                &receipt.payload.receipt_id.0.to_string(),
            ],
        )?;
        let signature: Vec<u8> = row.get("signature");
        if row
            .get::<_, Json<cs_mail_security::ReceiptPayload>>("payload")
            .0
            == receipt.payload
            && row.get::<_, String>("provider_operational_key")
                == receipt.provider_operational_key.0.to_string()
            && signature.as_slice() == receipt.signature
        {
            Ok(())
        } else {
            Err(StorageError::DuplicateConflict)
        }
    }

    fn execute_authorized(
        &self,
        authorized: Authorized<ProtocolCommand>,
        now: CanonicalTime,
        policy: PolicySnapshot,
        expected_registry_version: Option<Version>,
        require_signed_quote: bool,
    ) -> Result<DurableExecutionOutcome, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut transaction = client
            .build_transaction()
            .isolation_level(IsolationLevel::ReadCommitted)
            .start()?;
        let aggregate = load_locked_aggregate(&mut transaction, &self.aggregate_key)?;
        if let Some(expected) = expected_registry_version {
            lock_registry_version(&mut transaction, &self.aggregate_key, expected)?;
        }
        let idempotency_key = authorized.idempotency_key();
        let request = StoredRequest {
            actor: authorized.actor(),
            operational_key: authorized.operational_key(),
            command: authorized.command().clone(),
        };
        let blocks_relationship = matches!(
            authorized.command(),
            ProtocolCommand::BlockRelationship { .. }
        );
        if let Some(outcome) = load_idempotent_result(
            &mut transaction,
            &self.aggregate_key,
            idempotency_key,
            &request,
        )? {
            transaction.commit()?;
            return Ok(outcome);
        }
        let balances = load_balances(&mut transaction, &self.aggregate_key)?;
        let ledger = LedgerView::from_balances(aggregate.ledger_revision, aggregate.unit, balances);
        let snapshot = SettlementSnapshot::complete(aggregate.revision, aggregate.state, ledger);
        validate_issued_quote(
            &mut transaction,
            &self.aggregate_key,
            authorized.command(),
            require_signed_quote,
        )?;
        let next_revision = aggregate
            .revision
            .checked_add(1)
            .ok_or(StorageError::NumericRange)?;
        let journal_position = allocate_journal_position(
            &mut transaction,
            &self.aggregate_key,
            next_revision,
            "protocol",
            now,
        )?;
        let context = TransitionContext {
            now,
            journal_position,
            protocol_version: policy.protocol_version,
            policy,
        };
        let manifest = transition(&snapshot, &authorized, &context)?;
        validate_content_for_manifest(
            &mut transaction,
            &self.aggregate_key,
            &snapshot.state,
            authorized.command(),
            &manifest,
            now,
            context.protocol_version,
        )?;
        let _authorized_evidence = authorized.into_parts();
        let next_ledger = snapshot.ledger.apply(&manifest.ledger_batch)?;
        persist_manifest(
            &mut transaction,
            &self.aggregate_key,
            &manifest,
            &next_ledger,
            &request,
            idempotency_key,
            context.now,
            context.journal_position,
        )?;
        if blocks_relationship {
            revoke_lane_locked(
                &mut transaction,
                &self.aggregate_key,
                context.now,
                context.journal_position,
            )?;
        }
        transaction.commit()?;
        Ok(DurableExecutionOutcome {
            manifest,
            replayed: false,
        })
    }

    /// Verifies and durably executes one signed command.
    ///
    /// # Errors
    ///
    /// Returns an error when signature/key validation or durable execution
    /// preconditions fail.
    pub fn execute_signed(
        &self,
        registry: &KeyRegistry,
        signed: &SignedCommandBytes,
        deployment_domain: [u8; 32],
        now: CanonicalTime,
        policy: PolicySnapshot,
    ) -> Result<DurableExecutionOutcome, StorageError> {
        self.initialize_key_registry(registry, now)?;
        let durable_registry = self.key_registry()?;
        let registry_version = durable_registry.version();
        let verified = durable_registry.verify(
            signed,
            now,
            policy.protocol_version,
            SigningScope {
                deployment_domain,
                intended_provider: policy.recipient_provider,
                relationship: self.snapshot()?.state.relationship.key.reference,
            },
        )?;
        self.execute_authorized(
            verified.into_authorized(),
            now,
            policy,
            Some(registry_version),
            true,
        )
    }

    /// Verifies and atomically stores a recipient-signed express lane.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid signature, mismatched relationship,
    /// blocked pair, conflicting replay, or database failure.
    pub fn grant_lane(
        &self,
        signed: &SignedLaneGrant,
        recipient_key: &[u8; 32],
        idempotency_key: IdempotencyKey,
        now: CanonicalTime,
    ) -> Result<LaneOperationOutcome, StorageError> {
        let lane = Lane::from_verified_grant(signed, recipient_key)?;
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut transaction = client.transaction()?;
        let aggregate = load_locked_aggregate(&mut transaction, &self.aggregate_key)?;
        if aggregate.state.relationship.state == RelationshipState::Blocked {
            return Err(StorageError::Protocol(ProtocolError::ContactBlocked));
        }
        if lane.grant.sender != aggregate.state.relationship.key.sender
            || lane.grant.recipient != aggregate.state.relationship.key.recipient
        {
            return Err(StorageError::BondFreeNotAuthorized);
        }
        let request = serde_json::to_value(&lane)?;
        if let Some(row) = transaction.query_opt(
            "SELECT request, outcome FROM cs_capability_idempotency \
             WHERE aggregate_key = $1 AND idempotency_key = $2",
            &[&self.aggregate_key, &idempotency_key.0.to_string()],
        )? {
            if row.get::<_, Json<serde_json::Value>>("request").0 != request {
                return Err(StorageError::DuplicateConflict);
            }
            let replayed_lane = row.get::<_, Json<Lane>>("outcome").0;
            transaction.commit()?;
            return Ok(LaneOperationOutcome {
                lane: replayed_lane,
                replayed: true,
            });
        }
        if let Some(row) = transaction.query_opt(
            "SELECT lane FROM cs_capability_lanes WHERE aggregate_key = $1 FOR UPDATE",
            &[&self.aggregate_key],
        )? {
            let existing = row.get::<_, Json<Lane>>("lane").0;
            if existing != lane
                && (existing.grant.id != lane.grant.id
                    || lane.grant.version <= existing.grant.version)
            {
                return Err(StorageError::DuplicateConflict);
            }
        }
        let position = advance_aggregate_revision(
            &mut transaction,
            &self.aggregate_key,
            aggregate.revision,
            now,
        )?;
        upsert_lane(&mut transaction, &self.aggregate_key, &lane, now)?;
        persist_schedules(
            &mut transaction,
            &self.aggregate_key,
            &[lane.schedule_change()],
        )?;
        insert_capability_event(
            &mut transaction,
            &self.aggregate_key,
            position,
            &CapabilityEvent::LaneGranted(lane.grant.id),
        )?;
        transaction.execute(
            "INSERT INTO cs_capability_idempotency \
             (aggregate_key, idempotency_key, request, outcome, created_at) \
             VALUES ($1, $2, $3, $4, $5)",
            &[
                &self.aggregate_key,
                &idempotency_key.0.to_string(),
                &Json(&request),
                &Json(&lane),
                &to_i64(now.0)?,
            ],
        )?;
        transaction.commit()?;
        Ok(LaneOperationOutcome {
            lane,
            replayed: false,
        })
    }

    /// Applies one already-authenticated recipient lane control atomically.
    ///
    /// # Errors
    ///
    /// Returns an error for mismatched relationship, stale version, invalid
    /// state, conflicting replay, or database failure.
    pub fn control_lane(
        &self,
        control: &LaneControl,
        now: CanonicalTime,
    ) -> Result<LaneOperationOutcome, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut transaction = client.transaction()?;
        let aggregate = load_locked_aggregate(&mut transaction, &self.aggregate_key)?;
        if control.recipient != aggregate.state.relationship.key.recipient {
            return Err(StorageError::BondFreeNotAuthorized);
        }
        let request = serde_json::to_value(control)?;
        if let Some(row) = transaction.query_opt(
            "SELECT request, outcome FROM cs_capability_idempotency \
             WHERE aggregate_key = $1 AND idempotency_key = $2",
            &[&self.aggregate_key, &control.idempotency_key.0.to_string()],
        )? {
            if row.get::<_, Json<serde_json::Value>>("request").0 != request {
                return Err(StorageError::DuplicateConflict);
            }
            let lane = row.get::<_, Json<Lane>>("outcome").0;
            transaction.commit()?;
            return Ok(LaneOperationOutcome {
                lane,
                replayed: true,
            });
        }
        let row = transaction
            .query_opt(
                "SELECT lane FROM cs_capability_lanes WHERE aggregate_key = $1 FOR UPDATE",
                &[&self.aggregate_key],
            )?
            .ok_or(StorageError::Capability(CapabilityError::MissingLane))?;
        let mut lane = row.get::<_, Json<Lane>>("lane").0;
        if lane.grant.id != control.lane_id || lane.version != control.expected_version {
            return Err(StorageError::VersionConflict);
        }
        let event = match control.action {
            LaneControlAction::Reconfirm => {
                if aggregate.state.relationship.state == RelationshipState::Blocked {
                    return Err(StorageError::Protocol(ProtocolError::ContactBlocked));
                }
                lane.reconfirm(now)?;
                CapabilityEvent::LaneReconfirmed(lane.grant.id)
            }
            LaneControlAction::Revoke => {
                lane.revoke()?;
                CapabilityEvent::LaneRevoked(lane.grant.id)
            }
        };
        let position = advance_aggregate_revision(
            &mut transaction,
            &self.aggregate_key,
            aggregate.revision,
            now,
        )?;
        upsert_lane(&mut transaction, &self.aggregate_key, &lane, now)?;
        match control.action {
            LaneControlAction::Reconfirm => persist_schedules(
                &mut transaction,
                &self.aggregate_key,
                &[lane.schedule_change()],
            )?,
            LaneControlAction::Revoke => delete_schedule(
                &mut transaction,
                &self.aggregate_key,
                ScheduleTask::LaneHorizon(lane.grant.id),
            )?,
        }
        insert_capability_event(&mut transaction, &self.aggregate_key, position, &event)?;
        transaction.execute(
            "INSERT INTO cs_capability_idempotency \
             (aggregate_key, idempotency_key, request, outcome, created_at) \
             VALUES ($1, $2, $3, $4, $5)",
            &[
                &self.aggregate_key,
                &control.idempotency_key.0.to_string(),
                &Json(&request),
                &Json(&lane),
                &to_i64(now.0)?,
            ],
        )?;
        transaction.commit()?;
        Ok(LaneOperationOutcome {
            lane,
            replayed: false,
        })
    }

    /// Delivers encrypted content without settlement when an accepted
    /// relationship or express lane authorizes it.
    ///
    /// # Errors
    ///
    /// Returns an error for blocked or unauthorized contact, invalid content
    /// binding, a consumed rate allowance, conflicting replay, or database failure.
    #[allow(clippy::too_many_lines)]
    pub fn admit_bond_free(
        &self,
        request: &BondFreeAdmission,
        now: CanonicalTime,
    ) -> Result<BondFreeAdmissionOutcome, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut transaction = client.transaction()?;
        let aggregate = load_locked_aggregate(&mut transaction, &self.aggregate_key)?;
        let request_value = serde_json::to_value(request)?;
        if let Some(row) = transaction.query_opt(
            "SELECT request, outcome FROM cs_capability_idempotency \
             WHERE aggregate_key = $1 AND idempotency_key = $2",
            &[&self.aggregate_key, &request.idempotency_key.0.to_string()],
        )? {
            if row.get::<_, Json<serde_json::Value>>("request").0 != request_value {
                return Err(StorageError::DuplicateConflict);
            }
            let mut outcome = row.get::<_, Json<BondFreeAdmissionOutcome>>("outcome").0;
            outcome.replayed = true;
            transaction.commit()?;
            return Ok(outcome);
        }
        if aggregate.state.relationship.state == RelationshipState::Blocked {
            return Err(StorageError::Protocol(ProtocolError::ContactBlocked));
        }
        if request.sender != aggregate.state.relationship.key.sender
            || request.recipient != aggregate.state.relationship.key.recipient
        {
            return Err(StorageError::BondFreeNotAuthorized);
        }
        let binding = validate_content_for_delivery(
            &mut transaction,
            &self.aggregate_key,
            &aggregate.state,
            request.message_id,
            request.content_ref,
            now,
            request.protocol_version,
        )?;
        if binding.declarations != request.declarations
            || binding.message_valid_until != request.message_valid_until
            || binding.capability != request.capability
        {
            return Err(StorageError::ContentScopeMismatch);
        }
        let (authority, event, schedule) =
            if aggregate.state.relationship.state == RelationshipState::Accepted {
                if request.capability.is_some() || request.evidence.is_some() {
                    return Err(StorageError::BondFreeNotAuthorized);
                }
                (
                    BondFreeAuthority::AcceptedRelationship,
                    CapabilityEvent::AcceptedMessageAdmitted(request.message_id),
                    None,
                )
            } else {
                let evidence = request
                    .evidence
                    .as_ref()
                    .ok_or(StorageError::BondFreeNotAuthorized)?;
                let row = transaction
                    .query_opt(
                        "SELECT lane FROM cs_capability_lanes \
                         WHERE aggregate_key = $1 FOR UPDATE",
                        &[&self.aggregate_key],
                    )?
                    .ok_or(StorageError::BondFreeNotAuthorized)?;
                let mut lane = row.get::<_, Json<Lane>>("lane").0;
                lane.authorizes(
                    aggregate.state.relationship.key.sender,
                    aggregate.state.relationship.key.recipient,
                    request.capability,
                    &request.declarations,
                    evidence,
                    now,
                )?;
                let replayed = lane.consume(
                    request.message_id,
                    request.capability,
                    &request.declarations,
                    evidence,
                    now,
                )?;
                if replayed {
                    return Err(StorageError::DuplicateConflict);
                }
                upsert_lane(&mut transaction, &self.aggregate_key, &lane, now)?;
                (
                    BondFreeAuthority::ExpressLane(lane.grant.id),
                    CapabilityEvent::LaneMessageAdmitted(lane.grant.id, request.message_id),
                    Some(lane.schedule_change()),
                )
            };
        let position = advance_aggregate_revision(
            &mut transaction,
            &self.aggregate_key,
            aggregate.revision,
            now,
        )?;
        if let Some(change) = schedule {
            persist_schedules(&mut transaction, &self.aggregate_key, &[change])?;
        }
        let intent = EffectIntent::DeliverMessage {
            message_id: request.message_id,
            content_ref: request.content_ref,
            delivery_intent_ref: request.delivery_intent_ref,
        };
        insert_outbox_intent(
            &mut transaction,
            &self.aggregate_key,
            position,
            now,
            &intent,
        )?;
        insert_capability_event(&mut transaction, &self.aggregate_key, position, &event)?;
        let outcome = BondFreeAdmissionOutcome {
            authority,
            journal_position: position,
            replayed: false,
        };
        transaction.execute(
            "INSERT INTO cs_capability_idempotency \
             (aggregate_key, idempotency_key, request, outcome, created_at) \
             VALUES ($1, $2, $3, $4, $5)",
            &[
                &self.aggregate_key,
                &request.idempotency_key.0.to_string(),
                &Json(&request_value),
                &Json(&outcome),
                &to_i64(now.0)?,
            ],
        )?;
        transaction.commit()?;
        Ok(outcome)
    }

    /// Applies one due lane horizon under the aggregate lock.
    ///
    /// # Errors
    ///
    /// Returns an error for database, serialization, or arithmetic failures.
    fn process_lane_horizon(
        &self,
        lane_id: LaneId,
        now: CanonicalTime,
    ) -> Result<bool, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut transaction = client.transaction()?;
        let aggregate = load_locked_aggregate(&mut transaction, &self.aggregate_key)?;
        let Some(row) = transaction.query_opt(
            "SELECT lane FROM cs_capability_lanes WHERE aggregate_key = $1 FOR UPDATE",
            &[&self.aggregate_key],
        )?
        else {
            delete_schedule(
                &mut transaction,
                &self.aggregate_key,
                ScheduleTask::LaneHorizon(lane_id),
            )?;
            transaction.commit()?;
            return Ok(false);
        };
        let mut lane = row.get::<_, Json<Lane>>("lane").0;
        if lane.grant.id != lane_id || lane.state == LaneState::Revoked {
            delete_schedule(
                &mut transaction,
                &self.aggregate_key,
                ScheduleTask::LaneHorizon(lane_id),
            )?;
            transaction.commit()?;
            return Ok(false);
        }
        let effect = lane.reach_horizon(now)?;
        if effect == LaneHorizonEffect::None {
            persist_schedules(
                &mut transaction,
                &self.aggregate_key,
                &[lane.schedule_change()],
            )?;
            transaction.commit()?;
            return Ok(false);
        }
        let position = advance_aggregate_revision(
            &mut transaction,
            &self.aggregate_key,
            aggregate.revision,
            now,
        )?;
        upsert_lane(&mut transaction, &self.aggregate_key, &lane, now)?;
        let (reconfirmation_required, event) = match effect {
            LaneHorizonEffect::ReconfirmationRequired => {
                (true, CapabilityEvent::LaneReconfirmationRequired(lane_id))
            }
            LaneHorizonEffect::ReviewReminder => {
                persist_schedules(
                    &mut transaction,
                    &self.aggregate_key,
                    &[lane.schedule_change()],
                )?;
                (false, CapabilityEvent::LaneReviewReminder(lane_id))
            }
            LaneHorizonEffect::None => unreachable!(),
        };
        if reconfirmation_required {
            delete_schedule(
                &mut transaction,
                &self.aggregate_key,
                ScheduleTask::LaneHorizon(lane_id),
            )?;
        }
        insert_outbox_intent(
            &mut transaction,
            &self.aggregate_key,
            position,
            now,
            &EffectIntent::ReviewLane {
                lane_id,
                reconfirmation_required,
            },
        )?;
        insert_capability_event(&mut transaction, &self.aggregate_key, position, &event)?;
        transaction.commit()?;
        Ok(true)
    }

    /// Returns the current lane for this directed relationship.
    ///
    /// # Errors
    ///
    /// Returns an error for database, decoding, or lock failures.
    pub fn lane(&self) -> Result<Option<Lane>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        Ok(client
            .query_opt(
                "SELECT lane FROM cs_capability_lanes WHERE aggregate_key = $1",
                &[&self.aggregate_key],
            )?
            .map(|row| row.get::<_, Json<Lane>>("lane").0))
    }

    /// Stores one endpoint-encrypted object. Exact replay is idempotent.
    ///
    /// # Errors
    ///
    /// Returns an error for conflicting references, invalid scope, or database failures.
    pub fn store_content(&self, record: &EncryptedContentRecord) -> Result<(), StorageError> {
        self.store_content_with_retention(record, RetentionPolicyVersion(0))
    }

    /// Stores ciphertext and its versioned deletion obligation in one transaction.
    ///
    /// # Errors
    ///
    /// Returns an error for a conflicting record, invalid numeric value, or database failure.
    pub fn store_content_with_retention(
        &self,
        record: &EncryptedContentRecord,
        retention_policy_version: RetentionPolicyVersion,
    ) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut transaction = client.transaction()?;
        let content_ref = record.binding.content_ref.0.to_string();
        let existing = transaction.query_opt(
            "SELECT record FROM cs_encrypted_content \
             WHERE aggregate_key = $1 AND content_ref = $2",
            &[&self.aggregate_key, &content_ref],
        )?;
        if let Some(row) = existing {
            return if row.get::<_, Json<EncryptedContentRecord>>("record").0 == *record {
                transaction.commit()?;
                Ok(())
            } else {
                Err(StorageError::DuplicateConflict)
            };
        }
        let relationship = self.snapshot_relationship_ref(&mut transaction)?;
        let record_ref = retention_record_key(relationship.as_bytes(), "content", &content_ref);
        transaction.execute(
            "INSERT INTO cs_encrypted_content \
             (aggregate_key, content_ref, record, created_at, expires_at, \
              retention_policy_version, record_ref) VALUES ($1, $2, $3, $4, $5, $6, $7)",
            &[
                &self.aggregate_key,
                &content_ref,
                &Json(record),
                &to_i64(record.created_at.0)?,
                &to_i64(record.expires_at.0)?,
                &to_i64(u64::from(retention_policy_version.0))?,
                &record_ref,
            ],
        )?;
        transaction.execute(
            "INSERT INTO cs_retention_records \
             (record_ref, aggregate_key, record_domain, object_ref, policy_version, \
              delete_after, state, created_at) VALUES ($1, $2, 'content', $3, $4, $5, 'active', $6)",
            &[
                &record_ref,
                &self.aggregate_key,
                &content_ref,
                &to_i64(u64::from(retention_policy_version.0))?,
                &to_i64(record.expires_at.0)?,
                &to_i64(record.created_at.0)?,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn snapshot_relationship_ref(
        &self,
        transaction: &mut Transaction<'_>,
    ) -> Result<cs_mail_primitives::RelationshipRef, StorageError> {
        let row = transaction.query_one(
            "SELECT protocol_state FROM cs_relationship_aggregates WHERE aggregate_key = $1",
            &[&self.aggregate_key],
        )?;
        Ok(row
            .get::<_, Json<ProtocolState>>("protocol_state")
            .0
            .relationship
            .key
            .reference)
    }

    /// Loads ciphertext without exposing any decryption key to the provider.
    ///
    /// # Errors
    ///
    /// Returns an error for database, decoding, or lock failures.
    pub fn content(
        &self,
        content_ref: cs_mail_primitives::ContentRef,
    ) -> Result<Option<EncryptedContentRecord>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        Ok(client
            .query_opt(
                "SELECT record FROM cs_encrypted_content \
                 WHERE aggregate_key = $1 AND content_ref = $2",
                &[&self.aggregate_key, &content_ref.0.to_string()],
            )?
            .map(|row| row.get::<_, Json<EncryptedContentRecord>>("record").0))
    }

    /// Deletes expired ciphertext records and returns the number removed.
    ///
    /// # Errors
    ///
    /// Returns an error for database, numeric, or lock failures.
    pub fn purge_expired_content(&self, now: CanonicalTime) -> Result<u64, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut transaction = client.transaction()?;
        let rows = transaction.query(
            "SELECT content.content_ref, content.record_ref, content.retention_policy_version \
             FROM cs_encrypted_content AS content \
             WHERE content.aggregate_key = $1 AND content.expires_at <= $2 \
             AND NOT EXISTS (SELECT 1 FROM cs_outbox AS outbox \
                 WHERE outbox.aggregate_key = content.aggregate_key \
                 AND outbox.content_ref = content.content_ref \
                 AND outbox.status IN ('pending', 'processing')) FOR UPDATE",
            &[&self.aggregate_key, &to_i64(now.0)?],
        )?;
        for row in &rows {
            let content_ref: String = row.get("content_ref");
            let record_ref: Vec<u8> = row.get("record_ref");
            let policy_version: i64 = row.get("retention_policy_version");
            transaction.execute(
                "INSERT INTO cs_deletion_manifests \
                 (record_ref, aggregate_key, record_domain, object_ref, policy_version, \
                  deleted_at, reason) VALUES ($1, $2, 'content', $3, $4, $5, 'retention-expired')",
                &[
                    &record_ref,
                    &self.aggregate_key,
                    &content_ref,
                    &policy_version,
                    &to_i64(now.0)?,
                ],
            )?;
            transaction.execute(
                "UPDATE cs_retention_records SET state = 'deleted' WHERE record_ref = $1",
                &[&record_ref],
            )?;
        }
        let deleted = transaction.execute(
            "DELETE FROM cs_encrypted_content AS content \
             WHERE content.aggregate_key = $1 AND content.expires_at <= $2 \
             AND NOT EXISTS (SELECT 1 FROM cs_outbox AS outbox \
                 WHERE outbox.aggregate_key = content.aggregate_key \
                 AND outbox.content_ref = content.content_ref \
                 AND outbox.status IN ('pending', 'processing'))",
            &[&self.aggregate_key, &to_i64(now.0)?],
        )?;
        transaction.commit()?;
        Ok(deleted)
    }

    /// Loads the complete current settlement snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error for database, decoding, numeric, or lock failures.
    pub fn snapshot(&self) -> Result<SettlementSnapshot, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let row = client.query_one(
            "SELECT revision, ledger_revision, settlement_unit, protocol_state, \
                    protocol_format_version \
             FROM cs_relationship_aggregates WHERE aggregate_key = $1",
            &[&self.aggregate_key],
        )?;
        let mut aggregate = aggregate_from_row(&row)?;
        let subject = scoped_key(
            aggregate.state.attempt.subject.derivation_version(),
            aggregate.state.attempt.subject.as_bytes(),
        );
        let attempt_row = client.query_one(
            "SELECT attempt_state FROM cs_attempt_aggregates WHERE attempt_subject = $1",
            &[&subject],
        )?;
        aggregate.state.attempt = attempt_row
            .get::<_, Json<RepeatedAttemptState>>("attempt_state")
            .0;
        let balances = load_balances_client(&mut client, &self.aggregate_key)?;
        Ok(SettlementSnapshot::complete(
            aggregate.revision,
            aggregate.state,
            LedgerView::from_balances(aggregate.ledger_revision, aggregate.unit, balances),
        ))
    }

    /// Claims pending outbox items using `SKIP LOCKED` worker semantics.
    ///
    /// # Errors
    ///
    /// Returns an error for database, decoding, numeric, or lock failures.
    pub fn claim_outbox(
        &self,
        now: CanonicalTime,
        lease: Duration,
        limit: i64,
    ) -> Result<Vec<OutboxItem>, StorageError> {
        let claim_until = to_i64(now.checked_add(lease).ok_or(StorageError::NumericRange)?.0)?;
        let now = to_i64(now.0)?;
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut transaction = client.transaction()?;
        let rows = transaction.query(
            "SELECT id, aggregate_key, journal_position, payload \
             FROM cs_outbox WHERE aggregate_key = $1 AND (status = 'pending' \
             OR (status = 'processing' AND claim_until <= $2)) ORDER BY id \
             LIMIT $3 FOR UPDATE SKIP LOCKED",
            &[&self.aggregate_key, &now, &limit],
        )?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let id: i64 = row.get("id");
            transaction.execute(
                "UPDATE cs_outbox SET status = 'processing', claim_until = $2 WHERE id = $1",
                &[&id, &claim_until],
            )?;
            items.push(OutboxItem {
                id,
                aggregate_key: row.get("aggregate_key"),
                journal_position: JournalPosition(to_u64(row.get("journal_position"))?),
                payload: row.get::<_, Json<EffectIntent>>("payload").0,
            });
        }
        transaction.commit()?;
        Ok(items)
    }

    /// Marks one claimed outbox item as published.
    ///
    /// # Errors
    ///
    /// Returns an error for database, numeric, or lock failures.
    pub fn mark_outbox_published(&self, id: i64, at: CanonicalTime) -> Result<bool, StorageError> {
        let at = to_i64(at.0)?;
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        Ok(client.execute(
            "UPDATE cs_outbox SET status = 'published', published_at = $2, claim_until = NULL \
             WHERE id = $1 AND status = 'processing'",
            &[&id, &at],
        )? == 1)
    }

    /// Claims due scheduled tasks using `SKIP LOCKED` worker semantics.
    ///
    /// # Errors
    ///
    /// Returns an error for database, decoding, numeric, or lock failures.
    pub fn claim_due_schedules(
        &self,
        now: CanonicalTime,
        lease: Duration,
        limit: i64,
    ) -> Result<Vec<ScheduledItem>, StorageError> {
        let claim_until = to_i64(now.checked_add(lease).ok_or(StorageError::NumericRange)?.0)?;
        let now = to_i64(now.0)?;
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut transaction = client.transaction()?;
        let rows = transaction.query(
            "SELECT aggregate_key, task_key, task, due_at FROM cs_schedules \
             WHERE aggregate_key = $1 AND due_at <= $2 AND (status = 'pending' \
             OR (status = 'processing' AND claim_until <= $2)) ORDER BY due_at \
             LIMIT $3 FOR UPDATE SKIP LOCKED",
            &[&self.aggregate_key, &now, &limit],
        )?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let aggregate_key: String = row.get("aggregate_key");
            let task_key: String = row.get("task_key");
            transaction.execute(
                "UPDATE cs_schedules SET status = 'processing', claim_until = $3 \
                 WHERE aggregate_key = $1 AND task_key = $2",
                &[&aggregate_key, &task_key, &claim_until],
            )?;
            items.push(ScheduledItem {
                aggregate_key,
                task: row.get::<_, Json<ScheduleTask>>("task").0,
                due_at: CanonicalTime(to_u64(row.get("due_at"))?),
                claim_until: CanonicalTime(to_u64(claim_until)?),
            });
        }
        transaction.commit()?;
        Ok(items)
    }

    /// Executes only the protocol action predetermined by a claimed schedule.
    ///
    /// The opaque claim prevents callers from supplying arbitrary unsigned
    /// commands to the durable engine. Obsolete tasks are simply completed.
    ///
    /// # Errors
    ///
    /// Returns an error for a foreign claim or for snapshot, protocol, ledger,
    /// serialization, concurrency, idempotency, or numeric failures.
    #[allow(clippy::needless_pass_by_value)] // Consuming the opaque claim prevents caller reuse.
    pub fn execute_claimed_schedule(
        &self,
        item: ScheduledItem,
        now: CanonicalTime,
        scheduler: ProviderRef,
        operational_key: OperationalKeyRef,
        policy: PolicySnapshot,
    ) -> Result<(), StorageError> {
        if scheduler != policy.recipient_provider {
            return Err(StorageError::InvalidScheduleClaim);
        }
        self.key_registry()?.active_verifying_key(
            operational_key,
            ActorRef::Scheduler(scheduler),
            now,
        )?;
        self.validate_schedule_claim(&item, now)?;
        if let ScheduleTask::LaneHorizon(lane_id) = item.task {
            self.process_lane_horizon(lane_id, now)?;
            return Ok(());
        }
        let snapshot = self.snapshot()?;
        let command = match item.task {
            ScheduleTask::AdmissionTimeout(bond_id) => snapshot
                .state
                .bonds
                .get(&bond_id)
                .filter(|bond| bond.state == cs_mail_protocol::BondState::Reserved)
                .map(|bond| ProtocolCommand::CancelReservedAttempt {
                    bond_id,
                    expected_bond_version: bond.version.into(),
                    reason: cs_mail_protocol::CancellationReason::AdmissionTimeout,
                }),
            ScheduleTask::BondExpiry(bond_id) => snapshot
                .state
                .bonds
                .get(&bond_id)
                .filter(|bond| bond.state == cs_mail_protocol::BondState::Admitted)
                .map(|bond| ProtocolCommand::ExpireBond {
                    bond_id,
                    expected_bond_version: bond.version.into(),
                }),
            ScheduleTask::PersistenceRelease(reserve_id) => snapshot
                .state
                .reserves
                .get(&reserve_id)
                .filter(|reserve| reserve.state == cs_mail_protocol::ReserveState::Reserved)
                .map(|reserve| ProtocolCommand::ReleasePersistenceReserve {
                    reserve_id,
                    expected_reserve_version: reserve.version.into(),
                }),
            ScheduleTask::LaneHorizon(_) => unreachable!(),
        };
        if let Some(command) = command {
            self.execute_authorized(
                Authorized::assume_verified(
                    command,
                    ActorRef::Scheduler(scheduler),
                    operational_key,
                    schedule_idempotency(item.task),
                ),
                now,
                policy,
                None,
                false,
            )?;
        } else {
            self.complete_claimed_schedule(&item)?;
        }
        Ok(())
    }

    fn validate_schedule_claim(
        &self,
        item: &ScheduledItem,
        now: CanonicalTime,
    ) -> Result<(), StorageError> {
        if item.aggregate_key != self.aggregate_key || item.due_at > now || item.claim_until <= now
        {
            return Err(StorageError::InvalidScheduleClaim);
        }
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let row = client.query_opt(
            "SELECT due_at, status, claim_until FROM cs_schedules \
             WHERE aggregate_key = $1 AND task_key = $2",
            &[&self.aggregate_key, &task_key(item.task)?],
        )?;
        let Some(row) = row else {
            return Err(StorageError::InvalidScheduleClaim);
        };
        let due_at = CanonicalTime(to_u64(row.get("due_at"))?);
        let status: String = row.get("status");
        let claim_until = row
            .get::<_, Option<i64>>("claim_until")
            .map(to_u64)
            .transpose()?
            .map(CanonicalTime);
        if due_at != item.due_at || status != "processing" || claim_until != Some(item.claim_until)
        {
            return Err(StorageError::InvalidScheduleClaim);
        }
        Ok(())
    }

    fn complete_claimed_schedule(&self, item: &ScheduledItem) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let deleted = client.execute(
            "DELETE FROM cs_schedules WHERE aggregate_key = $1 AND task_key = $2 \
             AND status = 'processing' AND due_at = $3 AND claim_until = $4",
            &[
                &self.aggregate_key,
                &task_key(item.task)?,
                &to_i64(item.due_at.0)?,
                &to_i64(item.claim_until.0)?,
            ],
        )?;
        if deleted != 1 {
            return Err(StorageError::InvalidScheduleClaim);
        }
        Ok(())
    }
}

struct Aggregate {
    revision: u64,
    ledger_revision: u64,
    unit: SettlementUnit,
    state: ProtocolState,
}

fn migrate(client: &mut Client) -> Result<(), StorageError> {
    let mut transaction = client.transaction()?;
    transaction.query_one(
        "SELECT pg_advisory_xact_lock(hashtext('cs-mail-schema-migrations'))",
        &[],
    )?;
    transaction.batch_execute(
        "CREATE TABLE IF NOT EXISTS cs_schema_migrations (\
             version BIGINT PRIMARY KEY, \
             applied_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()\
         )",
    )?;
    for (version, migration) in [
        (1_i64, MIGRATION_1),
        (2_i64, MIGRATION_2),
        (3_i64, MIGRATION_3),
        (4_i64, MIGRATION_4),
        (5_i64, MIGRATION_5),
        (6_i64, MIGRATION_6),
    ] {
        if transaction
            .query_opt(
                "SELECT version FROM cs_schema_migrations WHERE version = $1",
                &[&version],
            )?
            .is_none()
        {
            transaction.batch_execute(migration)?;
        }
    }
    transaction.commit()?;
    Ok(())
}

fn lock_registry_version(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    expected: Version,
) -> Result<(), StorageError> {
    let row = transaction
        .query_opt(
            "SELECT registry_version FROM cs_key_registries \
             WHERE aggregate_key = $1 FOR SHARE",
            &[&aggregate_key],
        )?
        .ok_or(StorageError::RegistryMissing)?;
    if to_u64(row.get("registry_version"))? != expected.0 {
        return Err(StorageError::Security(SecurityError::VersionConflict));
    }
    Ok(())
}

fn validate_issued_quote(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    command: &ProtocolCommand,
    require_signature: bool,
) -> Result<(), StorageError> {
    let ProtocolCommand::ReserveAttempt { terms, .. } = command else {
        return Ok(());
    };
    let row = transaction
        .query_opt(
            "SELECT terms, provider_operational_key, signature FROM cs_contact_quotes \
             WHERE aggregate_key = $1 AND quote_id = $2 FOR SHARE",
            &[&aggregate_key, &terms.quote_id.0.to_string()],
        )?
        .ok_or(StorageError::QuoteMissing)?;
    if row
        .get::<_, Json<cs_mail_protocol::ContactTerms>>("terms")
        .0
        != **terms
    {
        return Err(StorageError::DuplicateConflict);
    }
    if require_signature {
        let signature = row
            .get::<_, Option<Vec<u8>>>("signature")
            .ok_or(StorageError::QuoteNotSigned)?;
        let signature: [u8; 64] = signature
            .try_into()
            .map_err(|_| StorageError::Security(SecurityError::InvalidSignature))?;
        let provider_key = row
            .get::<_, Option<String>>("provider_operational_key")
            .ok_or(StorageError::QuoteNotSigned)?
            .parse::<u128>()
            .map(OperationalKeyRef)
            .map_err(|_| StorageError::NumericRange)?;
        let registry_row = transaction
            .query_opt(
                "SELECT registry FROM cs_key_registries WHERE aggregate_key = $1",
                &[&aggregate_key],
            )?
            .ok_or(StorageError::RegistryMissing)?;
        let registry = registry_row.get::<_, Json<KeyRegistry>>("registry").0;
        let provider_verifying_key = registry.active_verifying_key(
            provider_key,
            ActorRef::Provider(terms.recipient_provider),
            terms.issued_at,
        )?;
        SignedContactTerms {
            terms: (**terms).clone(),
            provider_operational_key: provider_key,
            signature,
        }
        .verify(&provider_verifying_key)?;
    }
    Ok(())
}

fn allocate_journal_position(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    aggregate_revision: u64,
    entry_kind: &str,
    now: CanonicalTime,
) -> Result<JournalPosition, StorageError> {
    let row = transaction.query_one(
        "INSERT INTO cs_canonical_journal \
         (aggregate_key, aggregate_revision, entry_kind, received_at) \
         VALUES ($1, $2, $3, $4) RETURNING position",
        &[
            &aggregate_key,
            &to_i64(aggregate_revision)?,
            &entry_kind,
            &to_i64(now.0)?,
        ],
    )?;
    Ok(JournalPosition(to_u64(row.get("position"))?))
}

fn validate_content_for_manifest(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    state: &ProtocolState,
    command: &ProtocolCommand,
    manifest: &TransitionManifest,
    now: CanonicalTime,
    protocol_version: cs_mail_primitives::ProtocolVersion,
) -> Result<(), StorageError> {
    let ProtocolCommand::AdmitAttempt {
        bond_id,
        content_ref,
        declaration_digest,
        message_valid_until,
        ..
    } = command
    else {
        return Ok(());
    };
    if !manifest.outbox_intents.iter().any(
        |intent| matches!(intent, EffectIntent::DeliverMessage { content_ref: delivered, .. } if delivered == content_ref),
    ) {
        return Ok(());
    }
    let bond = state
        .bonds
        .get(bond_id)
        .ok_or(StorageError::Protocol(ProtocolError::MissingRecord))?;
    let row = transaction
        .query_opt(
            "SELECT record FROM cs_encrypted_content \
             WHERE aggregate_key = $1 AND content_ref = $2 FOR SHARE",
            &[&aggregate_key, &content_ref.0.to_string()],
        )?
        .ok_or(StorageError::ContentMissing)?;
    let record = row.get::<_, Json<EncryptedContentRecord>>("record").0;
    if record.expires_at <= now {
        return Err(StorageError::ContentExpired);
    }
    if record.binding.message_valid_until.0 < now {
        return Err(StorageError::MessageValidityClosed);
    }
    let binding = &record.binding;
    if binding.content_ref != *content_ref
        || binding.message_id != bond.message_id
        || binding.sender != state.relationship.key.sender
        || binding.recipient != state.relationship.key.recipient
        || binding.protocol_version != protocol_version
        || binding.relationship != state.relationship.key.reference
        || binding.message_valid_until != *message_valid_until
        || message_declaration_digest(&binding.declarations)
            .map_err(|_| StorageError::ContentScopeMismatch)?
            != *declaration_digest
    {
        return Err(StorageError::ContentScopeMismatch);
    }
    Ok(())
}

fn validate_content_for_delivery(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    state: &ProtocolState,
    message_id: MessageId,
    content_ref: ContentRef,
    now: CanonicalTime,
    protocol_version: ProtocolVersion,
) -> Result<ContentBinding, StorageError> {
    let row = transaction
        .query_opt(
            "SELECT record FROM cs_encrypted_content \
             WHERE aggregate_key = $1 AND content_ref = $2 FOR SHARE",
            &[&aggregate_key, &content_ref.0.to_string()],
        )?
        .ok_or(StorageError::ContentMissing)?;
    let record = row.get::<_, Json<EncryptedContentRecord>>("record").0;
    if record.expires_at <= now {
        return Err(StorageError::ContentExpired);
    }
    if record.binding.message_valid_until.0 < now {
        return Err(StorageError::MessageValidityClosed);
    }
    let binding = record.binding;
    if binding.content_ref != content_ref
        || binding.message_id != message_id
        || binding.sender != state.relationship.key.sender
        || binding.recipient != state.relationship.key.recipient
        || binding.protocol_version != protocol_version
        || binding.relationship != state.relationship.key.reference
    {
        return Err(StorageError::ContentScopeMismatch);
    }
    Ok(binding)
}

fn advance_aggregate_revision(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    expected_revision: u64,
    now: CanonicalTime,
) -> Result<JournalPosition, StorageError> {
    let next = expected_revision
        .checked_add(1)
        .ok_or(StorageError::NumericRange)?;
    let changed = transaction.execute(
        "UPDATE cs_relationship_aggregates SET revision = $2, updated_at = $3 \
         WHERE aggregate_key = $1 AND revision = $4",
        &[
            &aggregate_key,
            &to_i64(next)?,
            &to_i64(now.0)?,
            &to_i64(expected_revision)?,
        ],
    )?;
    if changed != 1 {
        return Err(StorageError::VersionConflict);
    }
    allocate_journal_position(transaction, aggregate_key, next, "capability", now)
}

fn upsert_lane(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    lane: &Lane,
    now: CanonicalTime,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO cs_capability_lanes (aggregate_key, lane_id, lane, updated_at) \
         VALUES ($1, $2, $3, $4) ON CONFLICT (aggregate_key) DO UPDATE SET \
         lane_id = EXCLUDED.lane_id, lane = EXCLUDED.lane, updated_at = EXCLUDED.updated_at",
        &[
            &aggregate_key,
            &lane.grant.id.0.to_string(),
            &Json(lane),
            &to_i64(now.0)?,
        ],
    )?;
    Ok(())
}

fn insert_capability_event(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    position: JournalPosition,
    event: &CapabilityEvent,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO cs_capability_events \
         (aggregate_key, journal_position, ordinal, event) VALUES ($1, $2, 0, $3)",
        &[&aggregate_key, &to_i64(position.0)?, &Json(event)],
    )?;
    Ok(())
}

fn insert_outbox_intent(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    position: JournalPosition,
    now: CanonicalTime,
    intent: &EffectIntent,
) -> Result<(), StorageError> {
    let content_ref = match intent {
        EffectIntent::DeliverMessage { content_ref, .. } => Some(content_ref.0.to_string()),
        EffectIntent::EstablishRelationshipSolicitation { .. }
        | EffectIntent::ReviewLane { .. } => None,
    };
    transaction.execute(
        "INSERT INTO cs_outbox \
         (aggregate_key, journal_position, ordinal, payload, content_ref, status, created_at) \
         VALUES ($1, $2, 0, $3, $4, 'pending', $5)",
        &[
            &aggregate_key,
            &to_i64(position.0)?,
            &Json(intent),
            &content_ref,
            &to_i64(now.0)?,
        ],
    )?;
    Ok(())
}

fn delete_schedule(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    task: ScheduleTask,
) -> Result<(), StorageError> {
    transaction.execute(
        "DELETE FROM cs_schedules WHERE aggregate_key = $1 AND task_key = $2",
        &[&aggregate_key, &task_key(task)?],
    )?;
    Ok(())
}

fn revoke_lane_locked(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    now: CanonicalTime,
    position: JournalPosition,
) -> Result<(), StorageError> {
    let Some(row) = transaction.query_opt(
        "SELECT lane FROM cs_capability_lanes WHERE aggregate_key = $1 FOR UPDATE",
        &[&aggregate_key],
    )?
    else {
        return Ok(());
    };
    let mut lane = row.get::<_, Json<Lane>>("lane").0;
    if lane.state == LaneState::Revoked {
        return Ok(());
    }
    lane.revoke()?;
    upsert_lane(transaction, aggregate_key, &lane, now)?;
    delete_schedule(
        transaction,
        aggregate_key,
        ScheduleTask::LaneHorizon(lane.grant.id),
    )?;
    insert_capability_event(
        transaction,
        aggregate_key,
        position,
        &CapabilityEvent::LaneRevoked(lane.grant.id),
    )?;
    Ok(())
}

fn bootstrap(
    client: &mut Client,
    aggregate_key: &str,
    state: &ProtocolState,
    unit: SettlementUnit,
    sender_balance: Money,
) -> Result<(), StorageError> {
    let mut transaction = client.transaction()?;
    let attempt_subject = scoped_key(
        state.attempt.subject.derivation_version(),
        state.attempt.subject.as_bytes(),
    );
    transaction.execute(
        "INSERT INTO cs_attempt_aggregates (attempt_subject, attempt_state, updated_at) \
         VALUES ($1, $2, $3) ON CONFLICT (attempt_subject) DO NOTHING",
        &[
            &attempt_subject,
            &Json(&state.attempt),
            &to_i64(state.attempt.changed_at.0)?,
        ],
    )?;
    let inserted = transaction.execute(
        "INSERT INTO cs_relationship_aggregates \
         (aggregate_key, revision, ledger_revision, settlement_unit, protocol_state, updated_at, \
          protocol_format_version) \
         VALUES ($1, 0, 0, $2, $3, 0, $4) ON CONFLICT (aggregate_key) DO NOTHING",
        &[
            &aggregate_key,
            &i64::from(unit.0),
            &Json(state),
            &CURRENT_PROTOCOL_FORMAT_VERSION,
        ],
    )?;
    if inserted == 1 {
        let account = Account::Sender(state.attempt.sender_account);
        let account_key = account_key(account)?;
        let balance = sender_balance.minor_units().to_string();
        transaction.execute(
            "INSERT INTO cs_ledger_accounts \
             (aggregate_key, account_key, account, balance) \
             VALUES ($1, $2, $3, $4::text::numeric)",
            &[&aggregate_key, &account_key, &Json(account), &balance],
        )?;
    }
    let stored_version: i16 = transaction
        .query_one(
            "SELECT protocol_format_version FROM cs_relationship_aggregates \
             WHERE aggregate_key = $1",
            &[&aggregate_key],
        )?
        .get("protocol_format_version");
    if stored_version != CURRENT_PROTOCOL_FORMAT_VERSION {
        return Err(StorageError::UnsupportedStoredProtocolFormat(
            stored_version,
        ));
    }
    transaction.commit()?;
    Ok(())
}

fn load_locked_aggregate(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
) -> Result<Aggregate, StorageError> {
    let row = transaction.query_one(
        "SELECT revision, ledger_revision, settlement_unit, protocol_state, \
                protocol_format_version \
         FROM cs_relationship_aggregates WHERE aggregate_key = $1 FOR UPDATE",
        &[&aggregate_key],
    )?;
    let mut aggregate = aggregate_from_row(&row)?;
    let subject = scoped_key(
        aggregate.state.attempt.subject.derivation_version(),
        aggregate.state.attempt.subject.as_bytes(),
    );
    let attempt_row = transaction.query_one(
        "SELECT attempt_state FROM cs_attempt_aggregates \
         WHERE attempt_subject = $1 FOR UPDATE",
        &[&subject],
    )?;
    aggregate.state.attempt = attempt_row
        .get::<_, Json<RepeatedAttemptState>>("attempt_state")
        .0;
    Ok(aggregate)
}

fn aggregate_from_row(row: &postgres::Row) -> Result<Aggregate, StorageError> {
    let stored_version: i16 = row.get("protocol_format_version");
    if stored_version != CURRENT_PROTOCOL_FORMAT_VERSION {
        return Err(StorageError::UnsupportedStoredProtocolFormat(
            stored_version,
        ));
    }
    let unit = to_u64(row.get("settlement_unit"))?;
    Ok(Aggregate {
        revision: to_u64(row.get("revision"))?,
        ledger_revision: to_u64(row.get("ledger_revision"))?,
        unit: SettlementUnit(u32::try_from(unit).map_err(|_| StorageError::NumericRange)?),
        state: row.get::<_, Json<ProtocolState>>("protocol_state").0,
    })
}

fn load_balances(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
) -> Result<BTreeMap<Account, Money>, StorageError> {
    let rows = transaction.query(
        "SELECT account, balance::text AS balance FROM cs_ledger_accounts \
         WHERE aggregate_key = $1",
        &[&aggregate_key],
    )?;
    decode_balances(rows)
}

fn load_balances_client(
    client: &mut Client,
    aggregate_key: &str,
) -> Result<BTreeMap<Account, Money>, StorageError> {
    let rows = client.query(
        "SELECT account, balance::text AS balance FROM cs_ledger_accounts \
         WHERE aggregate_key = $1",
        &[&aggregate_key],
    )?;
    decode_balances(rows)
}

fn decode_balances(rows: Vec<postgres::Row>) -> Result<BTreeMap<Account, Money>, StorageError> {
    rows.into_iter()
        .map(|row| {
            let account = row.get::<_, Json<Account>>("account").0;
            let balance: String = row.get("balance");
            let units = balance
                .parse::<u64>()
                .map_err(|_| StorageError::NumericRange)?;
            Ok((account, Money::from_minor_units(units)))
        })
        .collect()
}

fn load_idempotent_result(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    key: IdempotencyKey,
    request: &StoredRequest,
) -> Result<Option<DurableExecutionOutcome>, StorageError> {
    let key = key.0.to_string();
    let row = transaction.query_opt(
        "SELECT request, manifest FROM cs_idempotency_records \
         WHERE aggregate_key = $1 AND idempotency_key = $2",
        &[&aggregate_key, &key],
    )?;
    row.map_or(Ok(None), |row| {
        if row.get::<_, Json<StoredRequest>>("request").0 != *request {
            return Err(StorageError::DuplicateConflict);
        }
        Ok(Some(DurableExecutionOutcome {
            manifest: row.get::<_, Json<TransitionManifest>>("manifest").0,
            replayed: true,
        }))
    })
}

#[allow(clippy::too_many_arguments)]
fn persist_manifest(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    manifest: &TransitionManifest,
    next_ledger: &LedgerView,
    request: &StoredRequest,
    idempotency_key: IdempotencyKey,
    now: CanonicalTime,
    position: JournalPosition,
) -> Result<(), StorageError> {
    let next_revision = manifest
        .expected_snapshot_revision
        .checked_add(1)
        .ok_or(StorageError::NumericRange)?;
    let changed = transaction.execute(
        "UPDATE cs_relationship_aggregates SET revision = $2, ledger_revision = $3, \
         protocol_state = $4, updated_at = $5 WHERE aggregate_key = $1 AND revision = $6",
        &[
            &aggregate_key,
            &to_i64(next_revision)?,
            &to_i64(next_ledger.revision)?,
            &Json(&manifest.next_state),
            &to_i64(now.0)?,
            &to_i64(manifest.expected_snapshot_revision)?,
        ],
    )?;
    if changed != 1 {
        return Err(StorageError::VersionConflict);
    }
    let attempt_subject = scoped_key(
        manifest.next_state.attempt.subject.derivation_version(),
        manifest.next_state.attempt.subject.as_bytes(),
    );
    if transaction.execute(
        "UPDATE cs_attempt_aggregates SET attempt_state = $2, updated_at = $3 \
         WHERE attempt_subject = $1",
        &[
            &attempt_subject,
            &Json(&manifest.next_state.attempt),
            &to_i64(now.0)?,
        ],
    )? != 1
    {
        return Err(StorageError::VersionConflict);
    }
    persist_balances(transaction, aggregate_key, next_ledger)?;
    persist_ledger_batch(
        transaction,
        aggregate_key,
        manifest,
        next_ledger.unit,
        now,
        position,
    )?;
    persist_events(transaction, aggregate_key, manifest, position)?;
    persist_schedules(transaction, aggregate_key, &manifest.schedule_changes)?;
    persist_outbox(transaction, aggregate_key, manifest, now, position)?;
    if let Some(TermsOutcome::BondRequired(terms)) = &manifest.terms_outcome {
        let inserted = transaction.execute(
            "INSERT INTO cs_contact_quotes \
             (aggregate_key, quote_id, terms, issued_at, expires_at) VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (aggregate_key, quote_id) DO NOTHING",
            &[
                &aggregate_key,
                &terms.quote_id.0.to_string(),
                &Json(terms),
                &to_i64(terms.issued_at.0)?,
                &to_i64(terms.expires_at.0)?,
            ],
        )?;
        if inserted == 0 {
            let existing = transaction.query_one(
                "SELECT terms FROM cs_contact_quotes WHERE aggregate_key = $1 AND quote_id = $2",
                &[&aggregate_key, &terms.quote_id.0.to_string()],
            )?;
            if existing
                .get::<_, Json<cs_mail_protocol::ContactTerms>>("terms")
                .0
                != **terms
            {
                return Err(StorageError::DuplicateConflict);
            }
        }
    }
    transaction.execute(
        "INSERT INTO cs_idempotency_records \
         (aggregate_key, idempotency_key, request, manifest, journal_position, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6)",
        &[
            &aggregate_key,
            &idempotency_key.0.to_string(),
            &Json(request),
            &Json(manifest),
            &to_i64(position.0)?,
            &to_i64(now.0)?,
        ],
    )?;
    Ok(())
}

fn persist_balances(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    ledger: &LedgerView,
) -> Result<(), StorageError> {
    for (account, balance) in ledger.balances() {
        let account_key = account_key(*account)?;
        let balance = balance.minor_units().to_string();
        transaction.execute(
            "INSERT INTO cs_ledger_accounts (aggregate_key, account_key, account, balance) \
             VALUES ($1, $2, $3, $4::text::numeric) \
             ON CONFLICT (aggregate_key, account_key) DO UPDATE SET \
             account = EXCLUDED.account, balance = EXCLUDED.balance",
            &[&aggregate_key, &account_key, &Json(account), &balance],
        )?;
    }
    Ok(())
}

fn persist_ledger_batch(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    manifest: &TransitionManifest,
    unit: SettlementUnit,
    now: CanonicalTime,
    position: JournalPosition,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO cs_ledger_batches \
         (aggregate_key, journal_position, settlement_unit, created_at) VALUES ($1, $2, $3, $4)",
        &[
            &aggregate_key,
            &to_i64(position.0)?,
            &i64::from(unit.0),
            &to_i64(now.0)?,
        ],
    )?;
    for (ordinal, transfer) in manifest.ledger_batch.transfers().iter().enumerate() {
        transaction.execute(
            "INSERT INTO cs_ledger_transfers \
             (aggregate_key, journal_position, ordinal, source_account, destination_account, amount) \
             VALUES ($1, $2, $3, $4, $5, $6::text::numeric)",
            &[
                &aggregate_key,
                &to_i64(position.0)?,
                &i32::try_from(ordinal).map_err(|_| StorageError::NumericRange)?,
                &Json(transfer.from),
                &Json(transfer.to),
                &transfer.amount.minor_units().to_string(),
            ],
        )?;
    }
    Ok(())
}

fn persist_events(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    manifest: &TransitionManifest,
    position: JournalPosition,
) -> Result<(), StorageError> {
    for (ordinal, event) in manifest.protocol_events.iter().enumerate() {
        transaction.execute(
            "INSERT INTO cs_protocol_events \
             (aggregate_key, journal_position, ordinal, event) VALUES ($1, $2, $3, $4)",
            &[
                &aggregate_key,
                &to_i64(position.0)?,
                &i32::try_from(ordinal).map_err(|_| StorageError::NumericRange)?,
                &Json(event),
            ],
        )?;
    }
    Ok(())
}

fn persist_schedules(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    changes: &[ScheduleChange],
) -> Result<(), StorageError> {
    for change in changes {
        match *change {
            ScheduleChange::Schedule { task, at } => {
                let key = task_key(task)?;
                transaction.execute(
                    "INSERT INTO cs_schedules \
                     (aggregate_key, task_key, task, due_at, status, claim_until) \
                     VALUES ($1, $2, $3, $4, 'pending', NULL) \
                     ON CONFLICT (aggregate_key, task_key) DO UPDATE SET \
                     task = EXCLUDED.task, due_at = EXCLUDED.due_at, \
                     status = 'pending', claim_until = NULL",
                    &[&aggregate_key, &key, &Json(task), &to_i64(at.0)?],
                )?;
            }
            ScheduleChange::Cancel { task } => {
                transaction.execute(
                    "DELETE FROM cs_schedules WHERE aggregate_key = $1 AND task_key = $2",
                    &[&aggregate_key, &task_key(task)?],
                )?;
            }
        }
    }
    Ok(())
}

fn persist_outbox(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    manifest: &TransitionManifest,
    now: CanonicalTime,
    position: JournalPosition,
) -> Result<(), StorageError> {
    for (ordinal, intent) in manifest.outbox_intents.iter().enumerate() {
        let content_ref = match intent {
            EffectIntent::DeliverMessage { content_ref, .. } => Some(content_ref.0.to_string()),
            EffectIntent::EstablishRelationshipSolicitation { .. }
            | EffectIntent::ReviewLane { .. } => None,
        };
        transaction.execute(
            "INSERT INTO cs_outbox \
             (aggregate_key, journal_position, ordinal, payload, content_ref, status, created_at) \
             VALUES ($1, $2, $3, $4, $5, 'pending', $6)",
            &[
                &aggregate_key,
                &to_i64(position.0)?,
                &i32::try_from(ordinal).map_err(|_| StorageError::NumericRange)?,
                &Json(intent),
                &content_ref,
                &to_i64(now.0)?,
            ],
        )?;
    }
    Ok(())
}

fn account_key(account: Account) -> Result<String, StorageError> {
    Ok(serde_json::to_string(&account)?)
}

fn task_key(task: ScheduleTask) -> Result<String, StorageError> {
    Ok(serde_json::to_string(&task)?)
}

fn schedule_idempotency(task: ScheduleTask) -> IdempotencyKey {
    let mut hasher = Sha256::new();
    hasher.update(b"cs-mail/schedule/v1");
    match task {
        ScheduleTask::AdmissionTimeout(id) => {
            hasher.update([0]);
            hasher.update(id.0.to_be_bytes());
        }
        ScheduleTask::BondExpiry(id) => {
            hasher.update([1]);
            hasher.update(id.0.to_be_bytes());
        }
        ScheduleTask::PersistenceRelease(id) => {
            hasher.update([2]);
            hasher.update(id.0.to_be_bytes());
        }
        ScheduleTask::LaneHorizon(id) => {
            hasher.update([3]);
            hasher.update(id.0.to_be_bytes());
        }
    }
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    IdempotencyKey(u128::from_be_bytes(bytes))
}

fn scoped_key(derivation_version: u16, value: &[u8; 32]) -> Vec<u8> {
    let mut key = Vec::with_capacity(34);
    key.extend_from_slice(&derivation_version.to_be_bytes());
    key.extend_from_slice(value);
    key
}

fn retention_record_key(relationship: &[u8; 32], domain: &str, object_ref: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(64 + domain.len() + object_ref.len());
    key.extend_from_slice(b"cs-mail/retention-record/v1\0");
    key.extend_from_slice(relationship);
    key.extend_from_slice(domain.as_bytes());
    key.push(0);
    key.extend_from_slice(object_ref.as_bytes());
    key
}

fn to_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::NumericRange)
}

fn to_u64(value: i64) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::NumericRange)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cs_mail_primitives::{BondId, PersistenceReserveId};

    #[test]
    fn schedule_task_kinds_have_stable_distinct_idempotency_keys() {
        assert_ne!(
            schedule_idempotency(ScheduleTask::AdmissionTimeout(BondId(1))),
            schedule_idempotency(ScheduleTask::BondExpiry(BondId(1)))
        );
        assert_ne!(
            schedule_idempotency(ScheduleTask::AdmissionTimeout(BondId(1))),
            schedule_idempotency(ScheduleTask::PersistenceRelease(PersistenceReserveId(1)))
        );
        assert_eq!(
            schedule_idempotency(ScheduleTask::PersistenceRelease(PersistenceReserveId(1))),
            schedule_idempotency(ScheduleTask::PersistenceRelease(PersistenceReserveId(1)))
        );
    }
}
