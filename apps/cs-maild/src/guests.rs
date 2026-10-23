//! The guest request page and its API. A guest verifies a mailbox, chooses one of a
//! member's request classes, authorizes one charge and writes a message. Handlers pass
//! input to the guest gateway on the blocking pool; the message text is never logged.
use crate::diagnostics;
use crate::http::{ApiError, AppState};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use cs_mail_primitives::RequestClassId;
use cs_mail_service::guests::{GuestGateway, Outbox};
use cs_mail_service::{ServiceError, UnixMillisecondClock};
use cs_mail_smtp_gateway::outbound::{DkimIdentity, OutboundMail, render_signed};
use cs_mail_storage_postgres::StorageError;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

pub type Guests = Arc<GuestGateway<UnixMillisecondClock>>;

/// Development only: notices are DKIM-signed as for delivery and written to a directory,
/// one exclusive file per notice; an uncertain write is never replaced on retry.
pub struct DevelopmentOutbox {
    pub directory: PathBuf,
    pub dkim: DkimIdentity,
}
impl Outbox for DevelopmentOutbox {
    fn domain(&self) -> &str {
        self.dkim.domain()
    }
    fn deliver(&self, mail: &OutboundMail) -> cs_mail_service::external::Handoff {
        use cs_mail_service::external::Handoff;
        let Ok(message) = render_signed(mail, &self.dkim) else {
            return Handoff::Failed;
        };
        let name: String = mail
            .message_id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let written = (|| -> Result<(), std::io::Error> {
            use std::io::Write as _;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            let mut file = options.open(self.directory.join(format!("{name}.eml")))?;
            file.write_all(&message)?;
            file.sync_all()
        })();
        if written.is_ok() {
            Handoff::Accepted(
                cs_mail_correspondence::external::delivery::ProviderReference::new(format!(
                    "development:{name}"
                ))
                .expect("notice identifier"),
            )
        } else {
            Handoff::Unknown
        }
    }
}

#[cfg(feature = "simulation")]
pub fn outbox(config: &crate::config::GuestsConfig) -> Result<Arc<dyn Outbox>, String> {
    use crate::config::GuestNoticeDelivery;
    let (selector, key) = match &config.notices {
        GuestNoticeDelivery::Smtp2go {
            dkim_secret_file,
            dkim_selector,
            ..
        }
        | GuestNoticeDelivery::Development {
            dkim_secret_file,
            dkim_selector,
            ..
        } => (dkim_selector, dkim_secret_file),
    };
    let dkim = DkimIdentity::from_pkcs8_pem(
        &config.mail_domain,
        selector,
        &std::fs::read(key).map_err(|_| "cannot read notice DKIM key")?,
    )
    .map_err(|_| "invalid notice DKIM configuration")?;
    match &config.notices {
        GuestNoticeDelivery::Development { directory, .. } => {
            std::fs::create_dir_all(directory).map_err(|_| "cannot create notice outbox")?;
            Ok(Arc::new(DevelopmentOutbox {
                directory: directory.clone(),
                dkim,
            }))
        }
        GuestNoticeDelivery::Smtp2go {
            api_key_file,
            region,
            ..
        } => {
            let key =
                std::fs::read_to_string(api_key_file).map_err(|_| "cannot read notice API key")?;
            let carrier =
                cs_mail_smtp_gateway::smtp2go::Smtp2go::new(region.endpoint(), key.trim())
                    .map_err(|_| "invalid notice SMTP2GO configuration")?;
            Ok(Arc::new(cs_mail_service::guests::Smtp2goOutbox {
                carrier,
                dkim,
            }))
        }
    }
}

