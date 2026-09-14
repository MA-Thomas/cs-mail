//! Bounded workers over a common durable external-work lifecycle.
use core::fmt;
use cs_mail_content::EncryptedContentRecord;
use cs_mail_finance::{PaymentError, PaymentProvider};
use cs_mail_primitives::{CanonicalTime, Duration, OperationalKeyRef, ProviderRef, SettlementUnit};
use cs_mail_protocol::{EffectIntent, PolicySnapshot};
use cs_mail_storage_postgres::{
    PostgresEngine, StorageError, WorkFailure, WorkItem, WorkPayload, WorkQueue, WorkReport,
};

pub trait DeliverySink {
    type Error: fmt::Display;
    /// Deduplicate by stable work ID or delivery intent reference.
    /// # Errors
    /// Returns a delivery failure; permanent errors must be classified explicitly.
    fn publish(
        &mut self,
        item: &WorkItem,
        content: Option<&EncryptedContentRecord>,
    ) -> Result<(), Self::Error>;
    fn permanent_failure(_error: &Self::Error) -> bool {
        false
    }
}
#[derive(Debug)]
pub enum WorkerError {
    Storage(StorageError),
}
impl fmt::Display for WorkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(e) => write!(f, "storage error: {e}"),
        }
    }
}
impl std::error::Error for WorkerError {}
impl From<StorageError> for WorkerError {
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}

