//! Alias transport adapter. No policy, identity or financial decisions.
use crate::http::{ApiError, AppState};
use axum::{Json, extract::State};
use cs_mail_application::aliases::{Acknowledgement, SignedCommand};
pub async fn command(
    State(state): State<AppState>,
    Json(request): Json<SignedCommand>,
) -> Result<Json<Acknowledgement>, ApiError> {
    let gateway = state.members.ok_or_else(ApiError::unavailable)?;
    let domain = state.directory_domain.ok_or_else(ApiError::unavailable)?;
    let result = state
        .blocking
        .run(move || gateway.alias_command(&request, &domain))
        .await
        .map_err(|_| ApiError::unavailable())?
        .map_err(|error| match error {
            cs_mail_service::ServiceError::Storage(error) => ApiError::from(error),
            _ => ApiError::unavailable(),
        })?;
    Ok(Json(result))
}
