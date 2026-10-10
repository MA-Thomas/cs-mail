//! Operator review of enrollments held because the applicant's evidence overlaps another
//! enrollment's. The command is signed like other operator commands and forwarded to the
//! identity service, which records each decision.
use crate::enrollment::record;
use crate::http::{ApiError, AppState};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use cs_mail_application::accounts::review::{ReviewError, ReviewOutcome, review_enrollments};
use cs_mail_operations::{ReviewCommand, SignedOperatorCommand};
use cs_mail_storage_postgres::StorageError;

fn refused(error: ReviewError) -> ApiError {
    match error {
        ReviewError::Operations(error) => StorageError::Operations(error).into(),
        ReviewError::Contract(error) => {
            tracing::warn!(error = ?error, "review refused");
            match error {
                identity_contract::Error::Conflict => {
                    ApiError::from_status(StatusCode::CONFLICT, "conflict")
                }
                identity_contract::Error::Unavailable => ApiError::unavailable(),
                _ => ApiError::from_status(StatusCode::BAD_REQUEST, "invalid"),
            }
        }
        ReviewError::Client(error) => {
            tracing::warn!(error = ?error.code(), "identity service unavailable");
            ApiError::from_status(StatusCode::BAD_GATEWAY, "identity-unavailable")
        }
    }
}

pub async fn command(
    State(state): State<AppState>,
    Json(signed): Json<SignedOperatorCommand<ReviewCommand>>,
) -> Result<Json<ReviewOutcome>, ApiError> {
    let identity = state.identity.clone().ok_or_else(|| {
        ApiError::from_status(StatusCode::SERVICE_UNAVAILABLE, "no-identity-service")
    })?;
    let deployment = state.deployment.clone();
    let outcome = state
        .blocking
        .run(move || -> Result<ReviewOutcome, ApiError> {
            let record = record(&deployment)?;
            review_enrollments(
                &record.operator_authority(),
                &signed,
                &record.record().identity.product,
                &identity.client,
                &identity.product_secret,
            )
            .map_err(refused)
        })
        .await
        .map_err(|_| ApiError::unavailable())??;
    match &outcome {
        ReviewOutcome::Open(cases) => tracing::info!(open = cases.len(), "review cases listed"),
        ReviewOutcome::Resolved(case) => tracing::info!(
            case = case.id.as_str(),
            resolution = ?case.decision.as_ref().map(|d| d.resolution),
            "review case resolved (a repeat returns the recorded decision)"
        ),
    }
    Ok(Json(outcome))
}
