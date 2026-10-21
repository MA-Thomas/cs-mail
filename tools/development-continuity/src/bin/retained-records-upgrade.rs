//! Explicit offline conversion only. Runtime code contains no predecessor reader.
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use cs_mail_application::correspondence::MessageView;
use cs_mail_client::{
    correspondence::{EndpointKeys, open},
    member::MailKey,
};
use cs_mail_content::EndpointSecretKey;
use cs_mail_correspondence::ConversationSubject;
use cs_mail_primitives::{AccountId, CanonicalTime, ContentKeyRef, MessageId, ProtocolIdentity};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, io::Read, path::Path};
use zeroize::Zeroizing;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
struct CopyKey {
    thread: MailKey,
    message: MessageId,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    key: CopyKey,
    native: Option<MessageView>,
    text: String,
    subject: Option<ConversationSubject>,
    expires: CanonicalTime,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Legacy {
    account: AccountId,
    owner: ProtocolIdentity,
    entries: Vec<Entry>,
    excluded: BTreeSet<CopyKey>,
    words: Value,
    routes: Option<Value>,
    aliases: Vec<cs_mail_accounts::aliases::Alias>,
}
fn check(value: bool) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err("retained preservation check failed".into())
    }
}
fn context(kind: &[u8], id: [u8; 32]) -> Vec<u8> {
    [kind, &id].concat()
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
            .map_err(|_| "authentication failed")?,
    ))
}
fn encrypt(value: &impl Serialize, secret: &[u8; 32], context: &[u8]) -> Result<Vec<u8>> {
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
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    check(args.len() == 2)?;
    let path = Path::new(&args[1]);
    check(!path.exists())?;
    let mut input = Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(128 * 1024 * 1024)
        .read_to_end(&mut input)?;
    let input: Value = serde_json::from_slice(&input)?;
    let credential = Zeroizing::new(serde_json::from_value::<Vec<u8>>(
        input["credential"].clone(),
    )?);
    let secret: Value = serde_json::from_slice(&credential)?;
    let cache_key = Zeroizing::new(serde_json::from_value::<[u8; 32]>(secret["cache"].clone())?);
    let content = Zeroizing::new(serde_json::from_value::<[u8; 32]>(
        secret["content"].clone(),
    )?);
    let cache: Vec<u8> = serde_json::from_value(input["cache"].clone())?;
    let state: Value = serde_json::from_slice(&decrypt(
        &cache,
        &cache_key,
        b"cs-mail/apple-member-cache/v4",
    )?)?;
    let retained: Vec<u8> = serde_json::from_value(input["retained"].clone())?;
    let legacy: Legacy = serde_json::from_slice(&decrypt(
        &retained,
        &cache_key,
        b"cs-mail/retained-mail/v1",
    )?)?;
    check(
        serde_json::to_value(legacy.account)? == state["Active"]["profile"]["account"]
            && serde_json::to_value(legacy.owner)? == state["Active"]["profile"]["persona"],
    )?;
    check(legacy.words.is_object())?;
    let reference: ContentKeyRef =
        serde_json::from_value(state["Active"]["profile"]["content_key"].clone())?;
    let (endpoint_secret, public) = EndpointSecretKey::from_secret_bytes(reference, &content)?;
    let metadata = json!({"account":legacy.account,"owner":legacy.owner,"routes":legacy.routes,"aliases":legacy.aliases});
    let mut records = Vec::new();
    let mut unique = BTreeSet::new();
    let identity = |key: CopyKey| -> Result<[u8; 32]> {
        let mut mac = Hmac::<Sha256>::new_from_slice(&*cache_key)?;
        mac.update(b"cs-mail/retained-record-id/v2");
        mac.update(&serde_json::to_vec(&(legacy.account, legacy.owner, key))?);
        Ok(mac.finalize().into_bytes().into())
    };
    for entry in &legacy.entries {
        check(unique.insert(entry.key) && !legacy.excluded.contains(&entry.key))?;
        check(entry.native.is_some() == matches!(entry.key.thread, MailKey::Native(_)))?;
        if let Some(view) = &entry.native {
            check(
                view.record.manifest.message() == entry.key.message
                    && MailKey::Native(view.record.manifest.conversation()) == entry.key.thread
                    && view.ciphertext.expires_at == entry.expires,
            )?;
            let doc = open(
                &EndpointKeys {
                    owner: legacy.owner,
                    secret: &endpoint_secret,
                    public,
                },
                view,
                view.record.sender_key,
            )?;
            check(doc.subject() == entry.subject.as_ref())?;
        }
        let terms: BTreeSet<_> = entry
            .text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase)
            .collect();
        let index = json!({"Available":{"key":entry.key,"subject":entry.subject,"expires":entry.expires,"terms":terms}});
        let id = identity(entry.key)?;
        let ciphertext = encrypt(entry, &cache_key, &context(b"cs-mail/retained-copy/v2", id))?;
        check(
            serde_json::from_slice::<Value>(&decrypt(
                &ciphertext,
                &cache_key,
                &context(b"cs-mail/retained-copy/v2", id),
            )?)? == serde_json::to_value(entry)?,
        )?;
        records.push(json!({"id":hex(&id),"index":encrypt(&index,&cache_key,&context(b"cs-mail/retained-index/v2",id))?,"copy":ciphertext}));
    }
    for key in &legacy.excluded {
        let id = identity(*key)?;
        records.push(json!({"id":hex(&id),"index":encrypt(&json!({"Excluded":key}),&cache_key,&context(b"cs-mail/retained-index/v2",id))?,"copy":null}));
    }
    let plan = json!({"source_sha256":hex(&Sha256::digest(&retained)),"cache_sha256":hex(&Sha256::digest(&cache)),"account":legacy.account.0.to_string(),"persona":legacy.owner.0.to_string(),"copies":legacy.entries.len(),"exclusions":legacy.excluded.len(),"metadata":encrypt(&metadata,&cache_key,b"cs-mail/retained-metadata/v2")?,"records":records});
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    output.write_all(&serde_json::to_vec(&plan)?)?;
    output.sync_all()?;
    println!(
        "PASS verified current account, authenticated copies, exact retained text and exclusions; encrypted record plan prepared"
    );
    Ok(())
}
fn main() {
    if run().is_err() {
        eprintln!("Retained conversion refused; source and credential untouched");
        std::process::exit(1);
    }
}
