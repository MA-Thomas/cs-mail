//! Cross-service account enrollment orchestration over a narrow persistence port.
pub mod enrollment;
pub(crate) mod memory;
pub mod operations;
mod pending;
use contract::{DecisionVerifier, Error};
use cs_mail_accounts::{Account, AccountIdentityBinding, EnrollmentInput};
use cs_mail_primitives::{AccountId, CanonicalTime};
pub use enrollment::AccountEnrollment;
use identity_contract::{
    self as contract, IdentityClient, Request, Response, SignedBankOwnership, SignedDecision,
    SignedRequest,
};
pub use memory::MemoryAccountRepository;
pub use pending::PendingEnrollment;

/// Trusted host clock, sampled by application callbacks after storage establishes protection.
pub trait AccountClock: Send + Sync {
    fn now(&self) -> CanonicalTime;
}
impl<F: Fn() -> CanonicalTime + Send + Sync> AccountClock for F {
    fn now(&self) -> CanonicalTime {
        self()
    }
}

/// The single activation rule used by every storage adapter, evaluated inside its transaction.
/// # Errors
/// Rejects stale authority or a decision for another pending attempt.
pub fn activate(
    pending: &PendingEnrollment,
    decision: &SignedDecision,
    verifier: &DecisionVerifier,
    at: CanonicalTime,
) -> Result<Account, Error> {
    pending.validate()?;
    let now = i64::try_from(at.0 / 1000).map_err(|_| Error::Invalid)?;
    let proof = verifier.verify(decision, pending.intent(), now)?;
    let c = proof.claims();
    let binding = AccountIdentityBinding::new(
        c.issuer.clone(),
        c.intent.product.clone(),
        c.subject_ref.clone(),
        c.binding_version,
    )
    .map_err(|_| Error::Invalid)?;
    Account::new(
        pending.account(),
        pending.principal(),
        pending.input(),
        binding,
    )
    .map_err(|_| Error::Invalid)
}

/// Each method completes its local transaction before returning. Implementations must
/// never retain locks across a call to an external service.
pub trait EnrollmentRepository {
    type Error: operations::AccountFailure;
    /// # Errors
    /// Atomically reserves unique ownership. Invoke the callback after protected reads.
    fn reserve(
        &self,
        operation: &str,
        input: &EnrollmentInput,
        decide: impl FnOnce(
            enrollment::ReservationContext,
            &dyn AccountClock,
        ) -> Result<PendingEnrollment, Self::Error>,
    ) -> Result<PendingEnrollment, Self::Error>;
    /// # Errors
    /// Serializes renewal with activation and preserves the original ownership reservation.
    fn renewal(
        &self,
        operation: &str,
        decide: impl FnOnce(
            PendingEnrollment,
            bool,
            &dyn AccountClock,
        ) -> Result<PendingEnrollment, Self::Error>,
    ) -> Result<PendingEnrollment, Self::Error>;
    /// # Errors
    /// Rejects missing or inconsistent local operations.
    fn pending(&self, operation: &str) -> Result<PendingEnrollment, Self::Error>;
    /// # Errors
    /// Holds receipt ordering, ownership and trust protection through atomic activation.
    fn activation(
        &self,
        decision: &SignedDecision,
        decide: impl FnOnce(
            enrollment::ActivationContext,
            &dyn AccountClock,
        ) -> Result<enrollment::EnrollmentDecision, Self::Error>,
    ) -> Result<AccountId, Self::Error>;
    /// # Errors
    /// Returns durable storage failures.
    fn claim_confirmations(
        &self,
        limit: u32,
        at: CanonicalTime,
    ) -> Result<Vec<ConfirmationClaim>, Self::Error>;
    /// # Errors
    /// Rejects expired or superseded claims and returns durable storage failures.
    fn mark_confirmed(&self, claim: &ConfirmationClaim) -> Result<(), Self::Error>;
    /// Explicit administrative retry after resolving the reported confirmation failure.
    /// # Errors
    /// Rejects an unknown or uncommitted operation; never changes its signed decision.
    fn retry_confirmation(&self, operation: &str, at: CanonicalTime) -> Result<(), Self::Error>;

