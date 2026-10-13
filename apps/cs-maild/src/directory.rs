//! Thin HTTP adapter for address configuration and public directory queries.
use crate::http::{ApiError, AppState};
use axum::{Json, extract::State, http::StatusCode};
use cs_mail_accounts::directory::{DirectoryQuery, DirectoryReference};
use cs_mail_application::directory::{DirectoryService, SignedRequest};
use cs_mail_primitives::ProtocolIdentity;

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
    // Numeric public persona references and existing shared request links are canonical.
    // Also accept the deployment's generated member email address when configured.
    let address = if let Ok(value) = input.reference.parse::<u128>() {
        value
    } else if let Some(value) = input.reference.strip_prefix("/r/") {
        value
            .parse()
            .map_err(|_| ApiError::from_status(StatusCode::NOT_FOUND, "unavailable"))?
    } else {
        let mailbox = cs_mail_guests::GuestMailbox::parse(&input.reference)
            .map_err(|_| ApiError::from_status(StatusCode::NOT_FOUND, "unavailable"))?;
        let domain = state
            .mail_domain
            .as_ref()
            .ok_or_else(|| ApiError::from_status(StatusCode::NOT_FOUND, "unavailable"))?;
        let (local, host) = mailbox
            .address()
            .rsplit_once('@')
            .map(|(l, h)| (l.to_owned(), h.to_owned()))
            .ok_or_else(ApiError::unavailable)?;
        if !host.eq_ignore_ascii_case(domain) {
            return Err(ApiError::from_status(StatusCode::NOT_FOUND, "unavailable"));
        }
        let hex = local
            .strip_prefix("member-")
            .filter(|v| v.len() == 32)
            .ok_or_else(|| ApiError::from_status(StatusCode::NOT_FOUND, "unavailable"))?;
        u128::from_str_radix(hex, 16)
            .map_err(|_| ApiError::from_status(StatusCode::NOT_FOUND, "unavailable"))?
    };
    let value = state
        .blocking
        .run(move || {
            state.deployment.directory_lookup(
                DirectoryReference::Protocol(ProtocolIdentity(address)),
                crate::http::now,
            )
        })
        .await
        .map_err(|_| ApiError::unavailable())??;
    value
        .map(Json)
        .ok_or_else(|| ApiError::from_status(StatusCode::NOT_FOUND, "unavailable"))
}
