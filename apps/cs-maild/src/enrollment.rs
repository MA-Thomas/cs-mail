//! Open enrollment over HTTP. Anyone who completes a fresh login and bank-ownership
//! verification may enroll. Whether an applicant may hold an account beside another is
//! the identity service's decision under cs-mail's evidence policy, held for operator
//! review when their evidence overlaps another enrollment's (see `review`). Handlers call
//! the application's use cases on the blocking pool and render their outcomes.
use crate::http::{ApiError, AppState, now};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use cs_mail_application::accounts::{
    AccountEnrollment, EnrollmentError, EnrollmentEvidence, EnrollmentIdentifiers,
    EnrollmentOutcome, EnrollmentRepository, enroll_with_identity, reconcile_account_enrollment,
};
use cs_mail_operations::OperationsError;
use cs_mail_protocol::ActorRef;
use cs_mail_storage_postgres::{PostgresDeployment, StorageError};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

/// Where bank evidence comes from. A live deployment needs a bank integration; development
/// uses the simulated bank.
pub trait BankEvidence: Send + Sync {
    /// Financial verification of the enrolling person's bank account for the backend's
    /// allocations. `request` is specific to the bank integration.
    /// # Errors
    /// Rejects a request the integration cannot attest.
    fn verification(
        &self,
        scope: cs_mail_finance::FinancialScope,
        unit: cs_mail_primitives::SettlementUnit,
        identifiers: EnrollmentIdentifiers,
        request: &Value,
    ) -> Result<cs_mail_finance::BankVerification, ApiError>;
    /// Ownership attestation for one enrollment attempt and the login claiming it,
    /// reporting the account's owners. `request` names the same bank account as the
    /// `verification` reserved at the start of the enrollment.
    /// # Errors
    /// Rejects a request for another account, or evidence the integration cannot issue.
    fn ownership(
        &self,
        intent: &identity_contract::EnrollmentIntent,
        verification: &cs_mail_finance::BankVerification,
        request: &Value,
        login: (&str, &str),
        now: i64,
    ) -> Result<identity_contract::SignedBankOwnership, ApiError>;
}