    /// # Errors
    /// Records retry scheduling or the need for operator intervention.
    fn confirmation_failed(
        &self,
        claim: &ConfirmationClaim,
        failure: ConfirmationFailure,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnrollmentOutcome {
    AwaitingEvidence,
    Enrolled(AccountId),
    ReviewRequired,
    Denied,
}
#[derive(Clone)]
pub struct EnrollmentEvidence {
    pub oidc_token: String,
    pub bank: SignedBankOwnership,
    pub device_proof: Vec<u8>,
}
#[derive(Debug)]
pub enum EnrollmentError<E> {
    Storage(E),
    Contract(contract::Error),
    Client(contract::ClientError),
}
impl<E> From<contract::Error> for EnrollmentError<E> {
    fn from(e: contract::Error) -> Self {
        Self::Contract(e)
    }
}
impl<E: std::fmt::Display> std::fmt::Display for EnrollmentError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(e) => e.fmt(f),
            Self::Contract(e) => e.fmt(f),
            Self::Client(e) => e.fmt(f),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for EnrollmentError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Storage(e) => e,
            Self::Contract(e) => e,
            Self::Client(e) => e,
        })
    }
}
/// Completes the external step with no local locks held. Repositories sample their clock after acquiring ownership locks.
/// # Errors
/// Leaves the durable operation intact on transport failure or uncertain outcome.
pub fn enroll_with_identity<R: EnrollmentRepository>(
    repository: &R,
    operation: &str,
    evidence: EnrollmentEvidence,
    client: &impl IdentityClient,
    product_secret: &[u8; 32],
) -> Result<EnrollmentOutcome, EnrollmentError<R::Error>> {
    let p = repository
        .pending(operation)
        .map_err(EnrollmentError::Storage)?;
    let request = SignedRequest::sign(
        p.intent().product.clone(),
        Request::Enroll {
            intent: p.intent().clone(),
            oidc_token: evidence.oidc_token,
            bank: evidence.bank,
            device_proof: evidence.device_proof,
        },
        product_secret,
    )?;
    match client.call(&request).map_err(EnrollmentError::Client)? {
        Response::Eligible(decision) => Ok(EnrollmentOutcome::Enrolled(
            AccountEnrollment::new(repository)
                .commit(&decision)
                .map_err(EnrollmentError::Storage)?,
        )),
        Response::ReviewRequired => Ok(EnrollmentOutcome::ReviewRequired),
        Response::Denied => Ok(EnrollmentOutcome::Denied),
        Response::Rejected(error) => Err(error.into()),
        _ => Err(contract::Error::Context.into()),
    }
}
/// Reconcile a lost enrollment response without repeating authentication or creating IDs.
/// # Errors
/// Expired authority requires renewal and fresh evidence. Stored denial/review remains explicit.
pub fn reconcile_account_enrollment<R: EnrollmentRepository>(
    repository: &R,
    operation: &str,
    client: &impl IdentityClient,
    product_secret: &[u8; 32],
) -> Result<EnrollmentOutcome, EnrollmentError<R::Error>> {
    let p = repository
        .pending(operation)
        .map_err(EnrollmentError::Storage)?;
    let request = SignedRequest::sign(
        p.intent().product.clone(),
        Request::Lookup {
            operation: operation.into(),
        },
        product_secret,
    )?;
    match client.call(&request).map_err(EnrollmentError::Client)? {
        Response::Eligible(decision) => AccountEnrollment::new(repository)
            .commit(&decision)
            .map(EnrollmentOutcome::Enrolled)
            .map_err(EnrollmentError::Storage),
        Response::Rejected(error) => Err(error.into()),
        Response::ReviewRequired => Ok(EnrollmentOutcome::ReviewRequired),
        Response::Denied => Ok(EnrollmentOutcome::Denied),
        Response::Missing => Ok(EnrollmentOutcome::AwaitingEvidence),
        Response::Confirmed | Response::IdentityChanged(_) | Response::SecurityEvents(_) => {
            Err(contract::Error::Context.into())
        }
    }
}
/// A durable delivery claim. The increasing generation fences expired workers.
#[derive(Debug, Clone)]
pub struct ConfirmationClaim {
    pub decision: SignedDecision,
    pub generation: u64,
}
impl ConfirmationClaim {
    pub fn operation(&self) -> &str {
        &self.decision.claims.intent.operation
    }
}
pub const CONFIRMATION_LEASE_MILLIS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationFailure {
    Retryable,
    Intervention(Error),
}
impl ConfirmationFailure {
    /// Application retry policy, evaluated at the protected failure-recording instant.
    /// # Errors
    /// Rejects an overflowing retry timestamp.
    pub fn next_attempt(self, at: CanonicalTime) -> Result<CanonicalTime, Error> {
        Ok(CanonicalTime(
            at.0.checked_add(30_000).ok_or(Error::Invalid)?,
        ))
    }
}
#[derive(Debug, Default)]
pub struct ConfirmationReport {
    pub confirmed: usize,
    pub retryable: usize,
    pub intervention: usize,
    pub failures: Vec<(String, contract::ClientError)>,
}
/// Each failed delivery is persisted independently; a bad item cannot starve later items.
/// # Errors
/// Returns local storage failures. Delivery failures appear in the batch report.
pub fn confirm_identity_enrollments<R: EnrollmentRepository>(
    repository: &R,
    client: &impl IdentityClient,
    product_secret: &[u8; 32],
    limit: u32,
    at: CanonicalTime,
) -> Result<ConfirmationReport, R::Error> {
    let mut report = ConfirmationReport::default();
    for claim in repository.claim_confirmations(limit, at)? {
        let decision = &claim.decision;
        let operation = &decision.claims.intent.operation;
        let request = SignedRequest::sign(
            decision.claims.intent.product.clone(),
            Request::Confirm {
                decision: decision.clone(),
            },
            product_secret,
        )?;
        let failure = match client.call(&request) {
            Ok(Response::Confirmed) => {
                repository.mark_confirmed(&claim)?;
                report.confirmed += 1;
                continue;
            }
            Ok(Response::Rejected(e)) => e.into(),
            Err(e) => e,
            Ok(_) => Error::Context.into(),
        };
        let error = failure.code();
        report.failures.push((operation.clone(), failure));
        let failure = if error == Error::Unavailable {
            report.retryable += 1;
            ConfirmationFailure::Retryable
        } else {
            report.intervention += 1;
            ConfirmationFailure::Intervention(error)
        };
        repository.confirmation_failed(&claim, failure)?;
    }
    Ok(report)
}

