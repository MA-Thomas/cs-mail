//! Thin HTTP adapter for address configuration and public directory queries.
use crate::http::{ApiError, AppState};
use axum::{Json, extract::State, http::StatusCode};
use cs_mail_accounts::directory::{DirectoryQuery, DirectoryReference};
use cs_mail_application::directory::{DirectoryService, SignedRequest};
use cs_mail_primitives::AliasId;

pub async fn command(
    State(state): State<AppState>,
    Json(request): Json<SignedRequest>,
) -> Result<Json<cs_mail_application::directory::Response>, ApiError> {
    let gateway = state.members.ok_or_else(ApiError::unavailable)?;
    let value = state
        .blocking
        .run(move || {
            DirectoryService(gateway.deployment()).handle(
                &request,
                gateway.scope(),
                state.directory_domain.as_deref(),
                crate::http::now,
            )
        })
        .await
        .map_err(|_| ApiError::unavailable())??;
    Ok(Json(value))
}
pub async fn search(
    State(state): State<AppState>,
    Json(query): Json<DirectoryQuery>,
) -> Result<Json<cs_mail_application::directory::Page>, ApiError> {
    let value = state
        .blocking
        .run(move || state.deployment.directory_search(&query, crate::http::now))
        .await
        .map_err(|_| ApiError::unavailable())??;
    Ok(Json(value))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lookup {
    reference: String,
}
pub async fn lookup(
    State(state): State<AppState>,
    Json(input): Json<Lookup>,
) -> Result<Json<cs_mail_application::directory::Approach>, ApiError> {
    if let Ok(mailbox) =
        cs_mail_accounts::directory::MemberAddress::try_from(input.reference.clone())
    {
        let value = state
            .blocking
            .run(move || {
                state
                    .deployment
                    .directory_lookup(DirectoryReference::Mailbox(mailbox), crate::http::now)
            })
            .await
            .map_err(|_| ApiError::unavailable())??;
        return value
            .map(Json)
            .ok_or_else(|| ApiError::from_status(StatusCode::NOT_FOUND, "unavailable"));
    }
    let value = input
        .reference
        .strip_prefix("/r/")
        .unwrap_or(&input.reference);
    let address = AliasId::new(
        value
            .parse()
            .map_err(|_| ApiError::from_status(StatusCode::NOT_FOUND, "unavailable"))?,
    )
    .map_err(|_| ApiError::from_status(StatusCode::NOT_FOUND, "unavailable"))?;
    let value = state
        .blocking
        .run(move || {
            state
                .deployment
                .directory_lookup(DirectoryReference::Alias(address), crate::http::now)
        })
        .await
        .map_err(|_| ApiError::unavailable())??;
    value
        .map(Json)
        .ok_or_else(|| ApiError::from_status(StatusCode::NOT_FOUND, "unavailable"))
}
