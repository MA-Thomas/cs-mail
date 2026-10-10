//! Development only: the identity-model enrollment service with a development login.
//!
//! It serves the identity service's real enrollment API and application, with one
//! substitution: instead of verifying an OIDC provider's signed ID token, it accepts an
//! unsigned development token that states the login's claims directly:
//!
//! `dev-oidc.` followed by the hexadecimal UTF-8 of
//! `{"issuer","subject","audience","nonce","auth_time","issued_at","expires_at"}`
//! (times in Unix seconds).
//!
//! Anyone can write such a token, so this host proves nothing about a login. It exists so
//! the core request loop can be demonstrated without an OIDC provider; the enrollment run
//! against a real provider remains separate work. Configuration is the same as
//! `identity-enrollment`'s, through environment variables.
use identity_application::enrollment::{
    Config, DecisionSigner, EnrollmentService, SystemClock, evidence::EvidenceKey,
};
use identity_model::{
    OidcClientConfig, OidcSessionVerificationError, OidcSessionVerifier, VerifiedOidcSession, time,
};
use identity_storage_postgres::enrollment::PostgresEnrollmentStore;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DevelopmentClaims {
    issuer: String,
    subject: String,
    audience: String,
    nonce: String,
    auth_time: i64,
    issued_at: i64,
    expires_at: i64,
}

/// Accepts development tokens and checks their stated claims like any session.
struct DevelopmentVerifier;
impl OidcSessionVerifier for DevelopmentVerifier {
    fn verify_session(
        &self,
        token: &str,
        config: &OidcClientConfig,
        observed_at: &identity_model::Timestamp,
    ) -> Result<VerifiedOidcSession, OidcSessionVerificationError> {
        let hex = token
            .strip_prefix("dev-oidc.")
            .ok_or(OidcSessionVerificationError::InvalidToken)?;
        let bytes = (0..hex.len())
            .step_by(2)
            .map(|i| {
                hex.get(i..i + 2)
                    .and_then(|b| u8::from_str_radix(b, 16).ok())
            })
            .collect::<Option<Vec<u8>>>()
            .ok_or(OidcSessionVerificationError::InvalidToken)?;
        let claims: DevelopmentClaims = serde_json::from_slice(&bytes)
            .map_err(|_| OidcSessionVerificationError::InvalidToken)?;
        let mut session = VerifiedOidcSession::keycloak(
            claims.issuer,
            claims.subject,
            claims.audience,
            "development-session",
            time::unix_seconds_to_timestamp(claims.issued_at),
            time::unix_seconds_to_timestamp(claims.expires_at),
        );
        session.nonce = Some(claims.nonce);
        session.auth_time = Some(time::unix_seconds_to_timestamp(claims.auth_time));
        identity_model::validate_oidc_session_context(&session, config, observed_at)?;
        Ok(session)
    }
}

fn required(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    std::env::var(name).map_err(|_| format!("{name} is not set").into())
}
fn key(name: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&std::fs::read(required(name)?)?)?)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let config = Config {
            issuer: required("IDENTITY_ISSUER")?,
            product: required("IDENTITY_PRODUCT")?,
            oidc: OidcClientConfig::keycloak(required("OIDC_ISSUER")?, required("OIDC_CLIENT_ID")?),
            product_key: key("PRODUCT_KEY_FILE")?,
            bank_key: key("BANK_KEY_FILE")?,
        };
        let (db, connection) =
            tokio_postgres::connect(&required("IDENTITY_DATABASE_URL")?, tokio_postgres::NoTls)
                .await?;
        tokio::spawn(connection);
        let service = EnrollmentService::new(
            PostgresEnrollmentStore::new(db).await?,
            config,
            DevelopmentVerifier,
            SystemClock,
            DecisionSigner::new(key("DECISION_SECRET_FILE")?),
            EvidenceKey::new(key("EVIDENCE_SECRET_FILE")?),
        )?;
        let port: u16 = required("IDENTITY_PORT")?.parse()?;
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
        eprintln!("development identity service on 127.0.0.1:{port} (development logins only)");
        axum::serve(listener, identity_enrollment::router(Arc::new(service))).await?;
        Ok(())
    })
}