/// Product-owned cursor for ordered identity-security reconciliation.
#[derive(Debug, Clone)]
pub struct IdentitySecurityCursor {
    pub product: String,
    pub subject: contract::ProductSubjectRef,
    pub version: u64,
}
pub trait IdentitySecurityRepository {
    type Error: From<contract::Error>;
    /// # Errors
    /// Rejects missing accounts or invalid stored bindings.
    fn security_cursor(&self, account: AccountId) -> Result<IdentitySecurityCursor, Self::Error>;
}
/// Replays ordered notifications with no local locks held during the service call.
/// The bank callback supplies the product's authenticated financial evidence for rebinding.
/// # Errors
/// Leaves the failed event unapplied so the durable cursor can resume at the same version.
pub fn synchronize_identity_security<
    R: IdentitySecurityRepository
        + operations::AccountStore<Error = <R as IdentitySecurityRepository>::Error>,
>(
    repository: &R,
    account: AccountId,
    clock: &impl AccountClock,
    client: &impl IdentityClient,
    product_secret: &[u8; 32],
    bank_evidence: impl Fn(
        &contract::changes::SignedSecurityEvent,
    ) -> Option<cs_mail_finance::BankVerification>,
) -> Result<usize, EnrollmentError<<R as IdentitySecurityRepository>::Error>> {
    let cursor = repository
        .security_cursor(account)
        .map_err(EnrollmentError::Storage)?;
    let request = SignedRequest::sign(
        cursor.product.clone(),
        Request::SecurityEvents {
            after_version: cursor.version,
            subject_ref: cursor.subject.clone(),
        },
        product_secret,
    )?;
    let response = client.call(&request).map_err(EnrollmentError::Client)?;
    let Response::SecurityEvents(events) = response else {
        return Err(contract::Error::Context.into());
    };
    let mut expected_version = cursor.version;
    for event in &events {
        expected_version = expected_version
            .checked_add(1)
            .ok_or(contract::Error::Invalid)?;
        if event.event.account != account.0.to_string()
            || event.event.product != cursor.product
            || event.event.subject_ref != cursor.subject
            || event.event.security_version != expected_version
        {
            return Err(contract::Error::Context.into());
        }
        let bank = bank_evidence(event);
        operations::AccountService::new(repository, clock)
            .apply_identity_change(event, bank.as_ref())
            .map_err(EnrollmentError::Storage)?;
    }
    Ok(events.len())
}
