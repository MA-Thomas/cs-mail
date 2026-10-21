//! Offline conversion of the explicitly selected development copy. No runtime predecessor reader.
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use cs_mail_client::correspondence::{EndpointKeys, open, prepare};
use cs_mail_content::{EncryptedContentRecord, EndpointPublicKey, EndpointSecretKey};
use cs_mail_correspondence::{ConversationId, ConversationSubject, Document, DocumentPart};
use cs_mail_primitives::{CanonicalTime, ContentKeyRef, ProtocolIdentity};
use postgres::{Client, NoTls, types::Json};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, path::Path};
use zeroize::Zeroizing;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn require(value: bool) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err("preservation condition failed".into())
    }
}
fn number(value: &Value) -> Result<u128> {
    Ok(value.to_string().trim_matches('"').parse()?)
}
fn key(value: &Value) -> Result<[u8; 32]> {
    Ok(serde_json::from_value(value.clone())?)
}
fn read(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}
fn decrypt(bytes: &[u8], secret: &[u8; 32], context: &[u8]) -> Result<Value> {
    require(bytes.len() > 40)?;
    let plain = Zeroizing::new(
        XChaCha20Poly1305::new(secret.into())
            .decrypt(
                &XNonce::try_from(&bytes[..24])?,
                Payload {
                    msg: &bytes[24..],
                    aad: context,
                },
            )
            .map_err(|_| "cache authentication failed")?,
    );
    Ok(serde_json::from_slice(&plain)?)
}
fn encrypt(value: &Value, secret: &[u8; 32], context: &[u8]) -> Result<Vec<u8>> {
    let plain = Zeroizing::new(serde_json::to_vec(value)?);
    let mut nonce = [0; 24];
    getrandom::fill(&mut nonce)?;
    let mut output = nonce.to_vec();
    output.extend(
        XChaCha20Poly1305::new(secret.into())
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: &plain,
                    aad: context,
                },
            )
            .map_err(|_| "cache encryption failed")?,
    );
    Ok(output)
}
fn remap(value: &mut Value, versions: &BTreeMap<String, Value>) {
    match value {
        Value::Object(fields) => {
            for (name, field) in fields {
                if name == "version" {
                    if let Some(version) = versions.get(&field.to_string()) {
                        *field = version.clone();
                        continue;
                    }
                }
                remap(field, versions);
            }
        }
        Value::Array(values) => {
            for value in values {
                remap(value, versions);
            }
        }
        _ => (),
    }
}
fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    require(args.len() == 6)?;
    let database = &args[1];
    let root = Path::new(&args[2]);
    let output = Path::new(&args[3]);
    let mode = &args[4];
    require(
        database == "postgresql://localhost/cs_mail_subject_search_check_20261019"
            || database == "postgresql://localhost/cs_mail_subject_search_owner_20261019",
    )?;
    require(mode == "rollback" || mode == "convert")?;
    // Exact retries inspect only committed metadata; they need no credential access.
    let mut probe = Client::connect(database, NoTls)?;
    if probe
        .query_one("SELECT max(version) FROM cs_schema_migrations", &[])?
        .get::<_, i64>(0)
        == 42
    {
        require(mode == "convert" && output.join("converted.json").exists())?;
        let prepared = read(&output.join("converted.json"))?;
        let association: String = probe
            .query_one(
                "SELECT account FROM cs_persona_owners WHERE identity=$1",
                &[&prepared["persona"]
                    .as_str()
                    .ok_or("missing prepared persona")?],
            )?
            .get(0);
        require(prepared["account"].as_str() == Some(association.as_str()))?;
        println!("PASS exact committed retry; no credential access or data writes");
        return Ok(());
    }
    drop(probe);
    let mut bytes = Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(128 * 1024 * 1024)
        .read_to_end(&mut bytes)?;
    let input: Value = serde_json::from_slice(&bytes)?;
    let credential = Zeroizing::new(serde_json::from_value::<Vec<u8>>(
        input["credential"].clone(),
    )?);
    let secrets: Value = serde_json::from_slice(&credential)?;
    let cache_key = Zeroizing::new(key(&secrets["cache"])?);
    let cache: Vec<u8> = serde_json::from_value(input["cache"].clone())?;
    let mut local = decrypt(&cache, &cache_key, b"cs-mail/apple-member-cache/v3")?;
    let active = &mut local["Active"];
    require(active["format"] == 3)?;
    for name in [
        "sending",
        "pending",
        "requesting",
        "setup",
        "editing_conversation",
    ] {
        require(active[name].is_null())?;
    }
    require(
        active["external"].is_null()
            || active["external"]["records"]
                .as_object()
                .is_some_and(serde_json::Map::is_empty),
    )?;
    let owner = ProtocolIdentity(number(&active["profile"]["persona"])?);
    let original_profile = active["profile"].clone();
    let (owner_secret, owner_public) = EndpointSecretKey::from_secret_bytes(
        ContentKeyRef(number(&original_profile["content_key"])?),
        &key(&secrets["content"])?,
    )?;
    let custody_value = read(Path::new(&args[5]))?;
    let (custody_secret, custody_public) =
        EndpointSecretKey::from_secret_bytes(ContentKeyRef(900), &key(&custody_value)?)?;
    let mut endpoints = BTreeMap::new();
    endpoints.insert(owner, (owner_secret, owner_public));
    for name in ["maya", "daniel", "elena", "jules"] {
        let participant = read(&root.join("participants").join(name).join("member.json"))?;
        let ids = cs_mail_application::accounts::EnrollmentIdentifiers::for_operation(
            participant["enrollment"]
                .as_str()
                .ok_or("missing fixture identity")?,
        );
        let endpoint = EndpointSecretKey::from_secret_bytes(
            ContentKeyRef(number(&participant["content_key"])?),
            &key(&participant["content_secret"])?,
        )?;
        endpoints.insert(ids.persona, endpoint);
    }
    let mut db = Client::connect(database, NoTls)?;
    let mut tx = db.transaction()?;
    let schema: i64 = tx
        .query_one("SELECT max(version) FROM cs_schema_migrations", &[])?
        .get(0);
    if schema == 42 {
        require(mode == "convert" && output.join("converted.json").exists())?;
        println!("PASS already-committed conversion; prepared encrypted device artifacts retained");
        return Ok(());
    }
    require(schema == 41)?;
    require(
        tx.query_one("SELECT count(*) FROM cs_external_conversations", &[])?
            .get::<_, i64>(0)
            == 0,
    )?;
    require(
        tx.query_one("SELECT count(*) FROM cs_request_documents", &[])?
            .get::<_, i64>(0)
            == 0,
    )?;
    let associated: String = tx
        .query_one(
            "SELECT account FROM cs_persona_owners WHERE identity=$1",
            &[&owner.0.to_string()],
        )?
        .get(0);
    require(associated == number(&original_profile["account"])?.to_string())?;
    // Archive original evidence verbatim, including receipts; it is never read at runtime.
    tx.batch_execute("CREATE TABLE cs_subject_cutover_evidence(source_table TEXT NOT NULL,record JSONB NOT NULL); REVOKE ALL ON cs_subject_cutover_evidence FROM PUBLIC;")?;
    for table in [
        "cs_conversations",
        "cs_correspondence_messages",
        "cs_correspondence_copies",
        "cs_correspondence_recovery",
        "cs_correspondence_receipts",
    ] {
        tx.execute(
            &format!(
                "INSERT INTO cs_subject_cutover_evidence SELECT $1,to_jsonb(t) FROM {table} t"
            ),
            &[&table],
        )?;
    }
    let mut subjects = BTreeMap::new();
    for row in tx.query("SELECT c.id,c.record,coalesce(min((m.record->>'committed_at')::bigint),0) FROM cs_conversations c LEFT JOIN cs_correspondence_messages m ON m.conversation=c.id GROUP BY c.id,c.record ORDER BY c.id", &[])? {
        let id: String = row.get(0); let Json(mut record): Json<Value> = row.get(1); let at: i64 = row.get(2);
        let mut hash = Sha256::new(); hash.update(b"cs-mail/placeholder-subject-conversion/v1"); hash.update(*cache_key); hash.update(id.as_bytes());
        let subject = ConversationSubject::new("", CanonicalTime(u64::try_from(at)?), -14_400, hash.finalize().into())?;
        record["subject"] = serde_json::to_value(subject.commitment())?;
        let _: cs_mail_correspondence::Conversation = serde_json::from_value(record.clone())?;
        tx.execute("UPDATE cs_conversations SET record=$2 WHERE id=$1", &[&id, &Json(record)])?;
        subjects.insert(id, subject);
    }
    let mut versions = BTreeMap::new();
    let mut records = BTreeMap::new();
    let mut converted = 0;
    for row in tx.query(
        "SELECT id,conversation,record FROM cs_correspondence_messages ORDER BY sequence",
        &[],
    )? {
        let id: String = row.get(0);
        let conversation: String = row.get(1);
        let Json(mut record): Json<Value> = row.get(2);
        let sender = ProtocolIdentity(number(&record["author"])?);
        let sender_public: EndpointPublicKey =
            serde_json::from_value(record["sender_key"].clone())?;
        let subject = subjects.get(&conversation).ok_or("missing conversation")?;
        let copies = tx.query(
            "SELECT owner,ciphertext FROM cs_correspondence_copies WHERE message=$1 ORDER BY owner",
            &[&id],
        )?;
        let recoveries = tx.query(
            "SELECT owner,record FROM cs_correspondence_recovery WHERE message=$1 ORDER BY owner",
            &[&id],
        )?;
        if let Some(recovery) = recoveries.first() {
            let Json(recovery): Json<cs_mail_application::consent::RecoveryCopy> = recovery.get(1);
            let plain = Zeroizing::new(cs_mail_content::decrypt(
                &custody_secret,
                sender_public,
                &recovery.ciphertext,
            )?);
            let mut value: Value = serde_json::from_slice(&plain)?;
            let old_parts = value["parts"].clone();
            remap(&mut value["parts"], &versions);
            let parts: Vec<DocumentPart> = serde_json::from_value(value["parts"].clone())?;
            let document =
                Document::new(parts, key(&value["version_secret"])?)?.with_subject(subject.clone());
            // Source identity and selected text remain; only authenticated versions change.
            let mut restored_parts = value["parts"].clone();
            let reverse: BTreeMap<_, _> = versions
                .iter()
                .map(|(old, new)| {
                    (
                        new.to_string(),
                        serde_json::from_str(old).expect("recorded version"),
                    )
                })
                .collect();
            remap(&mut restored_parts, &reverse);
            require(restored_parts == old_parts)?;
            let (sender_secret, registered) =
                endpoints.get(&sender).ok_or("unknown fixture sender")?;
            require(*registered == sender_public)?;
            let other = recovery.ciphertext.binding.recipient;
            let recipient = if other == sender {
                // choose the other participant from the conversation scope
                let Json(c): Json<Value> = tx
                    .query_one(
                        "SELECT record FROM cs_conversations WHERE id=$1",
                        &[&conversation],
                    )?
                    .get(0);
                c["scope"]
                    .as_array()
                    .ok_or("invalid pair")?
                    .iter()
                    .map(number)
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .find(|p| *p != sender.0)
                    .map(ProtocolIdentity)
                    .ok_or("missing participant")?
            } else {
                other
            };
            let recipient_public = endpoints
                .get(&recipient)
                .ok_or("unknown fixture recipient")?
                .1;
            let binding_row = copies
                .iter()
                .find(|r| r.get::<_, String>(0) == recipient.0.to_string())
                .or_else(|| copies.first())
                .ok_or("missing endpoint copy")?;
            let Json(old_copy): Json<EncryptedContentRecord> = binding_row.get(1);
            let mut binding = old_copy.binding.clone();
            binding.recipient = recipient;
            let package = prepare(
                &EndpointKeys {
                    owner: sender,
                    secret: sender_secret,
                    public: sender_public,
                },
                &document,
                ConversationId::new(conversation.parse()?)?,
                recipient_public,
                custody_public,
                binding,
                (old_copy.created_at, old_copy.expires_at),
            )?;
            versions.insert(
                record["manifest"]["version"].to_string(),
                serde_json::to_value(package.manifest.version())?,
            );
            record["manifest"] = serde_json::to_value(&package.manifest)?;
            let typed: cs_mail_application::correspondence::MessageRecord =
                serde_json::from_value(record.clone())?;
            for copy in &copies {
                let who: String = copy.get(0);
                let next = if who == sender.0.to_string() {
                    &package.sender_copy
                } else {
                    require(who == recipient.0.to_string())?;
                    &package.recipient_copy
                };
                tx.execute("UPDATE cs_correspondence_copies SET ciphertext=$3 WHERE message=$1 AND owner=$2", &[&id, &who, &Json(next)])?;
                let identity = ProtocolIdentity(who.parse()?);
                let (secret, public) = endpoints.get(&identity).ok_or("missing endpoint")?;
                let opened = open(
                    &EndpointKeys {
                        owner: identity,
                        secret,
                        public: *public,
                    },
                    &cs_mail_application::correspondence::MessageView {
                        record: typed.clone(),
                        ciphertext: next.clone(),
                    },
                    sender_public,
                )?;
                require(opened.parts() == document.parts() && opened.subject() == Some(subject))?;
            }
            for recovery in &recoveries {
                let who: String = recovery.get(0);
                let next = if who == sender.0.to_string() {
                    &package.sender_recovery
                } else {
                    require(who == recipient.0.to_string())?;
                    &package.recipient_recovery
                };
                tx.execute(
                    "UPDATE cs_correspondence_recovery SET record=$3 WHERE message=$1 AND owner=$2",
                    &[&id, &who, &Json(next)],
                )?;
            }
            converted += 1;
        } else {
            require(copies.is_empty())?;
            record["manifest"]["subject"] = serde_json::to_value(subject.commitment())?;
        }
        tx.execute(
            "UPDATE cs_correspondence_messages SET record=$2 WHERE id=$1",
            &[&id, &Json(&record)],
        )?;
        records.insert(id, record);
    }
    // Historical creation receipts retain the absence of a subject at that time.
    tx.batch_execute("UPDATE cs_correspondence_receipts SET receipt=jsonb_set(receipt,'{outcome,Conversation,subject}','null'::jsonb) WHERE receipt#>'{outcome,Conversation}' IS NOT NULL;")?;
    for entry in active["mailbox"]["records"]
        .as_array_mut()
        .ok_or("invalid mailbox")?
    {
        let id = number(&entry[1]["manifest"]["message"])?.to_string();
        entry[1] = records.get(&id).ok_or("unmatched local header")?.clone();
    }
    active["format"] = json!(4);
    require(active["profile"] == original_profile)?;
    let mut workspace_output = None;
    if !input["workspace"].is_null() {
        let bytes: Vec<u8> = serde_json::from_value(input["workspace"].clone())?;
        let mut workspace = decrypt(&bytes, &cache_key, b"cs-mail/member-workspace/v1")?;
        require(workspace["submitted"].is_null())?;
        remap(&mut workspace, &versions);
        for draft in workspace["drafts"].as_array_mut().ok_or("invalid drafts")? {
            draft["subject"] = json!("");
            draft["local_offset_seconds"] = json!(0);
        }
        workspace_output = Some(encrypt(
            &workspace,
            &cache_key,
            b"cs-mail/member-workspace/v2",
        )?);
    }
    if !input["personal"].is_null() {
        let personal: Vec<u8> = serde_json::from_value(input["personal"].clone())?;
        require(
            decrypt(&personal, &cache_key, b"cs-mail/personal-mailbox/v1")?["work"]
                .as_array()
                .is_some_and(Vec::is_empty),
        )?;
    }
    let cache_output = encrypt(&local, &cache_key, b"cs-mail/apple-member-cache/v4")?;
    tx.batch_execute("INSERT INTO cs_schema_migrations(version) VALUES(42)")?;
    if mode == "rollback" {
        tx.rollback()?;
        println!(
            "PASS full subject/copy conversion rollback; original local identity and writing unchanged"
        );
        return Ok(());
    }
    std::fs::create_dir_all(output)?;
    write(&output.join("cache.ciphertext"), &cache_output)?;
    if let Some(bytes) = workspace_output {
        write(&output.join("workspace.ciphertext"), &bytes)?;
    }
    write(
        &output.join("converted.json"),
        &serde_json::to_vec_pretty(
            &json!({"schema":42,"messages":converted,"subjects":subjects.len(),"original_cache_sha256":format!("{:x}",Sha256::digest(&cache)),"account":associated,"persona":owner.0.to_string()}),
        )?,
    )?;
    tx.commit()?;
    println!(
        "PASS converted {converted} placeholder message documents; account, keys, copies, text and financial facts preserved"
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Subject conversion refused: {error}");
        std::process::exit(1);
    }
}
