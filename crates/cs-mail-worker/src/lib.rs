//! Retry-safe delivery and deadline worker loops.

use core::fmt;

use cs_mail_content::EncryptedContentRecord;
use cs_mail_primitives::{CanonicalTime, Duration, OperationalKeyRef, ProviderRef};
use cs_mail_protocol::{EffectIntent, PolicySnapshot};
use cs_mail_storage_postgres::{OutboxItem, PostgresEngine, StorageError};

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
        engine.execute_claimed_schedule(item, now, scheduler, operational_key, policy.clone())?;
        report.completed += 1;
    }
    Ok(report)
}