fn refused(error: ServiceError) -> ApiError {
    use cs_mail_guests::GuestError;
    use cs_mail_protocol::ProtocolError;
    let code = |status, code| ApiError::from_status(status, code);
    match error {
        ServiceError::Storage(StorageError::Guest(error)) => match error {
            GuestError::InvalidVerificationState => ApiError::unavailable(),
            GuestError::InvalidMailbox => code(StatusCode::BAD_REQUEST, "invalid-mailbox"),
            GuestError::WrongCode => code(StatusCode::UNAUTHORIZED, "wrong-code"),
            GuestError::VerificationClosed => code(StatusCode::GONE, "verification-closed"),
            GuestError::TooManyAttempts => code(StatusCode::TOO_MANY_REQUESTS, "too-many-attempts"),
            GuestError::RateLimited => code(StatusCode::TOO_MANY_REQUESTS, "rate-limited"),
            GuestError::SessionExpired => code(StatusCode::UNAUTHORIZED, "session-expired"),
            GuestError::PendingRequest => code(StatusCode::CONFLICT, "pending-request"),
            GuestError::AlteredRequest => code(StatusCode::CONFLICT, "altered-request"),
            GuestError::PendingRequestsConflict => {
                code(StatusCode::CONFLICT, "pending-requests-conflict")
            }
            GuestError::RecipientMismatch | GuestError::NotAGuestCommand => {
                code(StatusCode::BAD_REQUEST, "invalid")
            }
        },
        ServiceError::Storage(StorageError::SenderSubjectConflict) => {
            code(StatusCode::CONFLICT, "address-taken")
        }
        ServiceError::Storage(StorageError::Identity(identity_contract::Error::Invalid)) => {
            code(StatusCode::NOT_FOUND, "unknown-recipient")
        }
        ServiceError::Storage(StorageError::Protocol(ProtocolError::BackoffActive { .. })) => {
            code(StatusCode::CONFLICT, "backoff")
        }
        ServiceError::Storage(StorageError::Protocol(ProtocolError::RequestAlreadyExists)) => {
            code(StatusCode::CONFLICT, "request-in-progress")
        }
        ServiceError::Storage(StorageError::Protocol(ProtocolError::ContactBlocked)) => {
            code(StatusCode::FORBIDDEN, "blocked")
        }
        ServiceError::Storage(StorageError::Protocol(e)) => {
            ApiError::refusal(StatusCode::CONFLICT, format!("protocol:{e:?}"))
        }
        ServiceError::Storage(StorageError::Billing(e)) => {
            ApiError::refusal(StatusCode::CONFLICT, format!("billing:{e:?}"))
        }
        ServiceError::Storage(error) => error.into(),
        ServiceError::RecipientNotReady => code(StatusCode::CONFLICT, "recipient-not-ready"),
        ServiceError::NotFunded => code(StatusCode::CONFLICT, "not-funded"),
        ServiceError::NoRequest => code(StatusCode::NOT_FOUND, "no-request"),
        ServiceError::PaymentMethod => {
            code(StatusCode::PAYMENT_REQUIRED, "payment-method-declined")
        }
        ServiceError::ContentTimeInvalid => code(StatusCode::BAD_REQUEST, "invalid-message"),
        ServiceError::NoProtocolPolicy => {
            code(StatusCode::SERVICE_UNAVAILABLE, "no-protocol-policy")
        }
        other => {
            tracing::warn!(error = %other, "guest request failed");
            ApiError::unavailable()
        }
    }
}
async fn run<T: Send + 'static>(
    state: &AppState,
    call: impl FnOnce(&GuestGateway<UnixMillisecondClock>) -> Result<T, ServiceError> + Send + 'static,
) -> Result<T, ApiError> {
    let guests = state.guests.clone().ok_or_else(|| {
        ApiError::from_status(StatusCode::SERVICE_UNAVAILABLE, "no-guest-service")
    })?;
    state
        .blocking
        .run(move || call(&guests).map_err(refused))
        .await
        .map_err(|_| ApiError::unavailable())?
}
fn json<T: serde::Serialize>(value: &T) -> Result<Json<Value>, ApiError> {
    serde_json::to_value(value)
        .map(Json)
        .map_err(|_| ApiError::unavailable())
}

