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
/// Largest accepted encrypted content upload.
const CONTENT_LIMIT: usize = 1024 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub mail_domain: Option<String>,
    pub directory_domain: Option<String>,
    pub deployment: PostgresDeployment,
    pub blocking: Blocking,
    pub bank: Option<std::sync::Arc<dyn crate::enrollment::BankEvidence>>,
    pub identity: Option<std::sync::Arc<crate::enrollment::IdentityService>>,
    pub maximum_unresolved: u32,
    pub browser_login: Option<crate::config::BrowserLoginConfig>,
    pub members: Option<crate::members::Gateway>,
    pub guests: Option<crate::guests::Guests>,
    pub external: Option<crate::external::Gateway>,
    #[cfg(feature = "simulation")]
    pub simulator: Option<Simulator>,
}

/// Development only: the durable simulator shared by operator scenario commands.
#[cfg(feature = "simulation")]
pub type Simulator = std::sync::Arc<std::sync::Mutex<cs_mail_simulation::PostgresSimulator>>;

pub struct ApiError {
    status: StatusCode,
    code: std::borrow::Cow<'static, str>,
}
impl ApiError {
    const fn new(status: StatusCode, code: &'static str) -> Self {
        Self {
            status,
            code: std::borrow::Cow::Borrowed(code),
        }
    }
    /// A refusal whose stable code names the domain error variant.
    pub const fn refusal(status: StatusCode, code: String) -> Self {
        Self {
            status,
            code: std::borrow::Cow::Owned(code),
        }
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
            StorageError::Alias(cs_mail_accounts::aliases::AliasError::Permission)
            | StorageError::Operations(OperationsError::Unauthorized) => {
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
        .route(
            "/v1/aliases/commands",
            post(crate::aliases::command).layer(DefaultBodyLimit::max(CONTENT_LIMIT)),
        )
        .route(
            "/v1/directory/commands",
            post(crate::directory::command).layer(DefaultBodyLimit::max(CONTENT_LIMIT)),
        )
        .route("/v1/directory/search", post(crate::directory::search))
        .route("/v1/directory/lookup", post(crate::directory::lookup))
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
        )
        .route("/v1/deployment", get(crate::members::deployment))
        .route(
            "/v1/accounts/commands",
            post(crate::members::account_command),
        )
        .route(
            "/v1/billing/commands",
            post(crate::members::billing_command),
        )
        .route("/v1/request-classes", post(crate::members::request_classes))
        .route(
            "/v1/endpoint-keys",
            post(crate::members::publish_endpoint_key),
        )
        .route(
            "/v1/addresses/{persona}/correspondence-alias",
            get(crate::members::receiving_alias),
        )
        .route("/v1/addresses/{persona}/offer", get(crate::members::offer))
        .route(
            "/v1/addresses/{persona}/endpoint-key",
            get(crate::members::endpoint_key),
        )
        .route(
            "/v1/relationships/{sender}/{recipient}/commands",
            post(crate::members::relationship_command),
        )
        .route(
            "/v1/relationships/{sender}/{recipient}/content",
            post(crate::members::content).layer(DefaultBodyLimit::max(CONTENT_LIMIT)),
        )
        .route("/v1/lanes", post(crate::members::lane))
        .route("/v1/members/reads", post(crate::members::read))
        .route(
            "/v1/correspondence/commands",
            post(crate::members::correspondence).layer(DefaultBodyLimit::max(CONTENT_LIMIT)),
        )
        .route("/v1/external/prepare", post(crate::external::prepare))
        .route(
            "/v1/external/commands",
            post(crate::external::command).layer(DefaultBodyLimit::max(CONTENT_LIMIT)),
        )
        .route("/r/{persona}", get(crate::guests::page))
        .route("/r/assets/app.js", get(crate::guests::script))
        .route("/r/assets/style.css", get(crate::guests::style))
        .route(
            "/v1/guest/verifications",
            post(crate::guests::begin_verification),
        )
        .route(
            "/v1/guest/verifications/{id}/confirm",
            post(crate::guests::confirm_verification),
        )
        .route(
            "/v1/guest/recipients/{persona}/offer",
            get(crate::guests::offer),
        )
        .route("/v1/guest/requests", post(crate::guests::request))
        .route("/v1/guest/requests/submit", post(crate::guests::submit))
        .route("/v1/guest/requests/cancel", post(crate::guests::cancel))
        .route("/v1/guest/status", post(crate::guests::status));
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
        Ok(Ok(readiness)) if readiness.bootstrapped && readiness.command_failure.is_none() => {
            Json(json!({ "database": "ok", "bootstrapped": true })).into_response()
        }
        Ok(Ok(readiness)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                json!({ "database": "ok", "bootstrapped": readiness.bootstrapped,
                "command_failure": readiness.command_failure }),
            ),
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
