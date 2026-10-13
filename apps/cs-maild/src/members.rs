//! The member API: signed commands, publications and reads for the core request loop.
//! Handlers pass signed input to the member gateway and account and billing services on
//! the blocking pool, and render typed outcomes. Error bodies carry a stable code only.
#[cfg(feature = "simulation")]
use crate::diagnostics;
use crate::http::{ApiError, AppState};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use cs_mail_accounts::control::SignedAccountCommand;
use cs_mail_application::accounts::operations::AccountService;
use cs_mail_billing::SignedBillingCommand;
use cs_mail_capabilities::SignedLaneGrant;
use cs_mail_primitives::{IdempotencyKey, ProtocolIdentity};
use cs_mail_security::{
    SignedCommandBytes, SignedEndpointKey, SignedMemberRead, SignedRequestClasses,
};
use cs_mail_service::members::MemberGateway;
use cs_mail_service::{ServiceError, UnixMillisecondClock};
use cs_mail_storage_postgres::StorageError;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

pub type Gateway = Arc<MemberGateway<UnixMillisecondClock>>;

fn refused(error: ServiceError) -> ApiError {
    match error {
        ServiceError::Storage(StorageError::Identity(identity_contract::Error::Invalid)) => {
            ApiError::from_status(StatusCode::NOT_FOUND, "unknown-address")
        }
        ServiceError::Storage(StorageError::Protocol(e)) => {
            ApiError::refusal(StatusCode::CONFLICT, format!("protocol:{e:?}"))
        }
        ServiceError::Storage(StorageError::Security(e)) | ServiceError::Security(e) => {
            tracing::warn!(error = ?e, "member request not authorized");
            ApiError::from_status(StatusCode::UNAUTHORIZED, "unauthorized")
        }
        ServiceError::Storage(StorageError::AdmissionRefused(_))
        | ServiceError::ContentTimeInvalid
        | ServiceError::MessageValidityClosed => {
            ApiError::from_status(StatusCode::BAD_REQUEST, "content-refused")
        }
        ServiceError::Storage(error) => error.into(),
        ServiceError::NoProtocolPolicy => {
            ApiError::from_status(StatusCode::SERVICE_UNAVAILABLE, "no-protocol-policy")
        }
        ServiceError::SigningScopeMismatch => {
            ApiError::from_status(StatusCode::BAD_REQUEST, "scope")
        }
        ServiceError::Dmarc(_)
        | ServiceError::ReceiptUnavailable
        | ServiceError::Outbox
        | ServiceError::PaymentMethod
        | ServiceError::NotFunded
        | ServiceError::NoRequest
        | ServiceError::RecipientNotReady => ApiError::unavailable(),
    }
}
fn gateway(state: &AppState) -> Result<Gateway, ApiError> {
    state
        .members
        .clone()
        .ok_or_else(|| ApiError::from_status(StatusCode::SERVICE_UNAVAILABLE, "no-member-service"))
}
async fn run<T: Send + 'static>(
    state: &AppState,
    call: impl FnOnce(&MemberGateway<UnixMillisecondClock>) -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    let gateway = gateway(state)?;
    state
        .blocking
        .run(move || call(&gateway))
        .await
        .map_err(|_| ApiError::unavailable())?
}
fn json<T: serde::Serialize>(value: &T) -> Result<Json<Value>, ApiError> {
    serde_json::to_value(value)
        .map(Json)
        .map_err(|_| ApiError::unavailable())
}

/// What a client signs under: the deployment and provider, the product its account
/// commands name, and the current protocol policy.
pub async fn deployment(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let browser_login = state.browser_login.clone();
    let mail_domain = state.mail_domain.clone();
    let (scope, product, policy, provider_key, custody_key) = run(&state, |g| {
        Ok((
            g.scope(),
            g.record().record().identity.product.clone(),
            g.policy().map_err(refused)?,
            g.signer().verifying_key_bytes(),
            g.custody_key().map_err(refused)?,
        ))
    })
    .await?;
    Ok(Json(
        json!({ "scope": scope, "product": product, "policy": policy, "provider_key": provider_key,"custody_key":custody_key, "browser_login": browser_login, "mail_domain": mail_domain }),
    ))
}

/// An account command signed by a manager key: registering a key, adding an address.
pub async fn account_command(
    State(state): State<AppState>,
    Json(signed): Json<SignedAccountCommand>,
) -> Result<Json<Value>, ApiError> {
    let control = run(&state, move |g| {
        let accounts = g.deployment().accounts(crate::http::now);
        AccountService::new(&accounts, &crate::http::now)
            .execute_command(&signed)
            .map_err(ApiError::from)
    })
    .await?;
    tracing::info!("account command committed");
    json(&control)
}

/// A billing command signed by an account key: purchasing annual service.
pub async fn billing_command(
    State(state): State<AppState>,
    Json(signed): Json<SignedBillingCommand>,
) -> Result<Json<Value>, ApiError> {
    let outcome = run(&state, move |g| {
        g.billing_response(&signed).map_err(refused)
    })
    .await?;
    json(&outcome)
}

/// A recipient address's request classes.
pub async fn request_classes(
    State(state): State<AppState>,
    Json(signed): Json<SignedRequestClasses>,
) -> Result<Json<Value>, ApiError> {
    run(&state, move |g| g.publish_classes(&signed).map_err(refused)).await?;
    tracing::info!("request classes published");
    Ok(Json(json!({ "published": true })))
}

