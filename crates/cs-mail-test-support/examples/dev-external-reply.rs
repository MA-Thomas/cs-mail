//! Local synthetic receiver for the CLI demonstration. No listener or public API.
//! Possession of the local database and derivation secrets is required. This does
//! not verify SMTP authentication; it exercises the trusted-receiver boundary.
use cs_mail_correspondence::ConversationId;
use cs_mail_guests::GuestMailbox;
use cs_mail_primitives::{CanonicalTime, MessageId, ProtocolIdentity};
use cs_mail_storage_postgres::{
    PostgresDeployment,
    external::{Preparation, Repository},
};
use std::{
    env, fs,
    time::{SystemTime, UNIX_EPOCH},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 7 {
        return Err(
            "usage: dev-external-reply CONFIG PREPARATION MEMBER MAILBOX CONVERSATION MESSAGE"
                .into(),
        );
    }
    let config: serde_json::Value = serde_json::from_slice(&fs::read(&args[1])?)?;
    let key = |name: &str| -> Result<[u8; 32], Box<dyn std::error::Error>> {
        Ok(serde_json::from_slice(&fs::read(
            config["members"][name]
                .as_str()
                .ok_or("missing secret path")?,
        )?)?)
    };
    let deployment = PostgresDeployment::connect(
        config["database_url"]
            .as_str()
            .ok_or("missing database URL")?,
    )?;
    let preparation: Preparation = serde_json::from_slice(&fs::read(&args[2])?)?;
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
    let repository = Repository {
        accounts: deployment.accounts(now),
        deployment: deployment
            .deployment_record()?
            .ok_or("deployment not bootstrapped")?,
        deriver: cs_mail_privacy::PrivacyDeriver::new(
            key("relationship_secret_file")?,
            key("principal_secret_file")?,
        )
        .into(),
        relay_key: preparation.relay_key,
    };
    let member = ProtocolIdentity(args[3].parse()?);
    let mailbox = GuestMailbox::parse(&args[4])?;
    let current = repository.prepare(member, &mailbox)?;
    if current.relationship != preparation.relationship {
        return Err("preparation mismatch".into());
    }
    let reply = cs_mail_test_support::external::reply(
        &current,
        repository.scope(),
        ConversationId::new(args[5].parse()?)?,
        MessageId(args[6].parse()?),
        mailbox,
        now(),
        None,
    );
    let id = repository.incoming().receive(&reply)?;
    println!("admitted synthetic reply {}", id.0);
    Ok(())
}