/// The request page for one member's address.
pub async fn page(Path(_persona): Path<u128>) -> Html<&'static str> {
    Html(include_str!("../../cs-mail-web/index.html"))
}
/// The request page's script.
pub async fn script() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../../cs-mail-web/app.js"),
    )
        .into_response()
}
/// The request page's style sheet.
pub async fn style() -> Response {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../../cs-mail-web/style.css"),
    )
        .into_response()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeginVerification {
    mailbox: String,
}
pub async fn begin_verification(
    State(state): State<AppState>,
    Json(body): Json<BeginVerification>,
) -> Result<Json<Value>, ApiError> {
    let id = run(&state, move |g| g.begin_verification(&body.mailbox)).await?;
    tracing::info!("guest verification code sent");
    Ok(Json(json!({ "verification": id })))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmVerification {
    code: String,
}
pub async fn confirm_verification(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ConfirmVerification>,
) -> Result<Json<Value>, ApiError> {
    let token = run(&state, move |g| {
        let token = g.confirm_verification(&id, &body.code)?;
        let (mailbox, _) = g.whoami(&token)?;
        Ok((token, mailbox))
    })
    .await?;
    tracing::info!("guest mailbox verified");
    Ok(Json(json!({ "session": token.0, "mailbox": token.1 })))
}
pub async fn offer(
    State(state): State<AppState>,
    Path(persona): Path<u128>,
) -> Result<Json<Value>, ApiError> {
    json(
        &run(&state, move |g| {
            g.offer(
                cs_mail_primitives::AliasId::new(persona)
                    .map_err(|_| ServiceError::RecipientNotReady)?,
            )
        })
        .await?,
    )
}

/// A guest's request. The message is encrypted on arrival and never logged.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestRequest {
    session: String,
    recipient: u128,
    class: u128,
    /// The simulated card number the processor tokenizes.
    instrument: String,
    message: String,
    /// The page's identifier for this request; a retry with the same one replays it.
    operation: u128,
}
pub async fn request(
    State(state): State<AppState>,
    Json(body): Json<GuestRequest>,
) -> Result<Json<Value>, ApiError> {
    let answer = run(&state, move |g| {
        g.request(
            &body.session,
            cs_mail_primitives::AliasId::new(body.recipient)
                .map_err(|_| ServiceError::RecipientNotReady)?,
            RequestClassId(body.class),
            &body.instrument,
            &body.message,
            body.operation,
        )
    })
    .await?;
    tracing::info!("guest request created");
    json(&answer)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestStep {
    session: String,
    recipient: u128,
    operation: u128,
}
pub async fn submit(
    State(state): State<AppState>,
    Json(body): Json<GuestStep>,
) -> Result<Json<Value>, ApiError> {
    let view = run(&state, move |g| {
        g.submit(
            &body.session,
            cs_mail_primitives::AliasId::new(body.recipient)
                .map_err(|_| ServiceError::RecipientNotReady)?,
            body.operation,
        )
    })
    .await?;
    tracing::info!("guest request submitted");
    json(&view)
}
pub async fn cancel(
    State(state): State<AppState>,
    Json(body): Json<GuestStep>,
) -> Result<Json<Value>, ApiError> {
    let view = run(&state, move |g| {
        g.cancel(
            &body.session,
            cs_mail_primitives::AliasId::new(body.recipient)
                .map_err(|_| ServiceError::RecipientNotReady)?,
            body.operation,
        )
    })
    .await?;
    tracing::info!("guest request cancelled");
    json(&view)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestStatus {
    session: String,
    recipient: u128,
}
pub async fn status(
    State(state): State<AppState>,
    Json(body): Json<GuestStatus>,
) -> Result<Json<Value>, ApiError> {
    json(
        &run(&state, move |g| {
            g.status(
                &body.session,
                cs_mail_primitives::AliasId::new(body.recipient)
                    .map_err(|_| ServiceError::RecipientNotReady)?,
            )
        })
        .await?,
    )
}

/// Sends the notices owed to guests.
pub fn notices(guests: Guests) -> impl FnMut(cs_mail_primitives::Duration, i64) + Send + 'static {
    let mut failures = diagnostics::BatchFailures::default();
    move |_lease, _batch| match guests.send_notices() {
        Ok(attempted) => {
            failures.succeeded("guest-notices");
            if attempted > 0 {
                tracing::info!(attempted, "guest notice attempts started");
            }
        }
        Err(ServiceError::Storage(error)) => {
            failures.failed("guest-notices", diagnostics::code(&error));
        }
        Err(other) => failures.failed("guest-notices", other.to_string()),
    }
}