/// What a sender may choose when approaching an address.
pub async fn offer(
    State(state): State<AppState>,
    Path(persona): Path<u128>,
) -> Result<Json<Value>, ApiError> {
    let offer = run(&state, move |g| {
        g.offer(ProtocolIdentity(persona)).map_err(refused)
    })
    .await?;
    match offer {
        Some(offer) => json(&offer),
        None => Err(ApiError::from_status(StatusCode::NOT_FOUND, "no-offer")),
    }
}

/// An address's endpoint content key.
pub async fn publish_endpoint_key(
    State(state): State<AppState>,
    Json(signed): Json<SignedEndpointKey>,
) -> Result<Json<Value>, ApiError> {
    run(&state, move |g| {
        g.publish_endpoint_key(&signed).map_err(refused)
    })
    .await?;
    tracing::info!("endpoint key published");
    Ok(Json(json!({ "published": true })))
}
pub async fn endpoint_key(
    State(state): State<AppState>,
    Path(persona): Path<u128>,
) -> Result<Json<Value>, ApiError> {
    match run(&state, move |g| {
        g.endpoint_key(ProtocolIdentity(persona)).map_err(refused)
    })
    .await?
    {
        Some(key) => json(&key),
        None => Err(ApiError::from_status(
            StatusCode::NOT_FOUND,
            "no-endpoint-key",
        )),
    }
}

/// A participant's signed relationship command.
pub async fn relationship_command(
    State(state): State<AppState>,
    Path((sender, recipient)): Path<(u128, u128)>,
    Json(signed): Json<SignedCommandBytes>,
) -> Result<Json<Value>, ApiError> {
    let outcome = run(&state, move |g| {
        g.submit(
            ProtocolIdentity(sender),
            ProtocolIdentity(recipient),
            &signed,
        )
        .map_err(refused)
    })
    .await?;
    tracing::info!(kind = ?outcome.receipt.payload.kind, "relationship command answered");
    json(&outcome)
}

/// The sender's ciphertext for a message it will submit.
pub async fn content(
    State(state): State<AppState>,
    Path((sender, recipient)): Path<(u128, u128)>,
    Json(upload): Json<cs_mail_application::requests::documents::SignedRequestDocument>,
) -> Result<Json<Value>, ApiError> {
    let receipt = run(&state, move |g| {
        g.upload_request_document(
            ProtocolIdentity(sender),
            ProtocolIdentity(recipient),
            &upload,
        )
        .map_err(refused)
    })
    .await?;
    tracing::info!("content stored");
    json(&receipt)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaneAcceptance {
    grant: SignedLaneGrant,
    idempotency_key: u128,
}
/// A recipient's acceptance with an express lane.
pub async fn lane(
    State(state): State<AppState>,
    Json(acceptance): Json<LaneAcceptance>,
) -> Result<Json<Value>, ApiError> {
    let outcome = run(&state, move |g| {
        g.grant_lane(
            &acceptance.grant,
            IdempotencyKey(acceptance.idempotency_key),
        )
        .map_err(refused)
    })
    .await?;
    tracing::info!(kind = ?outcome.receipt.payload.kind, "lane grant answered");
    json(&outcome)
}

/// A member's authenticated read of its own records.
pub async fn read(
    State(state): State<AppState>,
    Json(signed): Json<SignedMemberRead>,
) -> Result<Json<Value>, ApiError> {
    let answer = run(&state, move |g| g.read(&signed).map_err(refused)).await?;
    json(&answer)
}

/// Development only: the relationship worker, using the simulator in place of a payment
/// provider.
#[cfg(feature = "simulation")]
pub fn worker(
    gateway: Gateway,
    mut processor: cs_mail_simulation::PostgresSimulator,
) -> impl FnMut(cs_mail_primitives::Duration, i64) + Send + 'static {
    let mut failures = diagnostics::BatchFailures::default();
    move |lease, batch| match gateway.run_relationship_work(&mut processor, lease, batch) {
        Ok(report) => {
            failures.succeeded("relationships");
            if report.relationships > 0 {
                tracing::info!(
                    relationships = report.relationships,
                    schedules = report.schedules,
                    payments = report.payments,
                    deliveries = report.deliveries,
                    failures = report.failures,
                    "relationship work"
                );
            }
        }
        Err(ServiceError::NoProtocolPolicy) => {
            failures.failed("relationships", "no-protocol-policy".into());
        }
        Err(ServiceError::Storage(error)) => {
            failures.failed("relationships", diagnostics::code(&error));
        }
        Err(_) => failures.failed("relationships", "service".into()),
    }
}

/// Typed native commands: transport object-field order has no authority.
pub async fn correspondence(
    State(state): State<AppState>,
    Json(request): Json<cs_mail_application::correspondence::SignedRequest>,
) -> Result<Json<cs_mail_application::correspondence::Response>, ApiError> {
    run(&state, move |g| {
        let repository = g.deployment().accounts(crate::http::now);
        cs_mail_application::correspondence::CorrespondenceService::new(&repository)
            .handle(&request)
            .map_err(ApiError::from)
    })
    .await
    .map(Json)
}
