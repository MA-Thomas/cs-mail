//! Retry-safe delivery and deadline worker loops.

use core::fmt;

use cs_mail_content::EncryptedContentRecord;
use cs_mail_primitives::{CanonicalTime, Duration, IdempotencyKey, OperationalKeyRef, ProviderRef};
use cs_mail_protocol::{
    ActorRef, Authorized, BondState, EffectIntent, PolicySnapshot, ProtocolCommand, ReserveState,
    ScheduleTask,
};
use cs_mail_storage_postgres::{OutboxItem, PostgresEngine, StorageError};
use sha2::{Digest, Sha256};

pub trait DeliverySink {
    type Error: fmt::Display;

    /// The sink must deduplicate by the stable outbox item ID or delivery intent reference.
    ///
    /// # Errors
    ///
    /// Returns a sink-specific transient or permanent delivery failure.
    fn publish(
        &mut self,
        item: &OutboxItem,
        content: Option<&EncryptedContentRecord>,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug)]
pub enum WorkerError {
    Storage(StorageError),
    Delivery(String),
    MissingClaimedContent,
}

impl fmt::Display for WorkerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "storage error: {error}"),
            Self::Delivery(error) => write!(formatter, "delivery error: {error}"),
            Self::MissingClaimedContent => {
                formatter.write_str("claimed delivery refers to missing ciphertext")
            }
        }
    }
}

impl std::error::Error for WorkerError {}

impl From<StorageError> for WorkerError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkerReport {
    pub claimed: usize,
    pub completed: usize,
}

/// Publishes one leased batch. Failures remain leased and become retryable after expiry.
///
/// # Errors
///
/// Returns the first storage, missing-content, or sink error without acknowledging that item.
pub fn deliver_batch<S: DeliverySink>(
    engine: &PostgresEngine,
    sink: &mut S,
    now: CanonicalTime,
    lease: Duration,
    limit: i64,
) -> Result<WorkerReport, WorkerError> {
    let items = engine.claim_outbox(now, lease, limit)?;
    let mut report = WorkerReport {
        claimed: items.len(),
        completed: 0,
    };
    for item in items {
        let content = match item.payload {
            EffectIntent::DeliverMessage { content_ref, .. } => Some(
                engine
                    .content(content_ref)?
                    .ok_or(WorkerError::MissingClaimedContent)?,
            ),
            EffectIntent::EstablishRelationshipSolicitation { .. }
            | EffectIntent::ReviewLane { .. } => None,
        };
        sink.publish(&item, content.as_ref())
            .map_err(|error| WorkerError::Delivery(error.to_string()))?;
        if engine.mark_outbox_published(item.id, now)? {
            report.completed += 1;
        }
    }
    Ok(report)
}

/// Materializes one leased batch of deadlines as ordinary protocol commands.
///
/// # Errors
///
/// Returns an error when claims, state reads, or command execution fail.
pub fn run_schedule_batch(
    engine: &PostgresEngine,
    now: CanonicalTime,
    lease: Duration,
    limit: i64,
    scheduler: ProviderRef,
    operational_key: OperationalKeyRef,
    policy: &PolicySnapshot,
) -> Result<WorkerReport, WorkerError> {
    let items = engine.claim_due_schedules(now, lease, limit)?;
    let mut report = WorkerReport {
        claimed: items.len(),
        completed: 0,
    };
    for item in items {
        if let ScheduleTask::LaneHorizon(lane_id) = item.task {
            engine.process_lane_horizon(lane_id, now)?;
            report.completed += 1;
            continue;
        }
        let snapshot = engine.snapshot()?;
        let command = match item.task {
            ScheduleTask::AdmissionTimeout(bond_id) => snapshot
                .state
                .bonds
                .get(&bond_id)
                .filter(|bond| bond.state == BondState::Reserved)
                .map(|bond| ProtocolCommand::CancelReservedAttempt {
                    bond_id,
                    expected_bond_version: bond.version,
                    reason: cs_mail_protocol::CancellationReason::AdmissionTimeout,
                }),
            ScheduleTask::BondExpiry(bond_id) => snapshot
                .state
                .bonds
                .get(&bond_id)
                .filter(|bond| bond.state == BondState::Admitted)
                .map(|bond| ProtocolCommand::ExpireBond {
                    bond_id,
                    expected_bond_version: bond.version,
                }),
            ScheduleTask::PersistenceRelease(reserve_id) => snapshot
                .state
                .reserves
                .get(&reserve_id)
                .filter(|reserve| reserve.state == ReserveState::Reserved)
                .map(|reserve| ProtocolCommand::ReleasePersistenceReserve {
                    reserve_id,
                    expected_reserve_version: reserve.version,
                }),
            ScheduleTask::LaneHorizon(_) => unreachable!(),
        };
        if let Some(command) = command {
            let authorized = Authorized::assume_verified(
                command,
                ActorRef::Scheduler(scheduler),
                operational_key,
                schedule_idempotency(item.task),
            );
            engine.execute(authorized, now, policy.clone())?;
        } else {
            engine.complete_schedule(item.task)?;
        }
        report.completed += 1;
    }
    Ok(report)
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

#[cfg(test)]
mod tests {
    use super::*;
    use cs_mail_primitives::{BondId, PersistenceReserveId};

    #[test]
    fn task_kinds_have_stable_distinct_idempotency_keys() {
        assert_ne!(
            schedule_idempotency(ScheduleTask::AdmissionTimeout(BondId(1))),
            schedule_idempotency(ScheduleTask::BondExpiry(BondId(1)))
        );
        assert_eq!(
            schedule_idempotency(ScheduleTask::PersistenceRelease(PersistenceReserveId(1))),
            schedule_idempotency(ScheduleTask::PersistenceRelease(PersistenceReserveId(1)))
        );
    }
}
