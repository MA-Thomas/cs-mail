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
//! use cs_mail_protocol::{KernelCommand, PolicySnapshot, ProtocolCommand};
//! use cs_mail_storage_postgres::PostgresEngine;
//!
//! fn bypass(
//!     engine: &PostgresEngine,
//!     command: KernelCommand<ProtocolCommand>,
//!     now: CanonicalTime,
//!     policy: PolicySnapshot,
//! ) {
//!     engine.execute(command, now, policy).unwrap();
//! }
//! ```

mod retention;
pub use retention::{LifecyclePolicy, RetentionReport};
mod work;
pub use work::{WorkFailure, WorkItem, WorkPayload, WorkQueue, WorkReport};
mod billing;
mod domain;
mod finance;
mod ingress;
use domain::{load_domain_records, persist_domain_records, persist_message};
pub use ingress::{ReceivedCommand, ReceivedOutcome, Refusal};

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

use cs_mail_capabilities::{
    BondFreeAdmission, CapabilityError, Lane, LaneControl, LaneControlAction, LaneHorizonEffect,
    LaneState, SignedLaneGrant,
};
use cs_mail_content::EncryptedContentRecord;
use cs_mail_ledger::{Account, LedgerError, LedgerView};
use cs_mail_primitives::{
    CanonicalTime, Duration, IdempotencyKey, JournalPosition, LaneId, MessageId, OperationalKeyRef,
    ProtocolVersion, ProviderRef, RequestId, RetentionPolicyVersion, ScheduleChange, ScheduleTask,
    SettlementUnit, Version,
};
use cs_mail_protocol::{
    ActorRef, EffectIntent, KernelCommand, Message, PolicySnapshot, ProtocolCommand, ProtocolError,
    ProtocolState, RelationshipState, RequestHistory, SettlementSnapshot, TermsOutcome,
    TransitionContext, TransitionManifest, transition,
};
use cs_mail_security::{
    KeyRegistry, SecurityError, SignedCommandBytes, SignedContactTerms, SignedReceipt, SigningScope,
};
use postgres::types::Json;
use postgres::{Client, NoTls, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

const MIGRATION_1: &str = include_str!("../migrations/0001_initial.sql");
const MIGRATION_2: &str = include_str!("../migrations/0002_encrypted_content.sql");
const MIGRATION_3: &str = include_str!("../migrations/0003_outbox_content_retention.sql");
const MIGRATION_4: &str = include_str!("../migrations/0004_express_lanes.sql");
const MIGRATION_5: &str = include_str!("../migrations/0005_protocol_foundations.sql");
const MIGRATION_6: &str = include_str!("../migrations/0006_authenticated_message_envelopes.sql");
const CURRENT_PROTOCOL_FORMAT_VERSION: i16 = 8;
const MIGRATION_14: &str = include_str!("../migrations/0014_utility_billing.sql");
const MIGRATION_13: &str = include_str!("../migrations/0013_annual_distribution.sql");
const MIGRATION_12: &str = include_str!("../migrations/0012_record_lifecycle.sql");
const MIGRATION_11: &str = include_str!("../migrations/0011_owner_records.sql");
const MIGRATION_10: &str = include_str!("../migrations/0010_financial_work.sql");
const MIGRATION_9: &str = include_str!("../migrations/0009_authenticated_receipts.sql");
const MIGRATION_8: &str = include_str!("../migrations/0008_domain_ownership.sql");
const MIGRATION_7: &str = include_str!("../migrations/0007_relationship_requests.sql");

#[derive(Debug)]
pub enum StorageError {
    Database(postgres::Error),
    Protocol(ProtocolError),
    Ledger(LedgerError),
    Finance(cs_mail_finance::ProgramError),
    Billing(cs_mail_billing::BillingError),
    Serialization(serde_json::Error),
    DuplicateConflict,
    VersionConflict,
    NumericRange,
    PendingCommands,
    AdmissionRefused(cs_mail_protocol::admission::AdmissionFailure),
    LockPoisoned,
    Security(SecurityError),
    ContentScopeMismatch,
    QuoteMissing,
    QuoteNotSigned,
    RegistryMissing,
    Capability(CapabilityError),
    BondFreeNotAuthorized,
    InvalidScheduleClaim,
    UnsupportedStoredProtocolFormat(i16),
    UnsupportedStoredFinancialFormat(i16),
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedStoredFinancialFormat(version) => write!(
                formatter,
                "stored financial format {version} requires an explicit migration"
            ),
            Self::PendingCommands => {
                formatter.write_str("earlier received commands must be processed first")
            }
            Self::AdmissionRefused(reason) => write!(formatter, "admission refused: {reason:?}"),
            Self::Billing(error) => write!(formatter, "billing error: {error}"),
            Self::Finance(error) => write!(formatter, "financial program error: {error}"),
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
            Self::ContentScopeMismatch => {
                formatter.write_str("encrypted content does not match the admitted message")
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

pub use cs_mail_protocol::CommittedTransition;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DurableExecutionOutcome {
    pub admission_failure: Option<cs_mail_protocol::admission::AdmissionFailure>,
    pub transition: CommittedTransition,
    #[serde(skip)]
    pub replayed: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub struct ScheduledItem {
    aggregate_key: String,
    task: ScheduleTask,
    due_at: CanonicalTime,
    claim_until: CanonicalTime,
    claim_token: i64,
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
    #[serde(skip)]
    pub replayed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LaneOperationOutcome {
    pub changed: bool,
    pub lane: Lane,
    #[serde(skip)]
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
    ) -> Result<Self, StorageError> {
        let client = Client::connect(database_url, NoTls)?;
        Self::from_client(client, aggregate_key, initial_state, unit)
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
    ) -> Result<Self, StorageError> {
        migrate(&mut client)?;
        let aggregate_key = aggregate_key.into();
        bootstrap(&mut client, &aggregate_key, initial_state, unit)?;
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
        ingress::require_drained(&mut transaction)?;
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

    /// Applies a horizon only after earlier received commands have been processed.
    fn process_lane_horizon(
        &self,
        lane_id: LaneId,
        now: CanonicalTime,
    ) -> Result<bool, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut transaction = client.transaction()?;
        ingress::require_drained(&mut transaction)?;
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
            None,
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
        enqueue_effect(
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
    pub fn lane(&self) -> Result<Option<Lane>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        Ok(client
            .query_opt(
                "SELECT lane FROM cs_capability_lanes WHERE aggregate_key = $1",
                &[&self.aggregate_key],
            )?
            .map(|row| row.get::<_, Json<Lane>>("lane").0))
    }

    /// Stores ciphertext supplied by the trusted host or SMTP gateway. This is not an
    /// authentication proof or a network upload endpoint. Native uploads use
    /// `store_authenticated_content`; delivery still requires authenticated ingress.
    /// # Errors
    /// Rejects conflicting content, invalid numeric values, or persistence failures.
    pub fn store_trusted_content(
        &self,
        record: &EncryptedContentRecord,
        retention_policy_version: RetentionPolicyVersion,
    ) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut transaction = client.transaction()?;
        ingress::lock_receipt_order(&mut transaction)?;
        persist_content(
            &mut transaction,
            &self.aggregate_key,
            record,
            retention_policy_version,
        )?;
        transaction.commit()?;
        Ok(())
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

    /// Loads the complete current settlement snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error for database, decoding, numeric, or lock failures.
    pub fn snapshot(&self) -> Result<SettlementSnapshot, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut transaction = client
            .build_transaction()
            .isolation_level(postgres::IsolationLevel::RepeatableRead)
            .read_only(true)
            .start()?;
        let row = transaction.query_one(
            "SELECT revision, ledger_revision, settlement_unit, relationship_state, \
                    protocol_format_version \
             FROM cs_relationship_aggregates WHERE aggregate_key = $1",
            &[&self.aggregate_key],
        )?;
        let mut aggregate = aggregate_from_row(&row)?;
        domain::load_request_records(
            &mut transaction,
            &self.aggregate_key,
            &mut aggregate.state,
            true,
            None,
        )?;
        let (history, payments, messages) = load_domain_records(
            &mut transaction,
            &self.aggregate_key,
            &aggregate.state,
            true,
            None,
        )?;
        let balances = load_balances(&mut transaction, &self.aggregate_key)?;
        let ledger = LedgerView::from_balances(aggregate.ledger_revision, aggregate.unit, balances);
        ledger.total_value()?;
        transaction.commit()?;
        Ok(SettlementSnapshot::complete(
            aggregate.revision,
            aggregate.state,
            history,
            payments,
            messages,
            ledger,
        ))
    }

    /// Claims due scheduled tasks using `SKIP LOCKED` worker semantics.
    /// Request decision deadlines are inclusive, so expiry is eligible only after `due_at`.
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
        if lease.0 == 0 || limit < 0 {
            return Err(StorageError::NumericRange);
        }
        let claim_until = to_i64(now.checked_add(lease).ok_or(StorageError::NumericRange)?.0)?;
        let now = to_i64(now.0)?;
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut transaction = client.transaction()?;
        ingress::require_drained(&mut transaction)?;
        let rows = transaction.query(
            "SELECT aggregate_key, task_key, task, due_at FROM cs_schedules \
             WHERE aggregate_key = $1 AND due_at <= $2 AND (status = 'pending' \
             OR (status = 'processing' AND claim_until <= $2)) \
             AND (due_at < $2 OR NOT (task ? 'RequestExpiry')) ORDER BY due_at \
             LIMIT $3 FOR UPDATE SKIP LOCKED",
            &[&self.aggregate_key, &now, &limit],
        )?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let aggregate_key: String = row.get("aggregate_key");
            let task_key: String = row.get("task_key");
            transaction.execute(
                "UPDATE cs_schedules SET status = 'processing', claim_until = $3, claim_token=nextval('cs_work_claim_token') \
                 WHERE aggregate_key = $1 AND task_key = $2",
                &[&aggregate_key, &task_key, &claim_until],
            )?;
            items.push(ScheduledItem {
                aggregate_key: aggregate_key.clone(),
                task: row.get::<_, Json<ScheduleTask>>("task").0,
                due_at: CanonicalTime(to_u64(row.get("due_at"))?),
                claim_until: CanonicalTime(to_u64(claim_until)?),
                claim_token: transaction.query_one("SELECT claim_token FROM cs_schedules WHERE aggregate_key=$1 AND task_key=$2", &[&aggregate_key,&task_key])?.get(0),
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
            ScheduleTask::SubmissionTimeout(request_id) => snapshot
                .state
                .requests
                .get(&request_id)
                .filter(|request| request.lifecycle.is_preparing_submission())
                .map(|request| ProtocolCommand::CancelRequestSubmission {
                    request_id,
                    expected_request_version: request.version.into(),
                    reason: cs_mail_protocol::CancellationReason::SubmissionTimeout,
                }),
            ScheduleTask::RequestExpiry(request_id) => snapshot
                .state
                .requests
                .get(&request_id)
                .filter(|request| request.lifecycle.is_awaiting_recipient_decision())
                .map(|request| ProtocolCommand::ExpireRequest {
                    request_id,
                    expected_request_version: request.version.into(),
                }),
            ScheduleTask::LaneHorizon(_) => unreachable!(),
        };
        if let Some(command) = command {
            self.execute_system(
                KernelCommand::new(
                    command,
                    ActorRef::Scheduler(scheduler),
                    operational_key,
                    schedule_idempotency(item.task),
                ),
                now,
                policy,
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
            "SELECT due_at, status, claim_until, claim_token FROM cs_schedules \
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
        if due_at != item.due_at
            || status != "processing"
            || claim_until != Some(item.claim_until)
            || row.get::<_, Option<i64>>("claim_token") != Some(item.claim_token)
        {
            return Err(StorageError::InvalidScheduleClaim);
        }
        Ok(())
    }

    fn complete_claimed_schedule(&self, item: &ScheduledItem) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let deleted = client.execute(
            "DELETE FROM cs_schedules WHERE aggregate_key = $1 AND task_key = $2 \
             AND status = 'processing' AND due_at = $3 AND claim_until = $4 AND claim_token=$5",
            &[
                &self.aggregate_key,
                &task_key(item.task)?,
                &to_i64(item.due_at.0)?,
                &to_i64(item.claim_until.0)?,
                &item.claim_token,
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
        (7_i64, MIGRATION_7),
        (8_i64, MIGRATION_8),
        (9_i64, MIGRATION_9),
        (10_i64, MIGRATION_10),
        (11_i64, MIGRATION_11),
        (12_i64, MIGRATION_12),
        (13_i64, MIGRATION_13),
        (14_i64, MIGRATION_14),
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

fn validate_issued_quote(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    command: &ProtocolCommand,
) -> Result<(), StorageError> {
    let ProtocolCommand::CreateRequest { terms, .. } = command else {
        return Ok(());
    };
    let row = transaction
        .query_opt(
            "SELECT terms, provider_operational_key, provider_verifying_key, signature FROM cs_contact_quotes \
             WHERE aggregate_key = $1 AND quote_id = $2 FOR SHARE",
            &[&aggregate_key, &terms.quote_id.0.to_string()],
        )?
        .ok_or(StorageError::QuoteMissing)?;
    if row
        .get::<_, Json<cs_mail_protocol::RequestTerms>>("terms")
        .0
        != **terms
    {
        return Err(StorageError::DuplicateConflict);
    }
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
    let provider_verifying_key: [u8; 32] = row
        .get::<_, Option<Vec<u8>>>("provider_verifying_key")
        .ok_or(StorageError::QuoteNotSigned)?
        .try_into()
        .map_err(|_| StorageError::NumericRange)?;
    SignedContactTerms {
        terms: (**terms).clone(),
        provider_operational_key: provider_key,
        signature,
    }
    .verify(&provider_verifying_key)?;
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

fn advance_aggregate_revision(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    expected_revision: u64,
    now: CanonicalTime,
    received_position: Option<JournalPosition>,
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
    if let Some(position) = received_position {
        record_journal_position(
            transaction,
            aggregate_key,
            next,
            "capability",
            now,
            position,
        )?;
        Ok(position)
    } else {
        allocate_journal_position(transaction, aggregate_key, next, "capability", now)
    }
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

fn enqueue_effect(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    position: JournalPosition,
    now: CanonicalTime,
    intent: &EffectIntent,
) -> Result<(), StorageError> {
    work::enqueue(
        transaction,
        aggregate_key,
        work::WorkSource::Receipt {
            position,
            ordinal: 0,
        },
        &WorkPayload::Effect(*intent),
        now,
    )
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
) -> Result<(), StorageError> {
    if !state.requests.is_empty() || !state.quotes.is_empty() || !state.used_quotes.is_empty() {
        return Err(StorageError::Protocol(ProtocolError::InvalidState));
    }
    let mut transaction = client.transaction()?;
    transaction.execute(
        "INSERT INTO cs_relationship_aggregates \
         (aggregate_key, revision, ledger_revision, settlement_unit, relationship_state, updated_at, \
          protocol_format_version) \
         VALUES ($1, 0, 0, $2, $3, 0, $4) ON CONFLICT (aggregate_key) DO NOTHING",
        &[
            &aggregate_key,
            &i64::from(unit.0),
            &Json(domain::RelationshipRecord::from(state)),
            &CURRENT_PROTOCOL_FORMAT_VERSION,
        ],
    )?;
    let request_history = scoped_key(
        state.relationship.history.derivation_version(),
        state.relationship.history.as_bytes(),
    );
    transaction.execute(
        "INSERT INTO cs_request_histories (request_history, history_state, updated_at) \
         VALUES ($1, $2, $3) ON CONFLICT (request_history) DO NOTHING",
        &[
            &request_history,
            &Json(RequestHistory::new(
                state.relationship.history,
                state.relationship.key.recipient,
                state.relationship.changed_at,
            )),
            &to_i64(state.relationship.changed_at.0)?,
        ],
    )?;
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
    transaction.execute("INSERT INTO cs_request_relationship_keys (relationship_ref, aggregate_key) VALUES ($1,$2) ON CONFLICT (aggregate_key) DO NOTHING", &[&scoped_key(state.relationship.key.reference.derivation_version(),state.relationship.key.reference.as_bytes()),&aggregate_key])?;
    transaction.commit()?;
    Ok(())
}

fn load_locked_aggregate(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
) -> Result<Aggregate, StorageError> {
    let row = transaction.query_one(
        "SELECT revision, ledger_revision, settlement_unit, relationship_state, \
                protocol_format_version \
         FROM cs_relationship_aggregates WHERE aggregate_key = $1 FOR UPDATE",
        &[&aggregate_key],
    )?;
    let mut aggregate = aggregate_from_row(&row)?;
    domain::load_request_records(
        transaction,
        aggregate_key,
        &mut aggregate.state,
        false,
        None,
    )?;
    let subject = scoped_key(
        aggregate.state.relationship.history.derivation_version(),
        aggregate.state.relationship.history.as_bytes(),
    );
    transaction.query_one(
        "SELECT history_state FROM cs_request_histories \
         WHERE request_history = $1 FOR UPDATE",
        &[&subject],
    )?;
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
        state: row
            .get::<_, Json<domain::RelationshipRecord>>("relationship_state")
            .0
            .into_state(),
    })
}

fn load_balances(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
) -> Result<BTreeMap<Account, i128>, StorageError> {
    let rows = transaction.query(
        "SELECT account, balance::text AS balance FROM cs_ledger_accounts \
         WHERE aggregate_key = $1",
        &[&aggregate_key],
    )?;
    decode_balances(rows)
}

fn decode_balances(rows: Vec<postgres::Row>) -> Result<BTreeMap<Account, i128>, StorageError> {
    rows.into_iter()
        .map(|row| {
            let account = row.get::<_, Json<Account>>("account").0;
            let balance: String = row.get("balance");
            let units = balance
                .parse::<i128>()
                .map_err(|_| StorageError::NumericRange)?;
            Ok((account, units))
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn persist_manifest(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    manifest: &TransitionManifest,
    next_ledger: &LedgerView,
    now: CanonicalTime,
    position: JournalPosition,
) -> Result<(), StorageError> {
    let next_revision = manifest
        .expected_snapshot_revision
        .checked_add(1)
        .ok_or(StorageError::NumericRange)?;
    let changed = transaction.execute(
        "UPDATE cs_relationship_aggregates SET revision = $2, ledger_revision = $3, \
         relationship_state = $4, updated_at = $5 WHERE aggregate_key = $1 AND revision = $6",
        &[
            &aggregate_key,
            &to_i64(next_revision)?,
            &to_i64(next_ledger.revision)?,
            &Json(domain::RelationshipRecord::from(&manifest.next_state)),
            &to_i64(now.0)?,
            &to_i64(manifest.expected_snapshot_revision)?,
        ],
    )?;
    if changed != 1 {
        return Err(StorageError::VersionConflict);
    }
    persist_domain_records(transaction, aggregate_key, manifest, now)?;
    finance::persist_forfeitures(transaction, &manifest.forfeitures, now)?;
    finance::persist_forfeiture_holds(
        transaction,
        next_ledger.unit,
        &manifest.forfeiture_holds,
        now,
    )?;
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
    enqueue_effects(transaction, aggregate_key, manifest, now, position)?;
    if let Some(TermsOutcome::ChargeRequired(terms)) = &manifest.terms_outcome {
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
                .get::<_, Json<cs_mail_protocol::RequestTerms>>("terms")
                .0
                != **terms
            {
                return Err(StorageError::DuplicateConflict);
            }
        }
    }
    Ok(())
}

fn persist_balances(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    ledger: &LedgerView,
) -> Result<(), StorageError> {
    for (account, balance) in ledger.balances() {
        let account_key = account_key(*account)?;
        let balance = balance.to_string();
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

fn enqueue_effects(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    manifest: &TransitionManifest,
    now: CanonicalTime,
    position: JournalPosition,
) -> Result<(), StorageError> {
    for (ordinal, intent) in manifest.outbox_intents.iter().enumerate() {
        work::enqueue(
            transaction,
            aggregate_key,
            work::WorkSource::Receipt { position, ordinal },
            &WorkPayload::Effect(*intent),
            now,
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
    // Earlier expiry workers could durably refuse at the inclusive deadline.
    // Use a new stable identity for expiry so those refusals do not poison retries.
    // Preserve their original receipts, and preserve all other schedule identities.
    if matches!(task, ScheduleTask::RequestExpiry(_)) {
        hasher.update(b"cs-mail/schedule/request-expiry/v2");
    } else {
        hasher.update(b"cs-mail/schedule/v1");
    }
    match task {
        ScheduleTask::SubmissionTimeout(id) => {
            hasher.update([0]);
            hasher.update(id.0.to_be_bytes());
        }
        ScheduleTask::RequestExpiry(id) => {
            hasher.update([1]);
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

fn persist_content(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
    record: &EncryptedContentRecord,
    retention_policy_version: RetentionPolicyVersion,
) -> Result<(), StorageError> {
    let content_ref = record.binding.content_ref.0.to_string();
    let existing = transaction.query_opt(
        "SELECT record FROM cs_encrypted_content \
             WHERE aggregate_key = $1 AND content_ref = $2",
        &[&aggregate_key, &content_ref],
    )?;
    if let Some(row) = existing {
        return if row.get::<_, Json<EncryptedContentRecord>>("record").0 == *record {
            Ok(())
        } else {
            Err(StorageError::DuplicateConflict)
        };
    }
    let relationship = load_locked_aggregate(transaction, aggregate_key)?
        .state
        .relationship
        .key
        .reference;
    let record_ref = retention_record_key(relationship.as_bytes(), "content", &content_ref);
    transaction.execute(
        "INSERT INTO cs_encrypted_content \
             (aggregate_key, content_ref, record, created_at, expires_at, \
              retention_policy_version, record_ref) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        &[
            &aggregate_key,
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
                &aggregate_key,
                &content_ref,
                &to_i64(u64::from(retention_policy_version.0))?,
                &to_i64(record.expires_at.0)?,
                &to_i64(record.created_at.0)?,
            ],
        )?;
    Ok(())
}

fn record_journal_position(
    tx: &mut Transaction<'_>,
    key: &str,
    revision: u64,
    kind: &str,
    now: CanonicalTime,
    position: JournalPosition,
) -> Result<(), StorageError> {
    tx.execute("INSERT INTO cs_canonical_journal(position,aggregate_key,aggregate_revision,entry_kind,received_at) VALUES($1,$2,$3,$4,$5)",&[&to_i64(position.0)?,&key,&to_i64(revision)?,&kind,&to_i64(now.0)?])?;
    Ok(())
}

impl From<cs_mail_billing::BillingError> for StorageError {
    fn from(e: cs_mail_billing::BillingError) -> Self {
        Self::Billing(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cs_mail_primitives::RequestId;

    #[test]
    fn schedule_task_kinds_have_stable_distinct_idempotency_keys() {
        assert_eq!(
            task_key(ScheduleTask::SubmissionTimeout(RequestId(1))).unwrap(),
            r#"{"SubmissionTimeout":1}"#
        );
        assert_ne!(
            schedule_idempotency(ScheduleTask::SubmissionTimeout(RequestId(1))),
            schedule_idempotency(ScheduleTask::RequestExpiry(RequestId(1)))
        );
    }
}
