//! `PostgreSQL` durability for the cs-mail transition kernel.
//!
//! Each command locks one relationship aggregate, evaluates the pure kernel,
//! and atomically commits protocol state, ledger projections, journal records,
//! idempotency evidence, schedules, and outbox intents.

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
    CanonicalTime, ContentRef, Duration, IdempotencyKey, JournalPosition, LaneId, MessageId, Money,
    OperationalKeyRef, ProtocolVersion, ScheduleChange, ScheduleTask, SettlementUnit,
};
use cs_mail_protocol::{
    ActorRef, Authorized, EffectIntent, PolicySnapshot, ProtocolCommand, ProtocolError,
    ProtocolState, RelationshipState, SettlementSnapshot, TransitionContext, TransitionManifest,
    transition,
};
use cs_mail_security::{KeyRegistry, SecurityError, SignedCommandBytes, SigningScope};
use postgres::types::Json;
use postgres::{Client, IsolationLevel, NoTls, Transaction};
use serde::{Deserialize, Serialize};

const MIGRATION_1: &str = include_str!("../migrations/0001_initial.sql");
const MIGRATION_2: &str = include_str!("../migrations/0002_encrypted_content.sql");
const MIGRATION_3: &str = include_str!("../migrations/0003_outbox_content_retention.sql");
const MIGRATION_4: &str = include_str!("../migrations/0004_express_lanes.sql");

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
    Capability(CapabilityError),
    BondFreeNotAuthorized,
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
            Self::Capability(error) => write!(formatter, "capability error: {error}"),
            Self::BondFreeNotAuthorized => formatter
                .write_str("no accepted relationship or valid express lane authorizes delivery"),
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledItem {
    pub aggregate_key: String,
    pub task: ScheduleTask,
    pub due_at: CanonicalTime,
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
    /// Connects, migrates the schema, and creates the aggregate if absent.
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
        let mut client = Client::connect(database_url, NoTls)?;
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

    /// Atomically evaluates and commits one protocol command.
    ///
    /// # Errors
    ///
    /// Returns an error if database, protocol, ledger, serialization,
    /// concurrency, idempotency, or numeric preconditions fail.
    pub fn execute(
        &self,
        authorized: Authorized<ProtocolCommand>,
        now: CanonicalTime,
        policy: PolicySnapshot,
    ) -> Result<DurableExecutionOutcome, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut transaction = client
            .build_transaction()
            .isolation_level(IsolationLevel::ReadCommitted)
            .start()?;
        let aggregate = load_locked_aggregate(&mut transaction, &self.aggregate_key)?;
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
        let journal_position = aggregate
            .revision
            .checked_add(1)
            .ok_or(StorageError::NumericRange)?;
        let context = TransitionContext {
            now,
            journal_position: JournalPosition(journal_position),
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
        let verified = registry.verify(
            signed,
            now,
            policy.protocol_version,
            SigningScope {
                deployment_domain,
                intended_provider: policy.recipient_provider,
            },
        )?;
        self.execute(verified.authorized, now, policy)
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
        validate_content_for_delivery(
            &mut transaction,
            &self.aggregate_key,
            &aggregate.state,
            request.message_id,
            request.content_ref,
            now,
            request.protocol_version,
        )?;
        let (authority, event, schedule) =
            if aggregate.state.relationship.state == RelationshipState::Accepted {
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
                    evidence,
                    now,
                )?;
                let replayed = lane.consume(request.message_id, evidence, now)?;
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
    pub fn process_lane_horizon(
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
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let content_ref = record.binding.content_ref.0.to_string();
        let existing = client.query_opt(
            "SELECT record FROM cs_encrypted_content \
             WHERE aggregate_key = $1 AND content_ref = $2",
            &[&self.aggregate_key, &content_ref],
        )?;
        if let Some(row) = existing {
            return if row.get::<_, Json<EncryptedContentRecord>>("record").0 == *record {
                Ok(())
            } else {
                Err(StorageError::DuplicateConflict)
            };
        }
        client.execute(
            "INSERT INTO cs_encrypted_content \
             (aggregate_key, content_ref, record, created_at, expires_at) \
             VALUES ($1, $2, $3, $4, $5)",
            &[
                &self.aggregate_key,
                &content_ref,
                &Json(record),
                &to_i64(record.created_at.0)?,
                &to_i64(record.expires_at.0)?,
            ],
        )?;
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

    /// Deletes expired ciphertext records and returns the number removed.
    ///
    /// # Errors
    ///
    /// Returns an error for database, numeric, or lock failures.
    pub fn purge_expired_content(&self, now: CanonicalTime) -> Result<u64, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        Ok(client.execute(
            "DELETE FROM cs_encrypted_content AS content \
             WHERE content.aggregate_key = $1 AND content.expires_at <= $2 \
             AND NOT EXISTS (SELECT 1 FROM cs_outbox AS outbox \
                 WHERE outbox.aggregate_key = content.aggregate_key \
                 AND outbox.content_ref = content.content_ref \
                 AND outbox.status IN ('pending', 'processing'))",
            &[&self.aggregate_key, &to_i64(now.0)?],
        )?)
    }

    /// Loads the complete current settlement snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error for database, decoding, numeric, or lock failures.
    pub fn snapshot(&self) -> Result<SettlementSnapshot, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let row = client.query_one(
            "SELECT revision, ledger_revision, settlement_unit, protocol_state \
             FROM cs_relationship_aggregates WHERE aggregate_key = $1",
            &[&self.aggregate_key],
        )?;
        let aggregate = aggregate_from_row(&row)?;
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
            });
        }
        transaction.commit()?;
        Ok(items)
    }

    /// Removes a claimed schedule that is already obsolete.
    ///
    /// # Errors
    ///
    /// Returns an error for database, serialization, or lock failures.
    pub fn complete_schedule(&self, task: ScheduleTask) -> Result<bool, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        Ok(client.execute(
            "DELETE FROM cs_schedules WHERE aggregate_key = $1 AND task_key = $2",
            &[&self.aggregate_key, &task_key(task)?],
        )? == 1)
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
    transaction.batch_execute(MIGRATION_1)?;
    transaction.batch_execute(MIGRATION_2)?;
    transaction.batch_execute(MIGRATION_3)?;
    transaction.batch_execute(MIGRATION_4)?;
    transaction.commit()?;
    Ok(())
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
    let binding = record.binding;
    if binding.content_ref != *content_ref
        || binding.message_id != bond.message_id
        || binding.sender != state.relationship.key.sender
        || binding.recipient != state.relationship.key.recipient
        || binding.protocol_version != protocol_version
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
) -> Result<(), StorageError> {
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
    let binding = record.binding;
    if binding.content_ref != content_ref
        || binding.message_id != message_id
        || binding.sender != state.relationship.key.sender
        || binding.recipient != state.relationship.key.recipient
        || binding.protocol_version != protocol_version
    {
        return Err(StorageError::ContentScopeMismatch);
    }
    Ok(())
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
    Ok(JournalPosition(next))
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
    let inserted = transaction.execute(
        "INSERT INTO cs_relationship_aggregates \
         (aggregate_key, revision, ledger_revision, settlement_unit, protocol_state, updated_at) \
         VALUES ($1, 0, 0, $2, $3, 0) ON CONFLICT (aggregate_key) DO NOTHING",
        &[&aggregate_key, &i64::from(unit.0), &Json(state)],
    )?;
    if inserted == 1 {
        let account = Account::Sender(state.attempt.principal);
        let account_key = account_key(account)?;
        let balance = sender_balance.minor_units().to_string();
        transaction.execute(
            "INSERT INTO cs_ledger_accounts \
             (aggregate_key, account_key, account, balance) \
             VALUES ($1, $2, $3, $4::text::numeric)",
            &[&aggregate_key, &account_key, &Json(account), &balance],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn load_locked_aggregate(
    transaction: &mut Transaction<'_>,
    aggregate_key: &str,
) -> Result<Aggregate, StorageError> {
    let row = transaction.query_one(
        "SELECT revision, ledger_revision, settlement_unit, protocol_state \
         FROM cs_relationship_aggregates WHERE aggregate_key = $1 FOR UPDATE",
        &[&aggregate_key],
    )?;
    aggregate_from_row(&row)
}

fn aggregate_from_row(row: &postgres::Row) -> Result<Aggregate, StorageError> {
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

fn to_i64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::NumericRange)
}

fn to_u64(value: i64) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::NumericRange)
}
