//! Local browser fixture host: the existing identity application and signed-JWT verifier.
//! Only this development executable permits a loopback HTTP OIDC fixture.
use identity_adapters::oidc::OidcJwksSessionVerifier;
use identity_application::enrollment::{
    Config, DecisionSigner, EnrollmentService, SystemClock, evidence::EvidenceKey,
};
use identity_model::OidcClientConfig;
use identity_storage_postgres::enrollment::PostgresEnrollmentStore;
use std::{env, future::IntoFuture, sync::Arc};
fn required(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(env::var(name)?)
}
fn key(name: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    // Secret files are JSON arrays of 32 bytes; never print their contents.
    Ok(serde_json::from_slice(&std::fs::read(required(name)?)?)?)
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config {
        issuer: required("IDENTITY_ISSUER")?,
        product: required("IDENTITY_PRODUCT")?,
        oidc: OidcClientConfig::keycloak(required("OIDC_ISSUER")?, required("OIDC_CLIENT_ID")?),
        product_key: key("PRODUCT_KEY_FILE")?,
        bank_key: key("BANK_KEY_FILE")?,
    };
    if !config.oidc.issuer.starts_with("http://127.0.0.1:") {
        return Err("this test host requires a loopback OIDC fixture".into());
    }
    let url = required("IDENTITY_DATABASE_URL")?;
    // This executable intentionally supports a local DB only. Remote deployment hosts
    // construct a TLS-configured Client and call the library API.
    let database_config: tokio_postgres::Config = url.parse()?;
    if database_config.get_hosts().iter().any(|host| matches!(host,tokio_postgres::config::Host::Tcp(name) if name != "localhost" && name != "127.0.0.1" && name != "::1"))
        || database_config.get_hostaddrs().iter().any(|address| !address.is_loopback()) {
        return Err("the loopback host requires a local PostgreSQL connection".into());
    }

    let (db, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await?;
    let mut connection_task = tokio::spawn(connection);
    let service = EnrollmentService::new(
        PostgresEnrollmentStore::new(db).await?,
        config,
        OidcJwksSessionVerifier::new(),
        SystemClock,
        DecisionSigner::new(key("DECISION_SECRET_FILE")?),
        // Keys the fingerprints enrollment evidence is compared by; changing it
        // disconnects new enrollments from recorded evidence.
        EvidenceKey::new(key("EVIDENCE_SECRET_FILE")?),
    )?;
    let app = identity_enrollment::router(Arc::new(service));
    let port: u16 = required("IDENTITY_PORT")?.parse()?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let server = axum::serve(listener, app).with_graceful_shutdown(async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            eprintln!("identity shutdown signal failed: {error}");
        }
    });
    tokio::select! {
        result = server.into_future() => { result?; },
        result = &mut connection_task => { result??; return Err("identity database connection closed".into()); }
    }
    connection_task.abort();
    match connection_task.await {
        Ok(result) => result?,
        Err(error) if error.is_cancelled() => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
