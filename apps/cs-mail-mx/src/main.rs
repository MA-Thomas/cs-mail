//! Thin process host. No migration, signing, provider, or plaintext spool authority.
use clap::Parser;
use cs_mail_adapters::DomainIdentity;
use cs_mail_primitives::CanonicalTime;
use cs_mail_service::{CanonicalClock, incoming::SmtpReceiver};
use cs_mail_smtp_gateway::{DmarcLimits, StalwartDmarcVerifier, receiver};
use cs_mail_storage_postgres::{PostgresDeployment, external::IncomingRepository};
use serde::Deserialize;
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio_rustls::{
    TlsAcceptor,
    rustls::{
        ServerConfig,
        pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
    },
};
#[derive(Parser)]
#[command(version, about)]
struct Args {
    #[arg(long)]
    config: PathBuf,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    database_url: String,
    listen: SocketAddr,
    mail_domain: String,
    tls_certificate_file: PathBuf,
    tls_private_key_file: PathBuf,
    replay_secret_file: PathBuf,
    maximum_connections: usize,
}
#[derive(Clone, Copy)]
struct Clock;
impl CanonicalClock for Clock {
    fn now(&self) -> CanonicalTime {
        CanonicalTime(
            u64::try_from(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("system clock")
                    .as_millis(),
            )
            .expect("timestamp range"),
        )
    }
}
#[tokio::main]
async fn main() {
    let args = Args::parse();
    if let Err(error) = run(args).await {
        eprintln!("cs-mail-mx: {error}");
        std::process::exit(1);
    }
}
async fn run(args: Args) -> Result<(), &'static str> {
    let config: Config = serde_json::from_slice(
        &std::fs::read(args.config).map_err(|_| "cannot read configuration")?,
    )
    .map_err(|_| "invalid configuration")?;
    if !(1..=256).contains(&config.maximum_connections) {
        return Err("maximum_connections must be 1..256");
    }
    let domain = Arc::new(
        DomainIdentity::parse_ascii(&config.mail_domain).map_err(|_| "invalid mail domain")?,
    );
    let certs = CertificateDer::pem_file_iter(&config.tls_certificate_file)
        .map_err(|_| "cannot read TLS chain")?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "invalid TLS chain")?;
    let key = PrivateKeyDer::from_pem_file(&config.tls_private_key_file)
        .map_err(|_| "invalid TLS key")?;
    let tls = TlsAcceptor::from(Arc::new(
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|_| "invalid TLS configuration")?,
    ));
    let secret: [u8; 32] = serde_json::from_slice(
        &std::fs::read(config.replay_secret_file).map_err(|_| "cannot read replay secret")?,
    )
    .map_err(|_| "invalid replay secret")?;
    let verifier = StalwartDmarcVerifier::from_system_config(DmarcLimits {
        max_message_bytes: cs_mail_smtp_gateway::incoming::MAX_EMAIL_BYTES,
        max_header_bytes: 32768,
        max_header_count: 100,
        max_dkim_signatures: 8,
        ..DmarcLimits::default()
    })
    .map_err(|_| "DNS resolver unavailable")?;
    let repository = tokio::task::spawn_blocking(move || {
        let deployment = PostgresDeployment::connect_existing(&config.database_url)
            .map_err(|_| "database/schema unavailable")?;
        let record = deployment
            .deployment_record()
            .map_err(|_| "cannot read deployment")?
            .ok_or("deployment missing")?;
        let scope = cs_mail_security::RecipientSigningScope {
            deployment_domain: record.record().deployment_domain,
            provider: record.record().provider,
        };
        Ok::<_, &'static str>(IncomingRepository {
            accounts: deployment.accounts(|| Clock.now()),
            scope,
        })
    })
    .await
    .map_err(|_| "database worker failed")??;
    let receiver: Arc<dyn receiver::Receiver> = Arc::new(
        SmtpReceiver::new(repository, verifier, (*domain).clone(), secret, Clock, 8)
            .map_err(|_| "invalid receiver configuration")?,
    );
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .map_err(|_| "cannot bind listener")?;
    let sessions = Arc::new(tokio::sync::Semaphore::new(config.maximum_connections));
    eprintln!("cs-mail-mx: receiver ready");
    loop {
        let (mut stream, peer) = tokio::select! {result=listener.accept()=>result.map_err(|_| "listener failed")?, _=tokio::signal::ctrl_c()=>return Ok(())};
        let Ok(permit) = sessions.clone().try_acquire_owned() else {
            use tokio::io::AsyncWriteExt;
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(1),
                stream.write_all(b"421 4.3.2 Server busy\r\n"),
            )
            .await;
            continue;
        };
        let (tls, domain, receiver) = (tls.clone(), domain.clone(), receiver.clone());
        tokio::spawn(async move {
            let _permit = permit;
            let _ = receiver::session(stream, peer.ip(), domain, tls, receiver).await;
        });
    }
}