fn finish(
    engine: &PostgresEngine,
    item: &WorkItem,
    now: CanonicalTime,
    result: Result<(), (WorkFailure, bool)>,
    report: &mut WorkReport,
) -> Result<(), StorageError> {
    let updated = match result {
        Ok(()) => {
            let ok = engine.complete_work(item, now)?;
            if ok {
                report.completed += 1;
            }
            ok
        }
        Err((failure, true)) => {
            let ok = engine.block_work(item, now, failure)?;
            if ok {
                report.blocked += 1;
            }
            ok
        }
        Err((failure, false)) => {
            let ok = engine.retry_work(item, now, failure)?;
            if ok {
                report.retried += 1;
            }
            ok
        }
    };
    if !updated {
        report.lost_claims += 1;
    }
    Ok(())
}
#[allow(clippy::needless_pass_by_value)] // Used directly as a consuming map_err conversion.
fn storage_failure(error: StorageError) -> (WorkFailure, bool) {
    match error {
        StorageError::Protocol(
            cs_mail_protocol::ProtocolError::PaymentInvalid
            | cs_mail_protocol::ProtocolError::DuplicateConflict,
        )
        | StorageError::Finance(
            cs_mail_finance::ProgramError::InvalidPayment
            | cs_mail_finance::ProgramError::DuplicateConflict
            | cs_mail_finance::ProgramError::Payment(_),
        )
        | StorageError::Security(_) => (WorkFailure::InvalidEvidence, true),
        _ => (WorkFailure::Storage, false),
    }
}
fn payment_failure(e: PaymentError) -> (WorkFailure, bool) {
    if e == PaymentError::Unavailable {
        (WorkFailure::DependencyUnavailable, false)
    } else {
        (WorkFailure::InvalidEvidence, true)
    }
}
fn reconcile<P: PaymentProvider>(
    provider: &mut P,
    operation: &cs_mail_finance::PaymentOperation,
    cancel: bool,
) -> Result<cs_mail_finance::SignedPaymentEvidence, (WorkFailure, bool)> {
    match provider.lookup(operation.id).map_err(payment_failure)? {
        Some(receipt) => Ok(receipt),
        None => provider.submit(operation, cancel).map_err(payment_failure),
    }
}
/// Delivers a bounded batch, isolating failures and retaining stable retry identities.
/// # Errors
/// Returns storage errors if claims or retry/completion records cannot be persisted.
pub fn deliver_batch<S: DeliverySink>(
    engine: &PostgresEngine,
    sink: &mut S,
    now: CanonicalTime,
    lease: Duration,
    limit: i64,
) -> Result<WorkReport, WorkerError> {
    let items = engine.claim_work(WorkQueue::Delivery, now, lease, limit)?;
    let mut report = WorkReport {
        claimed: items.len(),
        ..WorkReport::default()
    };
    for item in items {
        let result = (|| {
            let content = match item.payload {
                WorkPayload::Effect(EffectIntent::DeliverMessage { content_ref, .. }) => Some(
                    engine
                        .content(content_ref)
                        .map_err(storage_failure)?
                        .ok_or((WorkFailure::MissingContent, true))?,
                ),
                WorkPayload::Effect(
                    EffectIntent::EstablishRequestSolicitation { .. }
                    | EffectIntent::ReviewLane { .. },
                ) => None,
                _ => return Err((WorkFailure::InvalidEvidence, true)),
            };
            sink.publish(&item, content.as_ref())
                .map_err(|e| (WorkFailure::DependencyUnavailable, S::permanent_failure(&e)))
        })();
        finish(engine, &item, now, result, &mut report)?;
    }
    Ok(report)
}
/// Reconciles captures/refunds; lost responses never allocate another operation ID.
/// # Errors
/// Returns claim or acknowledgement storage errors; item failures are recorded in the report.
pub fn run_payment_batch<P: PaymentProvider>(
    engine: &PostgresEngine,
    provider: &mut P,
    now: CanonicalTime,
    lease: Duration,
    limit: i64,
    policy: &PolicySnapshot,
) -> Result<WorkReport, WorkerError> {
    let items = engine.claim_work(WorkQueue::RequestPayments, now, lease, limit)?;
    let mut report = WorkReport {
        claimed: items.len(),
        ..WorkReport::default()
    };
    for item in items {
        let result = (|| {
            let WorkPayload::Effect(EffectIntent::ExecutePayment {
                request_id,
                operation_id,
            }) = item.payload
            else {
                return Err((WorkFailure::InvalidEvidence, true));
            };
            if let Some((operation, cancel)) = engine
                .pending_request_payment(request_id, operation_id)
                .map_err(storage_failure)?
            {
                let receipt = reconcile(provider, &operation, cancel)?;
                engine
                    .confirm_request_payment(request_id, receipt, now, policy.clone())
                    .map_err(storage_failure)?;
            }
            Ok(())
        })();
        finish(engine, &item, now, result, &mut report)?;
    }
    Ok(report)
}
/// Executes bounded member payment work with the same claims and reconciliation as requests.
/// # Errors
/// Returns claim or acknowledgement storage errors.
pub fn run_member_payment_batch<P: PaymentProvider>(
    engine: &PostgresEngine,
    provider: &mut P,
    unit: SettlementUnit,
    now: CanonicalTime,
    lease: Duration,
    limit: i64,
) -> Result<WorkReport, WorkerError> {
    let items = engine.claim_work(WorkQueue::MemberPayments(unit), now, lease, limit)?;
    let mut report = WorkReport {
        claimed: items.len(),
        ..WorkReport::default()
    };
    for item in items {
        let result = (|| {
            let WorkPayload::MemberPayment {
                unit,
                allocation,
                operation,
            } = item.payload
            else {
                return Err((WorkFailure::InvalidEvidence, true));
            };
            if let Some(operation) = engine
                .pending_member_payment(unit, allocation, operation)
                .map_err(storage_failure)?
            {
                let receipt = reconcile(provider, &operation, false)?;
                engine
                    .confirm_member_payment(unit, allocation, &receipt, now)
                    .map_err(storage_failure)?;
            }
            Ok(())
        })();
        finish(engine, &item, now, result, &mut report)?;
    }
    Ok(report)
}
/// Drains only a bounded prefix; deadlines cannot pass any remaining received commands.
/// # Errors
/// Returns command or schedule errors. Failed schedule claims remain recoverable after expiry.
pub fn run_schedule_batch(
    engine: &PostgresEngine,
    now: CanonicalTime,
    lease: Duration,
    limit: i64,
    scheduler: ProviderRef,
    operational_key: OperationalKeyRef,
    policy: &PolicySnapshot,
) -> Result<WorkReport, WorkerError> {
    if limit < 0 {
        return Err(StorageError::NumericRange.into());
    }
    for _ in 0..limit {
        if !engine.process_next_received()? {
            break;
        }
    }
    let items = match engine.claim_due_schedules(now, lease, limit) {
        Ok(items) => items,
        Err(StorageError::PendingCommands) => return Ok(WorkReport::default()),
        Err(e) => return Err(e.into()),
    };
    let mut report = WorkReport {
        claimed: items.len(),
        ..WorkReport::default()
    };
    for item in items {
        if engine
            .execute_claimed_schedule(item, now, scheduler, operational_key, policy.clone())
            .is_ok()
        {
            report.completed += 1;
        } else {
            report.retried += 1;
        }
    }
    Ok(report)
}
/// Processes a bounded inbox prefix and then one bounded artifact batch.
/// # Errors
/// Infrastructure errors leave received commands pending in canonical order.
pub fn run_received_batch(
    engine: &PostgresEngine,
    signer: &cs_mail_security::ProviderSigner,
    now: CanonicalTime,
    lease: Duration,
    limit: i64,
) -> Result<WorkReport, WorkerError> {
    if limit < 0 {
        return Err(StorageError::NumericRange.into());
    }
    for _ in 0..limit {
        if !engine.process_next_received()? {
            break;
        }
    }
    Ok(engine.sign_artifacts_batch(signer, now, lease, limit)?)
}
