//! Populate the existing local account through backend contracts, using explicit
//! fictional authentication. No credential writes, payment actions or external sends.
use cs_mail_adapters::DomainIdentity;
use cs_mail_guests::GuestMailbox;
use cs_mail_primitives::CanonicalTime;
use cs_mail_privacy::PrivacyDeriver;
use cs_mail_security::RecipientSigningScope;
use cs_mail_storage_postgres::{PostgresDeployment, external::IncomingRepository};
use std::{
    env, fs,
    time::{SystemTime, UNIX_EPOCH},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 5 {
        return Err("usage: dev-email-samples CONFIG RECIPIENT SENDER MIME_FILE".into());
    }
    let config: serde_json::Value = serde_json::from_slice(&fs::read(&args[1])?)?;
    let key = |name: &str| -> Result<[u8; 32], Box<dyn std::error::Error>> {
        Ok(serde_json::from_slice(&fs::read(
            config["members"][name].as_str().ok_or("missing key path")?,
        )?)?)
    };
    let deployment =
        PostgresDeployment::connect(config["database_url"].as_str().ok_or("missing database")?)?;
    let bootstrap = deployment
        .deployment_record()?
        .ok_or("missing deployment")?;
    let scope = RecipientSigningScope {
        deployment_domain: bootstrap.record().deployment_domain,
        provider: bootstrap.record().provider,
    };
    let now = || {
        CanonicalTime(
            u64::try_from(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("system time")
                    .as_millis(),
            )
            .expect("time range"),
        )
    };
    let repository = IncomingRepository::new(deployment.accounts(now), scope).with_registration(
        PrivacyDeriver::new(
            key("relationship_secret_file")?,
            key("principal_secret_file")?,
        ),
        bootstrap,
    )?;
    let sender = GuestMailbox::parse(&args[3])?;
    let domain = DomainIdentity::parse_ascii(
        config["directory_domain"]
            .as_str()
            .ok_or("missing directory domain")?,
    )?;
    let id = cs_mail_test_support::external::admit_fictitious_email(
        &repository,
        &sender,
        &fs::read(&args[4])?,
        &args[2],
        &domain,
        now(),
    )?;
    println!("Admitted fictitious email {} through existing route", id.0);
    Ok(())
}
