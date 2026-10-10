//! The HTTP API. Handlers validate transport input, call application use cases on the
//! bounded blocking pool, and map typed outcomes to responses; they make no domain
//! decisions. Error bodies carry a stable code only.
use crate::blocking::Blocking;
use crate::diagnostics;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use cs_mail_application::operations::OperatorService;
use cs_mail_operations::{OperationsError, OperatorCommand, SignedOperatorCommand};
use cs_mail_protocol::ProtocolError;
use cs_mail_storage_postgres::{PostgresDeployment, StorageError};
use serde_json::json;

/// Largest accepted request body.
const BODY_LIMIT: usize = 64 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub deployment: PostgresDeployment,
    pub blocking: Blocking,
    pub bank: Option<std::sync::Arc<dyn crate::enrollment::BankEvidence>>,
    pub identity: Option<std::sync::Arc<crate::enrollment::IdentityService>>,
    pub maximum_unresolved: u32,
    #[cfg(feature = "simulation")]
    pub simulator: Option<Simulator>,
}

/// Development only: the durable simulator shared by operator scenario commands.
#[cfg(feature = "simulation")]
pub type Simulator = std::sync::Arc<std::sync::Mutex<cs_mail_simulation::PostgresSimulator>>;

pub struct ApiError {
    status: StatusCode,
    code: &'static str,
}
impl ApiError {
    const fn new(status: StatusCode, code: &'static str) -> Self {
        Self { status, code }
    }
    pub fn unavailable() -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, "unavailable")
    }
    pub const fn from_status(status: StatusCode, code: &'static str) -> Self {
        Self::new(status, code)
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.code }))).into_response()
    }
}
impl From<StorageError> for ApiError {
    fn from(error: StorageError) -> Self {
        let mapped = match &error {
            StorageError::Operations(OperationsError::Unauthorized) => {
                Self::new(StatusCode::UNAUTHORIZED, "unauthorized")
            }
            StorageError::Operations(OperationsError::NotBootstrapped) => {
                Self::new(StatusCode::SERVICE_UNAVAILABLE, "not-bootstrapped")
            }
            StorageError::Operations(OperationsError::Invalid)
            | StorageError::Protocol(ProtocolError::PolicyInvalid) => {
                Self::new(StatusCode::BAD_REQUEST, "invalid")
            }
            StorageError::Operations(
                OperationsError::DuplicateConflict | OperationsError::BootstrapConflict,
            )
            | StorageError::Protocol(
                ProtocolError::DuplicateConflict | ProtocolError::VersionConflict,
            )
            | StorageError::Billing(cs_mail_billing::BillingError::Conflict)
            | StorageError::Identity(identity_contract::Error::Conflict) => {
                Self::new(StatusCode::CONFLICT, "conflict")
            }
            _ => Self::unavailable(),
        };
        tracing::warn!(error = %diagnostics::code(&error), status = mapped.status.as_u16(), "request refused");
        mapped
    }
}

pub fn now() -> cs_mail_primitives::CanonicalTime {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    cs_mail_primitives::CanonicalTime(u64::try_from(millis).unwrap_or(u64::MAX))
}

pub fn router(state: AppState) -> Router {
    let router = Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/v1/operator/commands", post(operator_command))
        .route("/v1/operator/reviews", post(crate::review::command))
        .route("/v1/enrollments", post(crate::enrollment::begin))
        .route(
            "/v1/enrollments/{operation}",
            get(crate::enrollment::status),
        )
        .route(
            "/v1/enrollments/{operation}/complete",
            post(crate::enrollment::complete),
        )
        .route(
            "/v1/enrollments/{operation}/renew",
            post(crate::enrollment::renew),
        );
    #[cfg(feature = "simulation")]
    let router = router.route("/v1/operator/simulation", post(crate::simulation::command));
    router
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .with_state(state)
}

/// Process health: answers whenever the process serves requests, without the database.
async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

/// Readiness: the database answers and the deployment has been bootstrapped.
async fn ready(State(state): State<AppState>) -> Response {
    let deployment = state.deployment.clone();
    match state.blocking.run(move || deployment.readiness()).await {
        Ok(Ok(readiness)) if readiness.bootstrapped => {
            Json(json!({ "database": "ok", "bootstrapped": true })).into_response()
        }
        Ok(Ok(_)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "database": "ok", "bootstrapped": false })),
        )
            .into_response(),
        Ok(Err(error)) => {
            tracing::warn!(error = %diagnostics::code(&error), "readiness check failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "database": "unavailable" })),
            )
                .into_response()
        }
        Err(_) => ApiError::unavailable().into_response(),
    }
}

async fn operator_command(
    State(state): State<AppState>,
    Json(signed): Json<SignedOperatorCommand<OperatorCommand>>,
) -> Result<Json<cs_mail_operations::OperatorOutcome>, ApiError> {
    let deployment = state.deployment.clone();
    let outcome = state
        .blocking
        .run(move || OperatorService::new(&deployment).execute(&signed, now))
        .await
        .map_err(|_| ApiError::unavailable())??;
    tracing::info!(
        ?outcome,
        "operator command answered (a repeat returns the recorded outcome)"
    );
    Ok(Json(outcome))
}
