//! Dedicated provider-evidence HTTP adapter. No member/operator routes or sending worker.
use crate::{blocking::Blocking, config::ExternalFeedback, external::Gateway, http::ApiError};
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use cs_mail_adapters::DomainIdentity;
use cs_mail_service::ServiceError;
use cs_mail_smtp_gateway::smtp2go::{Error, Webhook};
use cs_mail_storage_postgres::StorageError;
use std::sync::Arc;

#[derive(Clone)]
pub struct Receiver {
    gateway: Gateway,
    webhook: Arc<Webhook>,
    blocking: Blocking,
}
impl Receiver {
    pub fn configure(
        config: &ExternalFeedback,
        domain: &DomainIdentity,
        gateway: Gateway,
    ) -> Result<Option<Self>, String> {
        let ExternalFeedback::Smtp2go { secret_file, .. } = config else {
            return Ok(None);
        };
        let secret = crate::config::secret(secret_file).map_err(|e| e.to_string())?;
        let webhook =
            Webhook::new(secret, domain.clone()).map_err(|_| "invalid feedback secret")?;
        Ok(Some(Self {
            gateway,
            webhook: Arc::new(webhook),
            blocking: Blocking::new(2),
        }))
    }
    pub fn router(self) -> Router {
        Router::new()
            .route("/v1/external/smtp2go/events", post(receive))
            .layer(DefaultBodyLimit::max(65_536))
            .with_state(self)
    }
}

async fn receive(
    State(receiver): State<Receiver>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    let mut values = headers.get_all(axum::http::header::AUTHORIZATION).iter();
    let authorization = values.next().and_then(|h| h.to_str().ok()).unwrap_or("");
    if values.next().is_some() {
        return Err(ApiError::from_status(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
        ));
    }
    let event = receiver
        .webhook
        .authenticate(authorization, &body)
        .map_err(|e| match e {
            Error::Unauthorized => ApiError::from_status(StatusCode::UNAUTHORIZED, "unauthorized"),
            _ => ApiError::from_status(StatusCode::BAD_REQUEST, "invalid-provider-event"),
        })?;
    receiver
        .blocking
        .run(move || {
            let repository = receiver.gateway.external_repository()?;
            match event {
                cs_mail_smtp_gateway::smtp2go::Feedback::Conversation(event) => {
                    repository.provider_feedback(&event)
                }
                cs_mail_smtp_gateway::smtp2go::Feedback::Notice(event) => {
                    repository.accounts.notice_feedback(&event)
                }
            }
            .map_err(ServiceError::from)
        })
        .await
        .map_err(|_| ApiError::unavailable())?
        .map_err(|e| match e {
            ServiceError::Storage(StorageError::Correspondence(_)) => {
                ApiError::from_status(StatusCode::CONFLICT, "provider-event-conflict")
            }
            _ => ApiError::unavailable(),
        })?;
    // A lost success response is safe to retry: event identity is committed atomically.
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests;
