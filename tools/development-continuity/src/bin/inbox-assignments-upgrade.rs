//! Explicit schema-42 fixture cutover. Never linked into the member application.
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use cs_mail_accounts::aliases::{Alias, AliasUsage};
use cs_mail_application::{
    aliases::DeliveredRoute,
    correspondence::{MessageRecord, MessageView},
};
use cs_mail_primitives::{AliasId, CanonicalTime, IdempotencyKey, ProtocolIdentity};
use postgres::{Client, NoTls, types::Json};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, path::Path};
use zeroize::Zeroizing;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const OWNER: ProtocolIdentity = ProtocolIdentity(331288778501094320672877597944731204898);
const OPERATION: IdempotencyKey = IdempotencyKey(202610190043);
fn check(ok: bool) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err("receiving assignment preservation refused".into())
    }
}
fn save(path: &Path, value: &Value) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    if path.exists() {
        check(std::fs::read(path)? == bytes)?;
    } else {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    Ok(())
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn backend(url: &str, output: &Path, mode: &str, assigned_at: CanonicalTime) -> Result<()> {
    check(
        url.starts_with("postgresql://localhost/cs_mail_required_inboxes_") && !url.contains('?'),
    )?;
    let mut db = Client::connect(url, NoTls)?;
    let version: i64 = db
        .query_one("SELECT max(version) FROM cs_schema_migrations", &[])?
        .get(0);
    if version == 43 {
        let plan: Value = serde_json::from_slice(&std::fs::read(output)?)?;
        for (id, expected) in plan["records"].as_object().ok_or("missing records")? {
            let Json(actual): Json<Value> = db
                .query_one(
                    "SELECT record FROM cs_correspondence_messages WHERE id=$1",
                    &[id],
                )?
                .get(0);
            check(actual == *expected)?;
        }
        return Ok(());
    }
    check(version == 42 && matches!(mode, "rollback" | "apply"))?;
    let mut tx = db.transaction()?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtext('cs-mail-receiving-inbox-cutover'))",
        &[],
    )?;
    let pending: i64 = tx.query_one("SELECT (SELECT count(*) FROM cs_request_documents)+(SELECT count(*) FROM cs_external_conversations)", &[])?.get(0);
    check(pending == 0)?;
    let mut aliases = BTreeMap::new();
    for row in tx.query("SELECT record FROM cs_alias_routes ORDER BY id", &[])? {
        let Json(alias): Json<Alias> = row.get(0);
        if alias.is_current_public() {
            check(aliases.insert(alias.owner(), alias).is_none())?;
        }
    }
    check(
        aliases
            .get(&OWNER)
            .is_some_and(|a| a.address().as_str() == "marcus.thomas@c-sqdmail.com"),
    )?;
    tx.batch_execute("CREATE TABLE cs_receiving_cutover_evidence(source_table TEXT NOT NULL,record JSONB NOT NULL); REVOKE ALL ON cs_receiving_cutover_evidence FROM PUBLIC; INSERT INTO cs_receiving_cutover_evidence SELECT 'cs_correspondence_messages',to_jsonb(m) FROM cs_correspondence_messages m; INSERT INTO cs_receiving_cutover_evidence SELECT 'cs_alias_routes',to_jsonb(a) FROM cs_alias_routes a; INSERT INTO cs_receiving_cutover_evidence SELECT 'cs_conversations',to_jsonb(c) FROM cs_conversations c; INSERT INTO cs_receiving_cutover_evidence SELECT 'cs_alias_deliveries',to_jsonb(d) FROM cs_alias_deliveries d;")?;
    let mut records = serde_json::Map::new();
    for row in tx.query("SELECT m.id,m.record,c.record FROM cs_correspondence_messages m JOIN cs_conversations c ON c.id=m.conversation ORDER BY m.sequence", &[])? {
        let id: String = row.get(0); let Json(mut value): Json<Value> = row.get(1);
        let Json(conversation): Json<cs_mail_correspondence::Conversation> = row.get(2);
        let sender: ProtocolIdentity = serde_json::from_value(value["author"].clone())?;
        let recipient = conversation.scope().other(sender)?;
        if !aliases.contains_key(&recipient) {
            let row = tx.query_opt("SELECT mailbox FROM cs_address_directory WHERE address=$1", &[&recipient.0.to_string()])?;
            let mailbox = row.and_then(|r| r.get::<_, Option<String>>(0)).unwrap_or_else(|| format!("development-{}@c-sqdmail.com", recipient.0));
            let alias = Alias::create(AliasId::new(recipient.0)?, recipient, mailbox.try_into()?, AliasUsage::Public, assigned_at)?;
            tx.execute("INSERT INTO cs_alias_routes(id,owner,address,record) VALUES($1,$2,$3,$4)", &[&alias.id().value().to_string(),&recipient.0.to_string(),&alias.address().as_str(),&Json(&alias)])?;
            aliases.insert(recipient, alias);
        }
        let manifest: cs_mail_correspondence::MessageManifest = serde_json::from_value(value["manifest"].clone())?;
        check(!value.as_object().ok_or("invalid header")?.contains_key("receiving"))?;
        let route = DeliveredRoute::development_assignment(&aliases[&recipient], sender, manifest.conversation(), manifest.message(), serde_json::from_value(value["committed_at"].clone())?, OPERATION)?;
        value["receiving"] = serde_json::to_value(&route)?;
        let record: MessageRecord = serde_json::from_value(value.clone())?;
        tx.execute("UPDATE cs_correspondence_messages SET record=$2 WHERE id=$1", &[&id,&Json(&record)])?;
        records.insert(id, value);
    }
    check(records.len() == 10)?;
    let owned: Vec<Alias> = tx
        .query(
            "SELECT record FROM cs_alias_routes WHERE owner=$1 ORDER BY id",
            &[&OWNER.0.to_string()],
        )?
        .into_iter()
        .map(|r| r.get::<_, Json<Alias>>(0).0)
        .collect();
    let plan = json!({"operation":OPERATION,"assigned_at":assigned_at,"owner":OWNER,"records":records,"aliases":owned});
    tx.execute("INSERT INTO cs_receiving_cutover_evidence(source_table,record) VALUES('assignment-plan',$1)", &[&Json(&plan)])?;
    let migration = include_str!(
        "../../../../crates/cs-mail-storage-postgres/migrations/0043_required_receiving_inboxes.sql"
    );
    let structural = migration
        .split_once("ALTER TABLE cs_alias_routes")
        .ok_or("missing migration")?
        .1;
    tx.batch_execute(&format!("ALTER TABLE cs_alias_routes{structural}"))?;
    save(output, &plan)?;
    if mode == "rollback" {
        tx.rollback()?;
    } else {
        tx.commit()?;
    }
    Ok(())
}
fn decrypt(bytes: &[u8], secret: &[u8; 32], context: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    check(bytes.len() > 40)?;
    Ok(Zeroizing::new(
        XChaCha20Poly1305::new(secret.into())
            .decrypt(
                &XNonce::try_from(&bytes[..24])?,
                Payload {
                    msg: &bytes[24..],
                    aad: context,
                },
            )
            .map_err(|_| "ciphertext authentication failed")?,
    ))
}
fn encrypt(value: &Value, secret: &[u8; 32], context: &[u8]) -> Result<Vec<u8>> {
    let plain = Zeroizing::new(serde_json::to_vec(value)?);
    let mut nonce = [0; 24];
    getrandom::fill(&mut nonce)?;
    let mut bytes = nonce.to_vec();
    bytes.extend(
        XChaCha20Poly1305::new(secret.into())
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: &plain,
                    aad: context,
                },
            )
            .map_err(|_| "encryption failed")?,
    );
    Ok(bytes)
}
fn update_header(header: &mut Value, plan: &Value) -> Result<()> {
    let message: cs_mail_primitives::MessageId =
        serde_json::from_value(header["manifest"]["message"].clone())?;
    let expected = &plan["records"][message.0.to_string()];
    check(expected.is_object())?;
    let mut prior = expected.clone();
    prior
        .as_object_mut()
        .ok_or("invalid record")?
        .remove("receiving");
    check(*header == prior)?;
    *header = expected.clone();
    let _: MessageRecord = serde_json::from_value(header.clone())?;
    Ok(())
}
fn device(plan_path: &Path, output: &Path) -> Result<()> {
    check(!output.exists())?;
    let plan: Value = serde_json::from_slice(&std::fs::read(plan_path)?)?;
    let mut input = Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(128 * 1024 * 1024)
        .read_to_end(&mut input)?;
    let input: Value = serde_json::from_slice(&input)?;
    let credential = Zeroizing::new(serde_json::from_value::<Vec<u8>>(
        input["credential"].clone(),
    )?);
    let secret: Value = serde_json::from_slice(&credential)?;
    let key = Zeroizing::new(serde_json::from_value::<[u8; 32]>(secret["cache"].clone())?);
    let old_cache: Vec<u8> = serde_json::from_value(input["cache"].clone())?;
    let mut state: Value = serde_json::from_slice(&decrypt(
        &old_cache,
        &key,
        b"cs-mail/apple-member-cache/v4",
    )?)?;
    let profile = state["Active"]["profile"].clone();
    check(profile["persona"] == serde_json::to_value(OWNER)?)?;
    for name in [
        "pending",
        "sending",
        "requesting",
        "editing_conversation",
        "setup",
    ] {
        check(state["Active"][name].is_null())?;
    }
    check(state["Active"]["external"].is_null())?;
    for row in state["Active"]["mailbox"]["records"]
        .as_array_mut()
        .ok_or("missing mailbox")?
    {
        update_header(&mut row[1], &plan)?;
    }
    if !input["personal"].is_null() {
        let encrypted: Vec<u8> = serde_json::from_value(input["personal"].clone())?;
        let personal: Value =
            serde_json::from_slice(&decrypt(&encrypted, &key, b"cs-mail/personal-mailbox/v1")?)?;
        check(
            personal["account"] == profile["account"] && personal["persona"] == profile["persona"],
        )?;
        check(personal["work"].as_array().is_some_and(Vec::is_empty))?;
    }
    let old_meta: Vec<u8> = serde_json::from_value(input["metadata"].clone())?;
    let mut metadata: Value =
        serde_json::from_slice(&decrypt(&old_meta, &key, b"cs-mail/retained-metadata/v2")?)?;
    check(metadata["owner"] == profile["persona"] && metadata["account"] == profile["account"])?;
    let aliases: Vec<Alias> = serde_json::from_value(plan["aliases"].clone())?;
    metadata["aliases"] = serde_json::to_value(&aliases)?;
    check(
        metadata
            .as_object_mut()
            .ok_or("invalid metadata")?
            .remove("routes")
            .is_some(),
    )?;
    let content: [u8; 32] = serde_json::from_value(secret["content"].clone())?;
    let reference = serde_json::from_value(profile["content_key"].clone())?;
    let (endpoint, public) =
        cs_mail_content::EndpointSecretKey::from_secret_bytes(reference, &content)?;
    let owner: ProtocolIdentity = serde_json::from_value(profile["persona"].clone())?;
    let mut copies = vec![];
    for row in input["copies"].as_array().ok_or("missing copies")? {
        let id: Vec<u8> = serde_json::from_value(row["id"].clone())?;
        check(id.len() == 32)?;
        let context = [b"cs-mail/retained-copy/v2".as_slice(), &id].concat();
        let old: Vec<u8> = serde_json::from_value(row["ciphertext"].clone())?;
        let mut entry: Value = serde_json::from_slice(&decrypt(&old, &key, &context)?)?;
        check(!entry["native"].is_null())?;
        update_header(&mut entry["native"]["record"], &plan)?;
        let view: MessageView = serde_json::from_value(entry["native"].clone())?;
        let document = cs_mail_client::correspondence::open(
            &cs_mail_client::correspondence::EndpointKeys {
                owner,
                secret: &endpoint,
                public,
            },
            &view,
            view.record.sender_key,
        )?;
        check(serde_json::to_value(document.subject())? == entry["subject"])?;
        copies
            .push(json!({"id":id,"before":hash(&old),"ciphertext":encrypt(&entry,&key,&context)?}));
    }
    check(copies.len() == 10 && state["Active"]["profile"] == profile)?;
    save(
        output,
        &json!({"owner":owner,"account":profile["account"],"original_cache":hash(&old_cache),"original_metadata":hash(&old_meta),"cache":encrypt(&state,&key,b"cs-mail/apple-member-cache/v4")?,"metadata":encrypt(&metadata,&key,b"cs-mail/retained-metadata/v2")?,"copies":copies,"revision":input["revision"]}),
    )?;
    Ok(())
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("backend") if args.len() == 6 => backend(
            &args[2],
            Path::new(&args[3]),
            &args[4],
            CanonicalTime(args[5].parse()?),
        ),
        Some("device") if args.len() == 4 => device(Path::new(&args[2]), Path::new(&args[3])),
        _ => Err("invalid maintenance arguments".into()),
    }
}
fn main() {
    if run().is_err() {
        eprintln!("Receiving-inbox cutover refused; preserve source and artifacts.");
        std::process::exit(1);
    }
}
