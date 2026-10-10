//! Signed member commands only. No public endpoint accepts claimed email authentication.
use crate::http::{ApiError, AppState};
use axum::{Json, extract::State, http::StatusCode};
use cs_mail_application::correspondence::external::SignedCommand;
use cs_mail_service::ServiceError;
use cs_mail_storage_postgres::StorageError;

fn error(e: ServiceError) -> ApiError {
    match e {
        ServiceError::Storage(StorageError::Correspondence(e)) => {
            ApiError::refusal(StatusCode::CONFLICT, format!("external:{e:?}"))
        }
        ServiceError::Storage(e) => e.into(),
        _ => ApiError::unavailable(),
    }
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prepare {
    member: cs_mail_primitives::ProtocolIdentity,
    mailbox: cs_mail_guests::GuestMailbox,
}
pub async fn prepare(
    State(state): State<AppState>,
    Json(input): Json<Prepare>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let gateway = state.external.clone().ok_or_else(ApiError::unavailable)?;
    let value = state
        .blocking
        .run(move || {
            gateway
                .external_repository()?
                .prepare(input.member, &input.mailbox)
                .map_err(ServiceError::from)
        })
        .await
        .map_err(|_| ApiError::unavailable())?
        .map_err(error)?;
    Ok(Json(
        serde_json::to_value(value).map_err(|_| ApiError::unavailable())?,
    ))
}
pub async fn command(
    State(state): State<AppState>,
    Json(signed): Json<SignedCommand>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let gateway = state.external.clone().ok_or_else(ApiError::unavailable)?;
    let value = state
        .blocking
        .run(move || gateway.external_command(&signed))
        .await
        .map_err(|_| ApiError::unavailable())?
        .map_err(error)?;
    Ok(Json(
        serde_json::to_value(value).map_err(|_| ApiError::unavailable())?,
    ))
}

/// A development relay handoff captured on disk, not an SMTP implementation.
#[cfg(feature = "simulation")]
fn development_worker(
    gateway: Gateway,
    directory: std::path::PathBuf,
    domain: cs_mail_adapters::DomainIdentity,
) -> impl FnMut(cs_mail_primitives::Duration, i64) + Send + 'static {
    struct Directory(std::path::PathBuf, cs_mail_adapters::DomainIdentity);
    impl cs_mail_service::external::Transport for Directory {
        fn send(
            &mut self,
            dispatch: &cs_mail_application::correspondence::external::Dispatch,
            document: &cs_mail_correspondence::Document,
        ) -> cs_mail_service::external::Handoff {
            use cs_mail_service::external::Handoff;
            let id = dispatch.message().manifest.message().0;
            let body = serde_json::json!({ "development_relay_handoff": true,
                "message": id, "conversation": dispatch.conversation().id(),
                "from_member": dispatch.conversation().member(), "to": dispatch.conversation().mailbox(),
                "reply_to": cs_mail_smtp_gateway::incoming::reply_address(dispatch.conversation().id(), &self.1),
                "document": document });
            let result = (|| -> Result<(), std::io::Error> {
                use std::io::Write;
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
                let mut file = options.open(self.0.join(format!("{id}.external.json")))?;
                file.write_all(body.to_string().as_bytes())?;
                file.sync_all()
            })();
            if result.is_ok() {
                Handoff::Accepted(
                    cs_mail_correspondence::external::delivery::ProviderReference::new(format!(
                        "development:{id}"
                    ))
                    .expect("generated reference"),
                )
            } else {
                Handoff::Unknown
            }
        }
    }
    worker(gateway, Directory(directory, domain))
}
fn worker(
    gateway: Gateway,
    mut transport: impl cs_mail_service::external::Transport + Send + 'static,
) -> impl FnMut(cs_mail_primitives::Duration, i64) + Send + 'static {
    move |_, batch| {
        let Ok(repository) = gateway.external_repository() else {
            return;
        };
        if let Ok(ids) = repository.waiting(batch.clamp(1, 1000)) {
            for id in ids {
                if gateway.dispatch_external(id, &mut transport).is_err() {
                    tracing::warn!("external dispatch failed; inspect delivery state");
                }
            }
        }
    }
}

pub type Gateway = std::sync::Arc<
    cs_mail_service::external::ExternalGateway<cs_mail_service::UnixMillisecondClock>,
>;

/// Select exactly one delivery adapter. No fall-through from real sending to a file.
pub fn configure(
    config: &crate::config::ExternalConfig,
    members: crate::members::Gateway,
    workers: &mut crate::Workers,
    worker_config: crate::config::WorkerConfig,
) -> Result<(Gateway, Option<crate::callbacks::Receiver>), String> {
    use crate::config::{ExternalDelivery, secret};
    let gateway = std::sync::Arc::new(cs_mail_service::external::ExternalGateway::new(
        members,
        secret(&config.content_secret_file).map_err(|e| e.to_string())?,
    ));
    let domain = cs_mail_adapters::DomainIdentity::parse_ascii(&config.mail_domain)
        .map_err(|_| "invalid mail domain")?;
    let feedback =
        crate::callbacks::Receiver::configure(&config.feedback, &domain, gateway.clone())?;
    match &config.delivery {
        ExternalDelivery::Disabled => {}
        #[cfg(feature = "simulation")]
        ExternalDelivery::Development { directory } => {
            std::fs::create_dir_all(directory).map_err(|_| "cannot create development outbox")?;
            workers.start(
                "external-delivery",
                worker_config,
                development_worker(gateway.clone(), directory.clone(), domain),
            );
        }
        ExternalDelivery::Smtp2go {
            api_key_file,
            dkim_secret_file,
            dkim_selector,
            region,
        } => {
            let key =
                std::fs::read_to_string(api_key_file).map_err(|_| "cannot read SMTP2GO API key")?;
            let carrier =
                cs_mail_smtp_gateway::smtp2go::Smtp2go::new(region.endpoint(), key.trim())
                    .map_err(|_| "invalid SMTP2GO configuration")?;
            let dkim = cs_mail_smtp_gateway::outbound::DkimIdentity::from_pkcs8_pem(
                domain.as_str(),
                dkim_selector,
                &std::fs::read(dkim_secret_file).map_err(|_| "cannot read DKIM key")?,
            )
            .map_err(|_| "invalid RSA-2048 DKIM configuration")?;
            let transport = cs_mail_service::external::Smtp2goTransport {
                carrier,
                dkim,
                domain,
            };
            workers.start(
                "external-delivery",
                worker_config,
                worker(gateway.clone(), transport),
            );
        }
    }
    Ok((gateway, feedback))
}
