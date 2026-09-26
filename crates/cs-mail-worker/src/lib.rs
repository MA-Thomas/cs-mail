//! Bounded workers over a common durable external-work lifecycle.
use core::fmt;
use cs_mail_content::EncryptedContentRecord;
use cs_mail_finance::{PaymentError, PaymentOutcome, PaymentProcessor};
use cs_mail_primitives::{CanonicalTime, Duration, OperationalKeyRef, ProviderRef, SettlementUnit};
use cs_mail_protocol::{EffectIntent, PolicySnapshot};
use cs_mail_storage_postgres::{
    DeploymentQueue, PostgresDeployment, PostgresEngine, RelationshipQueue, StorageError,
    WorkClaims, WorkFailure, WorkItem, WorkPayload, WorkReport,
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
    engine: &impl WorkClaims,
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
        StorageError::Billing(cs_mail_billing::BillingError::Payment(
            PaymentError::FundingRestricted,
        )) => (WorkFailure::FundingRestricted, false),
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
fn pending_outcome(outcome: PaymentOutcome) -> Result<(), (WorkFailure, bool)> {
    match outcome {
        PaymentOutcome::Pending => Err((WorkFailure::DependencyUnavailable, false)),
        PaymentOutcome::Failed => Err((WorkFailure::InvalidEvidence, true)),
        _ => Ok(()),
    }
}

/// Finalizes published annual periods and prepares one automatic bank payment per allocation.
/// # Errors
/// Returns claim/acknowledgement errors; per-item failures retain their durable work.
pub fn run_annual_distribution_batch(
    engine: &PostgresDeployment,
    unit: SettlementUnit,
    clock: &impl cs_mail_application::accounts::AccountClock,
    lease: Duration,
    limit: i64,
) -> Result<WorkReport, WorkerError> {
    let now = clock.now();
    let mut report = WorkReport::default();
    for queue in [
        DeploymentQueue::AnnualAllocations(unit),
        DeploymentQueue::DistributionPreparation,
    ] {
        let items = engine.claim_work(queue, now, lease, limit)?;
        report.claimed += items.len();
        for item in items {
            let result = match item.payload {
                WorkPayload::AnnualAllocation { unit, distribution } => engine
                    .finalize_due_annual_distribution(unit, distribution, now)
                    .map_err(storage_failure),
                WorkPayload::PrepareDistribution { unit, allocation } => {
                    cs_mail_application::billing::operations::BillingService::new(engine, clock)
                        .prepare_distribution(unit, allocation)
                        .map_err(storage_failure)
                }
                _ => Err((WorkFailure::InvalidEvidence, true)),
            };
            finish(engine, &item, now, result, &mut report)?;
        }
    }
    Ok(report)
}

/// Executes due utility charges, preserving pending funding across worker retries.
/// # Errors
/// Returns durable claim/acknowledgement errors; individual failures remain recorded.
pub fn run_utility_payment_batch<P: PaymentProcessor>(
    engine: &PostgresDeployment,
    provider: &mut P,
    clock: &impl cs_mail_application::accounts::AccountClock,
    lease: Duration,
    limit: i64,
) -> Result<WorkReport, WorkerError> {
    let now = clock.now();
    let items = engine.claim_work(DeploymentQueue::UtilityPayments, now, lease, limit)?;
    let mut report = WorkReport {
        claimed: items.len(),
        ..WorkReport::default()
    };
    for item in items {
        let result = (|| {
            let WorkPayload::UtilityPayment {
                account,
                contract,
                operation,
            } = item.payload
            else {
                return Err((WorkFailure::InvalidEvidence, true));
            };
            if let Some(operation) =
                cs_mail_application::billing::operations::BillingService::new(engine, clock)
                    .authorize_dispatch(account, contract, operation)
                    .map_err(storage_failure)?
            {
                let receipt = reconcile(
                    provider,
                    &cs_mail_finance::ProcessorRequest::Submit(operation),
                )?;
                cs_mail_application::billing::operations::BillingService::new(engine, clock)
                    .confirm_payment(account, contract, &receipt)
                    .map_err(storage_failure)?;
                pending_outcome(receipt.evidence.outcome)?;
            }
            Ok(())
        })();
        finish(engine, &item, now, result, &mut report)?;
    }
    Ok(report)
}
fn reconcile<P: PaymentProcessor>(
    provider: &mut P,
    request: &cs_mail_finance::ProcessorRequest,
) -> Result<cs_mail_finance::SignedPaymentEvidence, (WorkFailure, bool)> {
    match request {
        cs_mail_finance::ProcessorRequest::Submit(operation) => {
            match provider.lookup(operation.id).map_err(payment_failure)? {
                Some(receipt) => Ok(receipt),
                None => provider.submit(operation).map_err(payment_failure),
            }
        }
        cs_mail_finance::ProcessorRequest::CancelCapture(cancellation) => provider
            .cancel_capture(cancellation)
            .map_err(payment_failure),
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
    let items = engine.claim_work(RelationshipQueue::Delivery, now, lease, limit)?;
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
pub fn run_payment_batch<P: PaymentProcessor>(
    engine: &PostgresEngine,
    provider: &mut P,
    now: CanonicalTime,
    lease: Duration,
    limit: i64,
    policy: &PolicySnapshot,
) -> Result<WorkReport, WorkerError> {
    let items = engine.claim_work(RelationshipQueue::RequestPayments, now, lease, limit)?;
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
            if let Some(request) = engine
                .authorize_request_dispatch(request_id, operation_id)
                .map_err(storage_failure)?
            {
                let receipt = reconcile(provider, &request)?;
                let outcome = receipt.evidence.outcome;
                engine
                    .confirm_request_payment(request_id, receipt, now, policy.clone())
                    .map_err(storage_failure)?;
                pending_outcome(outcome)?;
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
pub fn run_member_payment_batch<P: PaymentProcessor>(
    engine: &PostgresDeployment,
    provider: &mut P,
    unit: SettlementUnit,
    now: CanonicalTime,
    lease: Duration,
    limit: i64,
) -> Result<WorkReport, WorkerError> {
    let items = engine.claim_work(DeploymentQueue::MemberPayments(unit), now, lease, limit)?;
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
                let receipt = reconcile(
                    provider,
                    &cs_mail_finance::ProcessorRequest::Submit(operation),
                )?;
                engine
                    .confirm_member_payment(unit, allocation, &receipt, now)
                    .map_err(storage_failure)?;
                pending_outcome(receipt.evidence.outcome)?;
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
/// Processes a bounded inbox prefix and then one bounded artifact batch, even if
/// processing fails. Signing can satisfy the dependency that blocked the inbox;
/// the next run then retries the same received command.
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
    let processing = (|| -> Result<(), StorageError> {
        for _ in 0..limit {
            if !engine.process_next_received()? {
                break;
            }
        }
        Ok(())
    })();
    let artifacts = engine.sign_artifacts_batch(signer, now, lease, limit);
    processing?;
    Ok(artifacts?)
}

/// Retries durable identity confirmations independently of request processing.
/// # Errors
/// Leaves failed items pending for the host's next bounded batch.
pub fn run_identity_confirmation_batch(
    repository: &cs_mail_storage_postgres::PostgresAccountRepository,
    client: &impl identity_contract::IdentityClient,
    product_secret: &[u8; 32],
    limit: u32,
    at: CanonicalTime,
) -> Result<cs_mail_application::accounts::ConfirmationReport, WorkerError> {
    Ok(cs_mail_application::accounts::confirm_identity_enrollments(
        repository,
        client,
        product_secret,
        limit,
        at,
    )?)
}
