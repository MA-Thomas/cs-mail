//! Explicit offline owner-fixture change. Never linked into the member app or server.
//! Keeps the protocol identity, account, registered keys and historical receipts intact.
use cs_mail_accounts::{
    aliases::{Alias, AliasPortfolio, AliasUsage},
    directory::{DirectoryEntry, MemberAddress},
};
use cs_mail_primitives::{AliasId, CanonicalTime, ProtocolIdentity};
use postgres::{Client, NoTls, types::Json};
use serde_json::{Value, json};
use std::{collections::BTreeMap, error::Error, path::Path, time::SystemTime};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const OWNER: &str = "331288778501094320672877597944731204898";
const ORIGINAL: &str = "marcus@csqd-mail.com";
const PROTOCOL: &str = "mtallis62831@c-sqdmail.com";

fn require(value: bool) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err("Owner fixture preservation check failed".into())
    }
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    require(args.len() == 4)?;
    let database = &args[1];
    require(matches!(
        database.as_str(),
        "postgresql://localhost/cs_mail_alias_assessment_check_20261016"
            | "postgresql://localhost/cs_mail_alias_assessment_owner_20261016"
    ))?;
    require(matches!(args[3].as_str(), "apply" | "rollback"))?;
    let path = Path::new(&args[2]);
    let mut db = Client::connect(database, NoTls)?;
    let mut tx = db.transaction()?;
    // Offline operation: refuse a concurrently running service, rather than coordinating
    // an administrative write with the member's live commands.
    require(tx.query_one("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid()", &[])?.get::<_, i64>(0) == 0)?;
    tx.batch_execute("LOCK TABLE cs_address_directory,cs_alias_routes IN EXCLUSIVE MODE")?;
    require(
        tx.query_one("SELECT max(version) FROM cs_schema_migrations", &[])?
            .get::<_, i64>(0)
            == 40,
    )?;
    let owner = ProtocolIdentity(OWNER.parse()?);
    let account: String = tx
        .query_one(
            "SELECT account FROM cs_persona_owners WHERE identity=$1",
            &[&OWNER],
        )?
        .get(0);
    let row = tx.query_one(
        "SELECT mailbox,entry FROM cs_address_directory WHERE address=$1",
        &[&OWNER],
    )?;
    let current: Value = row.get::<_, Json<Value>>(1).0;
    let plan: Value = if path.exists() {
        serde_json::from_slice(&std::fs::read(path)?)?
    } else {
        require(row.get::<_, String>(0) == ORIGINAL)?;
        require(
            tx.query_one("SELECT count(*) FROM cs_alias_routes", &[])?
                .get::<_, i64>(0)
                == 0,
        )?;
        let at = CanonicalTime(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_millis()
                .try_into()?,
        );
        let mut aliases = BTreeMap::<AliasId, Alias>::new();
        for (id, address, description) in [
            (62831001, "marcus.thomas@c-sqdmail.com", None),
            (62831002, "MAT7771@c-sqdmail.com", Some("NYC-Hospital")),
            (62831003, "m11a33t@c-sqdmail.com", Some("Brooklyn-Lab-Co")),
        ] {
            let usage = match description {
                Some(value) => AliasUsage::Private {
                    description: value.to_owned().try_into()?,
                },
                None => AliasUsage::Public,
            };
            let alias = AliasPortfolio::new(owner, &aliases)?.create(
                AliasId::new(id)?,
                MemberAddress::try_from(address.to_owned())?,
                usage,
                at,
            )?;
            aliases.insert(alias.id(), alias);
        }
        // This is an explicit development-fixture correction, not an ordinary profile
        // rename. Retain the prior directory entry and every signed historical command.
        let mut replacement = current.clone();
        replacement["mailbox"] = json!(PROTOCOL);
        replacement["revision"] = json!(current["revision"].as_u64().ok_or("revision")? + 1);
        let checked: DirectoryEntry = serde_json::from_value(replacement)?;
        let plan = json!({"purpose":"User-requested local alias assessment; no live email assignment",
            "owner":owner,"account":account,"before":current,"after":checked,
            "aliases":aliases.values().collect::<Vec<_>>()});
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut output = options.open(path)?;
        output.write_all(&serde_json::to_vec_pretty(&plan)?)?;
        output.sync_all()?;
        plan
    };
    require(plan["owner"] == json!(owner) && plan["account"] == json!(account))?;
    let after: DirectoryEntry = serde_json::from_value(plan["after"].clone())?;
    require(after.address() == owner && after.mailbox().is_some_and(|m| m.as_str() == PROTOCOL))?;
    let records: Vec<Alias> = serde_json::from_value(plan["aliases"].clone())?;
    let aliases: BTreeMap<_, _> = records.into_iter().map(|a| (a.id(), a)).collect();
    require(aliases.len() == 3)?;
    AliasPortfolio::new(owner, &aliases)?;
    if current == plan["after"] {
        require(args[3] == "apply")?;
        for alias in aliases.values() {
            let stored: Alias = tx
                .query_one(
                    "SELECT record FROM cs_alias_routes WHERE id=$1",
                    &[&alias.id().value().to_string()],
                )?
                .get::<_, Json<Alias>>(0)
                .0;
            require(stored == *alias)?;
        }
        require(
            tx.query_one("SELECT count(*) FROM cs_alias_routes", &[])?
                .get::<_, i64>(0)
                == 3,
        )?;
        println!(
            "PASS exact completed scenario reconciled; account and protocol identity unchanged"
        );
        return Ok(());
    }
    require(current == plan["before"] && row.get::<_, String>(0) == ORIGINAL)?;
    require(
        tx.query_one("SELECT count(*) FROM cs_alias_routes", &[])?
            .get::<_, i64>(0)
            == 0,
    )?;
    require(
        tx.execute(
            "UPDATE cs_address_directory SET mailbox=$1,entry=$2 WHERE address=$3 AND mailbox=$4",
            &[&PROTOCOL, &Json(&after), &OWNER, &ORIGINAL],
        )? == 1,
    )?;
    for alias in aliases.values() {
        tx.execute(
            "INSERT INTO cs_alias_routes(id,owner,address,record) VALUES($1,$2,$3,$4)",
            &[
                &alias.id().value().to_string(),
                &OWNER,
                &alias.address().as_str(),
                &Json(alias),
            ],
        )?;
    }
    if args[3] == "rollback" {
        tx.rollback()?;
        println!("PASS deliberate rollback after all scenario writes");
    } else {
        tx.commit()?;
        println!(
            "PASS requested addresses and checked alias grants committed atomically; account and protocol identity unchanged"
        );
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!(
            "Scenario update did not report success: {error}. Preserve the plan and database for reconciliation."
        );
        std::process::exit(1);
    }
}
