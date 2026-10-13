//! Explicit, local, offline conversion of the preserved October 11 development snapshot.
//! Never linked into an app or service. No old reader exists in the current runtime.
//! Secret input arrives through a pipe; only ciphertext and public checks leave this tool.
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use cs_mail_application::{
    correspondence::MessageRecord, requests::documents::RequestConversation,
};
use cs_mail_client::correspondence::{EndpointKeys, compose, open, prepare, readable_text};
use cs_mail_content::{EncryptedContentRecord, EndpointSecretKey};
use cs_mail_correspondence::{Conversation, ConversationId, CorrespondenceRef, DocumentPart};
use cs_mail_primitives::{CanonicalTime, ContentKeyRef, ProtocolIdentity};
use cs_mail_protocol::RelationshipRequest;
use postgres::{Client, NoTls, types::Json};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
};
use zeroize::Zeroizing;

mod align_acceptances;
mod artifact;
mod current_cache;
mod replies;
mod verify_replies;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[track_caller]
fn require(value: bool) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(format!(
            "preservation check failed at {}",
            std::panic::Location::caller()
        )
        .into())
    }
}
fn number(value: &Value) -> Result<u128> {
    Ok(value.to_string().trim_matches('"').parse()?)
}
fn key(value: &Value) -> Result<[u8; 32]> {
    Ok(serde_json::from_value(value.clone())?)
}
fn read_private(path: &Path) -> Result<Value> {
    let bytes = Zeroizing::new(std::fs::read(path)?);
    Ok(serde_json::from_slice(&bytes)?)
}
fn public_key(
    tx: &mut postgres::Transaction<'_>,
    persona: u128,
) -> Result<cs_mail_content::EndpointPublicKey> {
    let row = tx.query_one("SELECT publication FROM cs_endpoint_key_publications p JOIN cs_endpoint_keys c USING(persona,version) WHERE persona=$1", &[&persona.to_string()])?;
    let Json(value): Json<Value> = row.try_get(0)?;
    Ok(serde_json::from_value(value["publication"]["key"].clone())?)
}
// Reconstruct exclusively from the original recipient signature, then require byte-for-byte
// equality of every previous lane field. NativeSender remains the message-origin constraint.
fn preserved_lane(
    tx: &mut postgres::Transaction<'_>,
    id: &str,
    old: &Value,
) -> Result<cs_mail_capabilities::Lane> {
    let Json(signed):Json<cs_mail_capabilities::SignedLaneGrant>=tx.query_one("SELECT operation->'Grant' FROM cs_received_commands WHERE operation#>>'{Grant,grant,id}'=$1", &[&id])?.try_get(0)?;
    let Json(registry):Json<Value>=tx.query_one("SELECT a.registry FROM cs_product_accounts a JOIN cs_persona_owners p ON p.account=a.id WHERE p.identity=$1", &[&signed.grant.recipient.0.to_string()])?.try_get(0)?;
    let reference = signed.grant.recipient_operational_key.0.to_string();
    let record = &registry["keys"][&reference];
    require(
        record["actor"]["Recipient"] == serde_json::to_value(signed.grant.recipient)?
            && record["status"] == "Active",
    )?;
    let lane =
        cs_mail_capabilities::VerifiedLaneGrant::verify(&signed, &key(&record["verifying_key"])?)?
            .activate()?;
    let mut generated = serde_json::to_value(&lane)?;
    generated
        .as_object_mut()
        .ok_or("invalid lane")?
        .remove("authority");
    require(&generated == old)?;
    Ok(lane)
}
struct MemoryStore {
    secret: Zeroizing<Vec<u8>>,
    cache: Mutex<Vec<u8>>,
    cache_path: Option<std::path::PathBuf>,
}
impl cs_mail_client::member::DeviceStore for MemoryStore {
    fn load_secret(&self) -> std::result::Result<Vec<u8>, cs_mail_client::member::Error> {
        Ok(self.secret.to_vec())
    }
    fn create_secret(&self, _: &[u8]) -> std::result::Result<(), cs_mail_client::member::Error> {
        Err(cs_mail_client::member::Error::AlreadyConfigured)
    }
    fn load_cache(&self) -> std::result::Result<Option<Vec<u8>>, cs_mail_client::member::Error> {
        Ok(Some(self.cache.lock().unwrap().clone()))
    }
    fn save_cache(&self, bytes: &[u8]) -> std::result::Result<(), cs_mail_client::member::Error> {
        if let Some(path) = &self.cache_path {
            artifact::durable(path, bytes)
                .map_err(|_| cs_mail_client::member::Error::StorageUnavailable)?;
        }
        *self.cache.lock().unwrap() = bytes.to_vec();
        Ok(())
    }
}
fn main() {
    if let Err(error) = run() {
        if let Some(db) = error.downcast_ref::<postgres::Error>() {
            eprintln!("Database conversion error: {:?}", db.code());
        } else {
            eprintln!("Conversion check: {error}");
        }
        eprintln!(
            "The operation did not report success. Preserve prepared artifacts and rerun the same arguments to reconcile a possibly committed outcome. No replacement identity or credential was created."
        );
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    require(args.len() == 6)?;
    let database = &args[1];
    let simulate_ack_loss =
        std::env::var("CS_MAIL_CONTINUITY_SIMULATE_COMMIT_ACK_LOSS").as_deref() == Ok("1");
    require(
        !simulate_ack_loss
            || database == "postgresql://localhost/cs_mail_continuity_repair_check_20261012"
            || database == "postgresql://localhost/cs_mail_continuity_current_check_20261013"
            || database == "postgresql://localhost/cs_mail_acceptance_alignment_check_20261013",
    )?;
    let root = Path::new(&args[2]);
    let output = Path::new(&args[3]);
    let mode = &args[4];
    let custody_file = Path::new(&args[5]);
    require(
        database == "postgresql://localhost/cs_mail_continuity_20261012"
            || database == "postgresql://localhost/cs_mail_continuity_repair_check_20261012"
            || database == "postgresql://localhost/cs_mail_continuity_current_check_20261013"
            || database == "postgresql://localhost/cs_mail_acceptance_alignment_check_20261013"
            || database
                == "postgresql://localhost/cs_mail_acceptance_alignment_withdrawal_check_20261013"
            || (database == "postgresql://localhost/cs_mail_continuity_rollback_20261012"
                && mode == "rollback"),
    )?;
    require(matches!(
        mode.as_str(),
        "rollback"
            | "convert"
            | "verify"
            | "lane-check"
            | "lane-convert"
            | "repair-replies"
            | "verify-replies"
            | "align-acceptances"
            | "check-alignment-withdrawal"
    ))?;
    require(
        !output.exists()
            || matches!(
                mode.as_str(),
                "align-acceptances"
                    | "check-alignment-withdrawal"
                    | "verify"
                    | "lane-check"
                    | "lane-convert"
                    | "repair-replies"
                    | "verify-replies"
                    | "convert"
            ),
    )?;
    let mut input = Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(64 * 1024 * 1024)
        .read_to_end(&mut input)?;
    let input: Value = serde_json::from_slice(&input)?;
    let credential = Zeroizing::new(serde_json::from_value::<Vec<u8>>(
        input["credential"].clone(),
    )?);
    let secrets: Value = serde_json::from_slice(&credential)?;
    let old_cache: Vec<u8> = serde_json::from_value(input["cache"].clone())?;
    require(old_cache.len() > 40)?;
    let cache_key = Zeroizing::new(key(&secrets["cache"])?);
    let cipher = XChaCha20Poly1305::new((&*cache_key).into());
    let plain = Zeroizing::new(
        cipher
            .decrypt(
                &XNonce::try_from(&old_cache[..24]).map_err(|_| "invalid cache nonce")?,
                Payload {
                    msg: &old_cache[24..],
                    aad: b"cs-mail/apple-member-cache/v1",
                },
            )
            .map_err(|_| "cache authentication failed")?,
    );
    let mut state: Value = serde_json::from_slice(&plain)?;
    let old = &state["Active"];
    require(old["format"] == 1 && old["pending"].is_null() && old["setup"].is_null())?;
    require(old["profile"]["server"] == "http://127.0.0.1:9693")?;
    let profile = old["profile"].clone();
    let owner = number(&profile["persona"])?;
    let account = number(&profile["account"])?;
    let (owner_secret, owner_public) = EndpointSecretKey::from_secret_bytes(
        ContentKeyRef(number(&profile["content_key"])?),
        &key(&secrets["content"])?,
    )?;
    let custody_secret = read_private(custody_file)?;
    let (_, custodian) =
        EndpointSecretKey::from_secret_bytes(ContentKeyRef(900), &key(&custody_secret)?)?;
    let mut db = Client::connect(database, NoTls)?;
    let mut tx = db.transaction()?;
    let owner_account: String = tx
        .query_one(
            "SELECT account FROM cs_persona_owners WHERE identity=$1",
            &[&owner.to_string()],
        )?
        .try_get(0)?;
    require(owner_account == account.to_string() && public_key(&mut tx, owner)? == owner_public)?;
    let Json(registry): Json<Value> = tx
        .query_one(
            "SELECT registry FROM cs_product_accounts WHERE id=$1",
            &[&owner_account],
        )?
        .try_get(0)?;
    for (reference, secret) in [("sender_key", "device"), ("recipient_key", "recipient")] {
        let k = &registry["keys"][number(&profile[reference])?.to_string()];
        require(
            key(&k["verifying_key"])?
                == ed25519_dalek::SigningKey::from_bytes(&key(&secrets[secret])?)
                    .verifying_key()
                    .to_bytes(),
        )?;
    }
    let Json(directory): Json<Value> = tx
        .query_one(
            "SELECT entry FROM cs_address_directory WHERE address=$1",
            &[&owner.to_string()],
        )?
        .try_get(0)?;
    require(directory["mailbox"] == "marcus@csqd-mail.com")?;
    if mode == "check-alignment-withdrawal" {
        require(
            database
                == "postgresql://localhost/cs_mail_acceptance_alignment_withdrawal_check_20261013",
        )?;
        tx.rollback()?;
        return align_acceptances::check_withdrawal(database, root, output, &profile);
    }
    if mode == "align-acceptances" {
        tx.rollback()?;
        return align_acceptances::run(
            &mut db,
            database,
            root,
            output,
            &profile,
            simulate_ack_loss,
        );
    }
    if mode == "lane-check" || mode == "lane-convert" {
        for row in tx.query("SELECT lane_id,lane FROM cs_capability_lanes", &[])? {
            let id: String = row.try_get(0)?;
            let Json(old): Json<Value> = row.try_get(1)?;
            let lane = preserved_lane(&mut tx, &id, &old)?;
            if mode == "lane-convert" {
                tx.execute(
                    "INSERT INTO cs_development_continuity_archive(kind,record) VALUES('lane',$1)",
                    &[&Json(&old)],
                )?;
                tx.execute(
                    "UPDATE cs_capability_lanes SET lane=$2 WHERE lane_id=$1",
                    &[&id, &Json(lane)],
                )?;
            }
        }
        if mode == "lane-check" {
            tx.rollback()?;
        } else {
            tx.commit()?;
        }
        println!(
            "Original recipient signature verified. Reconstructed lane exactly matches every previous field; NativeSender constraint remains unchanged."
        );
        return Ok(());
    }
    if mode == "repair-replies" {
        let schema: i64 = tx
            .query_one("SELECT max(version) FROM cs_schema_migrations", &[])?
            .get(0);
        require(schema == 34 || schema == 35 || schema == 36)?;
        if output.exists() || output.with_extension("continuity.json").exists() {
            artifact::recover(output, &profile, schema == 36)?;
            if schema == 36 {
                tx.rollback()?;
                println!("Original-account upgrade reconciled; prepared v3 cache retained.");
                return Ok(());
            }
        }

        if schema == 34 {
            let migration = std::fs::read_to_string(
                root.parent()
                    .ok_or("missing workspace")?
                    .parent()
                    .ok_or("missing workspace")?
                    .join("crates/cs-mail-storage-postgres/migrations/0035_billing_refusals.sql"),
            )?;
            let structural = migration
                .split_once("ALTER TABLE")
                .ok_or("missing migration body")?
                .1;
            tx.batch_execute(&format!("ALTER TABLE{structural}"))?;
        }
        let restored = replies::repair(&mut tx, owner, root)?;
        let current = current_cache::read_current(Path::new(&std::env::var(
            "CS_MAIL_CONTINUITY_CURRENT_CACHE",
        )?))?;
        let upgraded = current_cache::upgrade(&current, &secrets, &profile, &mut tx)?;
        let store = Arc::new(MemoryStore {
            secret: Zeroizing::new(credential.to_vec()),
            cache: Mutex::new(upgraded.clone()),
            cache_path: None,
        });
        cs_mail_client::member::MemberClient::new(
            store,
            Arc::new(cs_mail_client::member::HttpTransport::new()?),
            Arc::new(|| CanonicalTime(0)),
        )
        .cached_inbox()?;
        artifact::prepare(output, &profile, &upgraded)?;
        tx.commit()?;
        println!(
            "Recorded {restored} conversation-reply corrections from original delivery evidence. Reverse-lane agreement, financial records, account and credential remain unchanged."
        );
        return Ok(());
    }
    if mode == "verify" || mode == "verify-replies" {
        tx.rollback()?;
        if mode == "verify-replies" {
            let locked: bool = db
                .query_one("SELECT pg_try_advisory_lock(20261013, 36)", &[])?
                .get(0);
            require(locked)?;
        }
        let check_path = output.with_extension("reply-check-cache");
        let initial = if mode == "verify-replies" && check_path.exists() {
            std::fs::read(&check_path)?
        } else {
            std::fs::read(output)?
        };
        let store = Arc::new(MemoryStore {
            secret: credential,
            cache: Mutex::new(initial),
            cache_path: (mode == "verify-replies").then_some(check_path),
        });
        let client = cs_mail_client::member::MemberClient::new(
            store,
            Arc::new(cs_mail_client::member::HttpTransport::new()?),
            Arc::new(|| {
                CanonicalTime(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64,
                )
            }),
        );
        let inbox = client.sync()?;
        require(inbox.conversations.len() >= 4)?;
        if mode == "verify-replies" {
            verify_replies::check(&client, root, owner)?;
        }
        for subject in ["maya", "daniel", "elena", "jules"] {
            let participant =
                read_private(&root.join("participants").join(subject).join("member.json"))?;
            let ids = cs_mail_application::accounts::EnrollmentIdentifiers::for_operation(
                participant["enrollment"]
                    .as_str()
                    .ok_or("missing enrollment")?,
            );
            let conversation = client.conversation(&ids.persona.0.to_string())?;
            require(!conversation.messages.is_empty())?;
        }
        println!(
            "Original credential authenticates; four sample contacts and conversations are readable through the current client."
        );
        return Ok(());
    }
    if mode == "convert" && (output.exists() || output.with_extension("continuity.json").exists()) {
        let schema: i64 = tx
            .query_one("SELECT max(version) FROM cs_schema_migrations", &[])?
            .get(0);
        require(schema == 32 || schema == 36)?;
        artifact::recover(output, &profile, schema == 36)?;
        if schema == 36 {
            tx.rollback()?;
            println!(
                "Prepared encrypted cache reconciled to the committed database; account identity preserved."
            );
            return Ok(());
        }
    }
    require(
        tx.query_one("SELECT max(version) FROM cs_schema_migrations", &[])?
            .try_get::<_, i64>(0)?
            == 32,
    )?;
    require(
        tx.query_one(
            "SELECT count(*) FROM cs_received_commands WHERE outcome IS NULL",
            &[],
        )?
        .try_get::<_, i64>(0)?
            == 0,
    )?;
    require(
        tx.query_one("SELECT count(*) FROM cs_conversations", &[])?
            .try_get::<_, i64>(0)?
            == 0,
    )?;
    require(
        tx.query_one("SELECT count(*) FROM cs_requests", &[])?
            .try_get::<_, i64>(0)?
            == 4,
    )?;
    // Structural cutover only: no live database resets and no historical consent invented.
    tx.batch_execute("ALTER TABLE cs_canonical_journal DROP CONSTRAINT cs_canonical_journal_pkey; ALTER TABLE cs_canonical_journal ADD PRIMARY KEY(position,aggregate_key); INSERT INTO cs_schema_migrations(version) VALUES(33);")?;
    let migration =
        std::fs::read_to_string(
            root.parent().unwrap().parent().unwrap().join(
                "crates/cs-mail-storage-postgres/migrations/0034_native_request_documents.sql",
            ),
        )?;
    let structural = migration
        .split_once("ALTER TABLE")
        .ok_or("missing structural migration")?
        .1;
    tx.batch_execute(&format!("ALTER TABLE{structural}"))?;
    for row in tx.query("SELECT aggregate_key,relationship_state FROM cs_relationship_aggregates ORDER BY aggregate_key",&[])? {
        let id:String=row.try_get(0)?; let Json(mut value):Json<Value>=row.try_get(1)?;
        value["relationship"]["reply_epoch"]=json!(0);
        let _:cs_mail_protocol::Relationship=serde_json::from_value(value["relationship"].clone())?;
        tx.execute("UPDATE cs_relationship_aggregates SET relationship_state=$2,protocol_format_version=10 WHERE aggregate_key=$1",&[&id,&Json(value)])?;
    }
    // Keep exact signed evidence in an explicitly frozen development archive. Expired
    // quotes become replay tombstones; they cannot be presented as newly signed terms.
    tx.batch_execute("CREATE TABLE cs_development_continuity_archive(kind TEXT NOT NULL,record JSONB NOT NULL); INSERT INTO cs_development_continuity_archive SELECT 'policy-pointer',to_jsonb(p) FROM cs_protocol_policy_current p; DELETE FROM cs_protocol_policy_current; INSERT INTO cs_development_continuity_archive SELECT 'quote',to_jsonb(q) FROM cs_contact_quotes q; INSERT INTO cs_development_continuity_archive SELECT 'request',to_jsonb(r) FROM cs_requests r; INSERT INTO cs_quote_tombstones SELECT aggregate_key,quote_id FROM cs_contact_quotes ON CONFLICT DO NOTHING; DELETE FROM cs_contact_quotes;")?;
    for row in tx.query("SELECT lane_id,lane FROM cs_capability_lanes", &[])? {
        let id: String = row.try_get(0)?;
        let Json(old): Json<Value> = row.try_get(1)?;
        let lane = preserved_lane(&mut tx, &id, &old)?;
        tx.execute(
            "INSERT INTO cs_development_continuity_archive(kind,record) VALUES('lane',$1)",
            &[&Json(&old)],
        )?;
        tx.execute(
            "UPDATE cs_capability_lanes SET lane=$2 WHERE lane_id=$1",
            &[&id, &Json(lane)],
        )?;
    }
    let mut preserved = 0;
    for row in tx.query("SELECT r.aggregate_key,r.request_id,r.request,m.message,e.record FROM cs_requests r JOIN cs_messages m ON m.aggregate_key=r.aggregate_key AND m.message_id=r.request->>'initial_message' JOIN cs_encrypted_content e ON e.aggregate_key=m.aggregate_key AND e.content_ref=m.message->>'content' ORDER BY r.request_id",&[])? {
        let aggregate:String=row.try_get(0)?;let request_id:String=row.try_get(1)?; let Json(mut request):Json<Value>=row.try_get(2)?;
        let Json(message):Json<Value>=row.try_get(3)?; let Json(encrypted):Json<EncryptedContentRecord>=row.try_get(4)?;
        let request_number=number(&request["id"])?;
        let provenance=tx.query_one("SELECT position,operation#>'{Protocol,key}' FROM cs_received_commands WHERE aggregate_key=$1 AND operation#>>'{Protocol,command,CreateRequest,request_id}'=$2",&[&aggregate,&request_id])?;
        let creation:i64=provenance.try_get(0)?; let Json(creation_key):Json<Value>=provenance.try_get(1)?;
        request["creation_key"]=creation_key;request["creation_event"]=json!(creation);request["terms"]["reply_lane"]=Value::Null;
        let typed:RelationshipRequest=serde_json::from_value(request.clone())?;
        require(typed.id.0==request_number && typed.terms.recipient.0==owner)?;
        let sender=typed.terms.sender;
        let participant_path=["maya","daniel","elena","jules"].into_iter().find_map(|subject|{
            let p=root.join("participants").join(subject).join("member.json");
            let v=read_private(&p).ok()?;
            let ids=cs_mail_application::accounts::EnrollmentIdentifiers::for_operation(v["enrollment"].as_str()?);
            (ids.persona==sender).then_some(p)
        }).ok_or("missing original fixture endpoint")?;
        let participant=read_private(&participant_path)?;
        let (sender_secret,sender_public)=EndpointSecretKey::from_secret_bytes(ContentKeyRef(number(&participant["content_key"])?),&key(&participant["content_secret"])?)?;
        require(public_key(&mut tx,sender.0)?==sender_public)?;
        let old_text=Zeroizing::new(cs_mail_content::decrypt(&owner_secret,sender_public,&encrypted)?);
        let text=std::str::from_utf8(&old_text)?;
        let document=compose(vec![DocumentPart::Text(text.into())])?;
        let conversation_id=ConversationId::new(request_number)?;
        let pair=CorrespondenceRef::new(sender,ProtocolIdentity(owner))?;
        let conversation=Conversation::begun_by_request(conversation_id,pair);
        let submission:i64=tx.query_one("SELECT journal_position FROM cs_protocol_events WHERE aggregate_key=$1 AND event#>>'{kind,RequestSubmitted}'=$2",&[&aggregate,&request_id])?.try_get(0)?;
        let link:RequestConversation=serde_json::from_value(json!({"conversation":request_number,"request":typed.reservation(),"pair":pair,"sender":sender,"submission":submission,"reverse_offer_epoch":null}))?;
        let prepared=prepare(&EndpointKeys{owner:sender,secret:&sender_secret,public:sender_public},&document,conversation_id,owner_public,custodian,encrypted.binding.clone(),(encrypted.created_at,encrypted.expires_at))?;
        let record=MessageRecord{sender_key:sender_public,manifest:prepared.manifest.clone(),author:sender,admission:serde_json::from_value(message["basis"].clone())?,committed_at:CanonicalTime(number(&message["admitted_at"])? as u64)};
        let opened=open(&EndpointKeys{owner:ProtocolIdentity(owner),secret:&owner_secret,public:owner_public},&cs_mail_application::correspondence::MessageView{record:record.clone(),ciphertext:prepared.recipient_copy.clone()},sender_public)?;
        require(readable_text(&opened).as_bytes()==old_text.as_slice())?;
        tx.execute("UPDATE cs_requests SET request=$3 WHERE aggregate_key=$1 AND request_id=$2",&[&aggregate,&request_id,&Json(typed)])?;
        let cid=request_id.clone();let mid=record.manifest.message().0.to_string();
        tx.execute("INSERT INTO cs_conversation_routes(id,route) VALUES($1,'native')",&[&cid])?;
        tx.execute("INSERT INTO cs_conversations(id,record) VALUES($1,$2)",&[&cid,&Json(conversation)])?;
        tx.execute("INSERT INTO cs_request_conversations(conversation,aggregate_key,request_id,record) VALUES($1,$2,$3,$4)",&[&cid,&aggregate,&request_id,&Json(link)])?;
        tx.execute("INSERT INTO cs_correspondence_messages(id,conversation,record) VALUES($1,$2,$3)",&[&mid,&cid,&Json(record)])?;
        for copy in [&prepared.sender_copy,&prepared.recipient_copy] { tx.execute("INSERT INTO cs_correspondence_copies(message,owner,ciphertext) VALUES($1,$2,$3)",&[&mid,&copy.binding.recipient.0.to_string(),&Json(copy)])?; }
        for copy in [&prepared.sender_recovery,&prepared.recipient_recovery] { tx.execute("INSERT INTO cs_correspondence_recovery(message,owner,record) VALUES($1,$2,$3)",&[&mid,&copy.ciphertext.binding.recipient.0.to_string(),&Json(copy)])?; }
        preserved+=1;
    }
    let billing_migration = std::fs::read_to_string(
        root.parent()
            .ok_or("missing workspace")?
            .parent()
            .ok_or("missing workspace")?
            .join("crates/cs-mail-storage-postgres/migrations/0035_billing_refusals.sql"),
    )?;
    tx.batch_execute(&format!(
        "ALTER TABLE{}",
        billing_migration
            .split_once("ALTER TABLE")
            .ok_or("missing migration body")?
            .1
    ))?;
    require(preserved == 4)?;
    replies::repair(&mut tx, owner, root)?;
    if mode == "rollback" {
        tx.batch_execute("SELECT 1/0")?;
        unreachable!();
    }
    let active = &mut state["Active"];
    active["format"] = json!(3);
    active["mailbox"] = json!({"after":0,"records":[],"participants":{}});
    active["sending"] = Value::Null;
    active["inbox"] = json!([]);
    active["content"] = json!({});
    active["synchronized_at"] = Value::Null;
    active["profile"]["deployment"]["custody_key"] = serde_json::to_value(custodian)?;
    active["profile"]["deployment"]["policy"]["reply_lane"] = json!({"lifetime":604800000,"rate_limit":{"max_messages":20,"interval":86400000},"purpose":{"Known":"Personal"}});
    require(
        active["profile"]["account"] == profile["account"]
            && active["profile"]["persona"] == profile["persona"],
    )?;
    let encoded = Zeroizing::new(serde_json::to_vec(&state)?);
    let mut nonce = [0; 24];
    getrandom::fill(&mut nonce)?;
    let mut sealed = nonce.to_vec();
    sealed.extend(
        cipher
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: &encoded,
                    aad: b"cs-mail/apple-member-cache/v3",
                },
            )
            .map_err(|_| "cache encryption failed")?,
    );
    // Current client validation precedes committing either conversion artifact.
    let store = Arc::new(MemoryStore {
        secret: credential,
        cache: Mutex::new(sealed.clone()),
        cache_path: None,
    });
    let client = cs_mail_client::member::MemberClient::new(
        store,
        Arc::new(cs_mail_client::member::HttpTransport::new()?),
        Arc::new(|| CanonicalTime(0)),
    );
    client.cached_inbox()?;
    artifact::prepare(output, &profile, &sealed)?;
    if tx.commit().is_err() {
        return Err("conversion commit outcome unknown; prepared cache and intent retained. Rerun convert to reconcile against the database".into());
    }
    if simulate_ack_loss {
        return Err("simulated commit acknowledgement loss; prepared ciphertext retained for reconciliation".into());
    }
    println!(
        "Converted four sample requests with unchanged IDs, decisions, permissions and message text. Original account and keys verified. Confined replies restored from original delivery evidence; broader reverse-lane agreement remains unchanged."
    );
    Ok(())
}