/// The identity service and the product key that signs requests to it.
pub struct IdentityService {
    pub client: cs_mail_identity_client::HttpIdentityClient,
    pub product_secret: [u8; 32],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeginRequest {
    /// The client's identity for this enrollment; a retry with the same body is a replay.
    operation: String,
    /// The enrolling device's Ed25519 public key.
    initial_key: [u8; 32],
    /// Bank-integration specific request for account verification.
    bank: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteRequest {
    /// The ID token from a fresh login whose nonce is the enrollment challenge.
    oidc_token: String,
    /// The device's signature over the enrollment intent.
    device_proof: Vec<u8>,
    /// The login the token belongs to, for the bank's ownership attestation.
    login_issuer: String,
    login_subject: String,
    /// Bank-integration specific request naming the account verified at the start.
    bank: Value,
    /// The applicant's name, date of birth, address and phone. Forwarded to the identity
    /// service; never stored or logged here.
    applicant: identity_contract::evidence::ApplicantClaims,
}

fn refused(error: EnrollmentError<StorageError>) -> ApiError {
    match error {
        EnrollmentError::Storage(error) => error.into(),
        EnrollmentError::Contract(error) => {
            tracing::warn!(error = ?error, "enrollment refused");
            identity_error(error)
        }
        EnrollmentError::Client(error) => {
            tracing::warn!(error = ?error.code(), "identity service unavailable");
            ApiError::from_status(StatusCode::BAD_GATEWAY, "identity-unavailable")
        }
    }
}
fn identity_error(error: identity_contract::Error) -> ApiError {
    match error {
        identity_contract::Error::Conflict => {
            ApiError::from_status(StatusCode::CONFLICT, "conflict")
        }
        identity_contract::Error::Unavailable => ApiError::unavailable(),
        _ => ApiError::from_status(StatusCode::BAD_REQUEST, "invalid"),
    }
}
fn rendered(outcome: &EnrollmentOutcome) -> Value {
    match outcome {
        EnrollmentOutcome::Enrolled(account) => {
            json!({ "outcome": "enrolled", "account": account.0.to_string() })
        }
        EnrollmentOutcome::AwaitingEvidence => json!({ "outcome": "awaiting-evidence" }),
        EnrollmentOutcome::ReviewRequired => json!({ "outcome": "review-required" }),
        EnrollmentOutcome::UnderReview => json!({ "outcome": "under-review" }),
        EnrollmentOutcome::ReviewApproved => json!({ "outcome": "review-approved" }),
        EnrollmentOutcome::Denied => json!({ "outcome": "denied" }),
    }
}
pub(crate) fn record(
    deployment: &PostgresDeployment,
) -> Result<cs_mail_operations::DeploymentBootstrap, ApiError> {
    Ok(deployment
        .deployment_record()?
        .ok_or(StorageError::Operations(OperationsError::NotBootstrapped))?)
}
pub(crate) fn services(
    state: &AppState,
) -> Result<(Arc<dyn BankEvidence>, Arc<IdentityService>), ApiError> {
    let bank = state.bank.clone().ok_or_else(|| {
        ApiError::from_status(StatusCode::SERVICE_UNAVAILABLE, "no-bank-integration")
    })?;
    let identity = state.identity.clone().ok_or_else(|| {
        ApiError::from_status(StatusCode::SERVICE_UNAVAILABLE, "no-identity-service")
    })?;
    Ok((bank, identity))
}

/// Starts or replays an enrollment and returns the intent the client authenticates.
pub async fn begin(
    State(state): State<AppState>,
    Json(request): Json<BeginRequest>,
) -> Result<Json<Value>, ApiError> {
    let (bank, _) = services(&state)?;
    let deployment = state.deployment.clone();
    let maximum_unresolved = state.maximum_unresolved;
    let pending = state
        .blocking
        .run(move || -> Result<_, ApiError> {
            let record = record(&deployment)?;
            let identifiers = EnrollmentIdentifiers::for_operation(&request.operation);
            let verification = bank.verification(
                record.record().financial_scope,
                record.record().unit,
                identifiers,
                &request.bank,
            )?;
            let input = cs_mail_accounts::EnrollmentInput {
                bank: verification,
                persona: identifiers.persona,
                actor: ActorRef::Sender(identifiers.persona),
                key_ref: identifiers.key,
                initial_key: request.initial_key,
                maximum_unresolved,
            };
            Ok(AccountEnrollment::new(&deployment.accounts(now))
                .begin(&request.operation, &input)?)
        })
        .await
        .map_err(|_| ApiError::unavailable())??;
    tracing::info!("enrollment reserved");
    Ok(Json(json!({ "intent": pending.intent() })))
}

/// Renews an expired, uncommitted attempt with a fresh challenge; ownership is unchanged.
pub async fn renew(
    State(state): State<AppState>,
    Path(operation): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let deployment = state.deployment.clone();
    let pending = state
        .blocking
        .run(move || AccountEnrollment::new(&deployment.accounts(now)).renew(&operation))
        .await
        .map_err(|_| ApiError::unavailable())??;
    Ok(Json(json!({ "intent": pending.intent() })))
}

/// Submits fresh login, device and bank evidence to the identity service and commits an
/// eligible decision.
pub async fn complete(
    State(state): State<AppState>,
    Path(operation): Path<String>,
    Json(request): Json<CompleteRequest>,
) -> Result<Json<Value>, ApiError> {
    let (bank, identity) = services(&state)?;
    let deployment = state.deployment.clone();
    let outcome = state
        .blocking
        .run(move || -> Result<_, ApiError> {
            let accounts = deployment.accounts(now);
            let pending = accounts.pending(&operation)?;
            let seconds = i64::try_from(now().0 / 1000).map_err(|_| ApiError::unavailable())?;
            let ownership = bank.ownership(
                pending.intent(),
                &pending.input().bank,
                &request.bank,
                (&request.login_issuer, &request.login_subject),
                seconds,
            )?;
            enroll_with_identity(
                &accounts,
                &operation,
                EnrollmentEvidence {
                    oidc_token: request.oidc_token,
                    bank: ownership,
                    device_proof: request.device_proof,
                    applicant: request.applicant,
                },
                &identity.client,
                &identity.product_secret,
            )
            .map_err(refused)
        })
        .await
        .map_err(|_| ApiError::unavailable())??;
    tracing::info!(outcome = %rendered(&outcome)["outcome"], "enrollment completed");
    Ok(Json(rendered(&outcome)))
}

/// Reconciles an enrollment whose response was lost, without repeating authentication.
pub async fn status(
    State(state): State<AppState>,
    Path(operation): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let (_, identity) = services(&state)?;
    let deployment = state.deployment.clone();
    let outcome = state
        .blocking
        .run(move || {
            reconcile_account_enrollment(
                &deployment.accounts(now),
                &operation,
                &identity.client,
                &identity.product_secret,
            )
            .map_err(refused)
        })
        .await
        .map_err(|_| ApiError::unavailable())??;
    Ok(Json(rendered(&outcome)))
}

/// Delivers committed enrollment confirmations to the identity service, independently of
/// the requests that created them.
pub fn confirmations(
    deployment: PostgresDeployment,
    identity: Arc<IdentityService>,
) -> impl FnMut(cs_mail_primitives::Duration, i64) + Send + 'static {
    let mut failures = crate::diagnostics::BatchFailures::default();
    move |_lease, batch| {
        let limit = u32::try_from(batch).unwrap_or(u32::MAX);
        match cs_mail_worker::run_identity_confirmation_batch(
            &deployment.accounts(now),
            &identity.client,
            &identity.product_secret,
            limit,
            now(),
        ) {
            Ok(report) => {
                failures.succeeded("identity-confirmations");
                if report.confirmed + report.retryable + report.intervention > 0 {
                    tracing::info!(
                        confirmed = report.confirmed,
                        retryable = report.retryable,
                        intervention = report.intervention,
                        "identity confirmations"
                    );
                }
            }
            Err(cs_mail_worker::WorkerError::Storage(error)) => {
                failures.failed("identity-confirmations", crate::diagnostics::code(&error));
            }
        }
    }
}
