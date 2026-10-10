//! Development only: operator scenario commands for the durable payment simulator, and
//! the payment workers that use it in place of a provider.
use crate::http::{ApiError, AppState, now};
use axum::Json;
use axum::extract::State;
use cs_mail_operations::{OperationsError, SignedOperatorCommand};
use cs_mail_simulation::{
    PostgresSimulator, SimulationCommand, SimulationError, SimulationOutcome,
};
use cs_mail_storage_postgres::StorageError;

fn refused(error: &SimulationError) -> ApiError {
    tracing::warn!(error = ?error_code(error), "simulation command refused");
    match error {
        SimulationError::Operations(e) => StorageError::Operations(*e).into(),
        _ => ApiError::unavailable(),
    }
}
fn error_code(error: &SimulationError) -> &'static str {
    match error {
        SimulationError::Database(_) => "database",
        SimulationError::Serialization(_) => "record",
        SimulationError::Operations(_) => "operations",
        SimulationError::Payment(_) => "payment",
    }
}

pub async fn command(
    State(state): State<AppState>,
    Json(signed): Json<SignedOperatorCommand<SimulationCommand>>,
) -> Result<Json<SimulationOutcome>, ApiError> {
    let Some(simulator) = state.simulator.clone() else {
        return Err(ApiError::unavailable());
    };
    let deployment = state.deployment.clone();
    let outcome = state
        .blocking
        .run(move || -> Result<SimulationOutcome, ApiError> {
            let authority = deployment
                .deployment_record()?
                .ok_or(StorageError::Operations(OperationsError::NotBootstrapped))?
                .operator_authority();
            let mut simulator = simulator.lock().map_err(|_| ApiError::unavailable())?;
            simulator
                .execute(&authority, &signed, now)
                .map_err(|e| refused(&e))
        })
        .await
        .map_err(|_| ApiError::unavailable())??;
    tracing::info!(
        ?outcome,
        "simulation command answered (a repeat returns the recorded outcome)"
    );
    Ok(Json(outcome))
}

/// Connects the simulator and checks that the deployment trusts its key.
/// # Errors
/// A connection failure, or a deployment that trusts a different processor key.
pub fn connect(
    database_url: &str,
    secret: [u8; 32],
    trusted_processor: [u8; 32],
) -> Result<PostgresSimulator, String> {
    let simulator = PostgresSimulator::connect(database_url, secret)
        .map_err(|e| format!("simulator: {}", error_code(&e)))?;
    if simulator.verifying_key() != trusted_processor {
        return Err("the deployment does not trust the simulator's processor key".into());
    }
    Ok(simulator)
}

/// The simulated processor's tokenization of guest cards.
pub struct Cards(pub cs_mail_simulation::SimulatedCards);
impl cs_mail_service::guests::InstrumentTokens for Cards {
    fn tokenize(
        &self,
        instrument: &str,
        holder: &cs_mail_guests::GuestMailbox,
    ) -> Result<([u8; 32], cs_mail_guests::InstrumentFingerprint), String> {
        self.0
            .tokenize(instrument, &holder.address())
            .map(|(token, fingerprint)| (token, cs_mail_guests::InstrumentFingerprint(fingerprint)))
            .map_err(|_| "declined".to_owned())
    }
}

/// The simulated bank as the enrollment bank-evidence source. The request names a
/// simulated account and its owner:
/// `{"account": "...", "owner": {"name": "...", "address": "...", "phone": "..."}}`.
pub struct Bank(pub cs_mail_simulation::SimulatedBank);
impl crate::enrollment::BankEvidence for Bank {
    fn verification(
        &self,
        scope: cs_mail_finance::FinancialScope,
        unit: cs_mail_primitives::SettlementUnit,
        identifiers: cs_mail_application::accounts::EnrollmentIdentifiers,
        request: &serde_json::Value,
    ) -> Result<cs_mail_finance::BankVerification, ApiError> {
        let account: cs_mail_simulation::SimulatedBankAccount =
            serde_json::from_value(request.clone()).map_err(|_| {
                ApiError::from_status(axum::http::StatusCode::BAD_REQUEST, "invalid-bank-request")
            })?;
        self.0
            .verification(
                scope,
                unit,
                identifiers.billing,
                identifiers.member,
                &account,
            )
            .map_err(|_| {
                ApiError::from_status(axum::http::StatusCode::BAD_REQUEST, "invalid-bank-request")
            })
    }
    fn ownership(
        &self,
        intent: &identity_contract::EnrollmentIntent,
        verification: &cs_mail_finance::BankVerification,
        request: &serde_json::Value,
        login: (&str, &str),
        now: i64,
    ) -> Result<identity_contract::SignedBankOwnership, ApiError> {
        let account: cs_mail_simulation::SimulatedBankAccount =
            serde_json::from_value(request.clone()).map_err(|_| {
                ApiError::from_status(axum::http::StatusCode::BAD_REQUEST, "invalid-bank-request")
            })?;
        self.0
            .ownership(intent, &account, verification, login, now)
            .map_err(|_| {
                ApiError::from_status(axum::http::StatusCode::BAD_REQUEST, "invalid-bank-request")
            })
    }
}
