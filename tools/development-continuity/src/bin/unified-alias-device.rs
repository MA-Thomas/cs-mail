//! Explicit offline format conversion. This is never a runtime predecessor reader.
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use serde_json::{Value, json};
use std::{io::Read, path::Path};
use zeroize::Zeroizing;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[track_caller]
fn check(ok: bool) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(format!(
            "preservation condition refused at line {}",
            std::panic::Location::caller().line()
        )
        .into())
    }
}
fn decrypt(bytes: &[u8], key: &[u8; 32], context: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    check(bytes.len() > 40)?;
    Ok(Zeroizing::new(
        XChaCha20Poly1305::new(key.into())
            .decrypt(
                &XNonce::try_from(&bytes[..24])?,
                Payload {
                    msg: &bytes[24..],
                    aad: context,
                },
            )
            .map_err(|_| "ciphertext authentication refused")?,
    ))
}
fn encrypt(value: &Value, key: &[u8; 32], context: &[u8]) -> Result<Vec<u8>> {
    let plain = Zeroizing::new(serde_json::to_vec(value)?);
    let mut nonce = [0; 24];
    getrandom::fill(&mut nonce)?;
    let mut bytes = nonce.to_vec();
    bytes.extend(
        XChaCha20Poly1305::new(key.into())
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: &plain,
                    aad: context,
                },
            )
            .map_err(|_| "encryption refused")?,
    );
    Ok(bytes)
}
fn number(value: &Value) -> String {
    value.to_string().trim_matches('"').to_owned()
}
fn transform(value: &mut Value, plan: &Value) -> Result<()> {
    match value {
        Value::Array(values) => {
            for item in values {
                transform(item, plan)?;
            }
        }
        Value::Object(fields) => {
            if fields.contains_key("usage")
                && fields.contains_key("permission")
                && fields.contains_key("owner")
            {
                let id = number(&fields["id"]);
                let mapping = &plan["aliases"][&id];
                check(
                    mapping.is_object()
                        && fields["owner"] == mapping["owner"]
                        && fields["address"] == mapping["address"]
                        && fields["usage"] == mapping["usage"],
                )?;
                fields.remove("usage");
                fields.insert("configuration".into(), mapping["configuration"].clone());
                let _: cs_mail_accounts::aliases::Alias =
                    serde_json::from_value(Value::Object(fields.clone()))?;
            }
            if fields.contains_key("selected_class")
                && fields.contains_key("quote_id")
                && fields.contains_key("recipient")
            {
                check(!fields.contains_key("receiving_alias"))?;
                let receiver = &plan["receiving"][number(&fields["recipient"])];
                let sender = &plan["receiving"][number(&fields["sender"])];
                check(receiver.is_number() && sender.is_number())?;
                fields.insert("receiving_alias".into(), receiver.clone());
                fields.insert("return_route".into(), json!({"Native":sender}));
            }
            if fields.contains_key("signature")
                && fields
                    .get("terms")
                    .is_some_and(|v| v.get("selected_class").is_some())
            {
                return Err("pending signed quote requires a separate explicit decision".into());
            }
            if let Some(target) = fields.get_mut("Request").and_then(Value::as_object_mut) {
                if target.contains_key("other")
                    && target.contains_key("class")
                    && !target.contains_key("alias")
                {
                    let alias = &plan["receiving"][number(&target["other"])];
                    check(alias.is_number())?;
                    target.insert("alias".into(), alias.clone());
                }
            }
            for field in fields.values_mut() {
                transform(field, plan)?;
            }
        }
        _ => (),
    }
    Ok(())
}
/// Read-only verifier for the proposed ciphertext; never installed as an app adapter.
struct ProposedStore {
    data: Value,
    secret: Zeroizing<Vec<u8>>,
    personal: Option<Vec<u8>>,
}
impl cs_mail_client::member::DeviceStore for ProposedStore {
    fn load_secret(&self) -> std::result::Result<Vec<u8>, cs_mail_client::member::Error> {
        Ok(self.secret.to_vec())
    }
    fn create_secret(&self, _: &[u8]) -> std::result::Result<(), cs_mail_client::member::Error> {
        Err(cs_mail_client::member::Error::Unavailable)
    }
    fn load_cache(&self) -> std::result::Result<Option<Vec<u8>>, cs_mail_client::member::Error> {
        Ok(serde_json::from_value(self.data["cache"].clone()).ok())
    }
    fn save_cache(&self, _: &[u8]) -> std::result::Result<(), cs_mail_client::member::Error> {
        Err(cs_mail_client::member::Error::Unavailable)
    }
    fn load_workspace(
        &self,
    ) -> std::result::Result<Option<Vec<u8>>, cs_mail_client::member::Error> {
        Ok(serde_json::from_value(self.data["workspace"].clone()).ok())
    }
    fn save_workspace(&self, _: &[u8]) -> std::result::Result<(), cs_mail_client::member::Error> {
        Err(cs_mail_client::member::Error::Unavailable)
    }
    fn load_personal(&self) -> std::result::Result<Option<Vec<u8>>, cs_mail_client::member::Error> {
        Ok(self.personal.clone())
    }
    fn save_personal(&self, _: &[u8]) -> std::result::Result<(), cs_mail_client::member::Error> {
        Err(cs_mail_client::member::Error::Unavailable)
    }
    fn save_submission(
        &self,
        _: &[u8],
        _: &[u8],
        _: &[u8],
    ) -> std::result::Result<(), cs_mail_client::member::Error> {
        Err(cs_mail_client::member::Error::Unavailable)
    }
    fn retained_state(
        &self,
    ) -> std::result::Result<(u64, Option<Vec<u8>>), cs_mail_client::member::Error> {
        Ok((
            self.data["revision"].as_u64().unwrap(),
            serde_json::from_value(self.data["metadata"].clone()).ok(),
        ))
    }
    fn retained_indexes(
        &self,
        _: Option<cs_mail_client::member::RetainedRecordId>,
        _: u16,
    ) -> std::result::Result<
        Vec<cs_mail_client::member::RetainedIndexRow>,
        cs_mail_client::member::Error,
    > {
        Ok(vec![])
    }
    fn retained_copy(
        &self,
        _: cs_mail_client::member::RetainedRecordId,
    ) -> std::result::Result<Option<Vec<u8>>, cs_mail_client::member::Error> {
        Err(cs_mail_client::member::Error::Unavailable)
    }
    fn commit_retained(
        &self,
        _: cs_mail_client::member::RetainedCommit,
    ) -> std::result::Result<(), cs_mail_client::member::Error> {
        Err(cs_mail_client::member::Error::Unavailable)
    }
}
struct Offline;
impl cs_mail_client::member::Transport for Offline {
    fn request(
        &self,
        _: &str,
        _: &str,
        _: Option<&[u8]>,
    ) -> std::result::Result<Vec<u8>, cs_mail_client::member::Error> {
        Err(cs_mail_client::member::Error::Offline)
    }
    fn identity_request(
        &self,
        _: &str,
        _: Option<&[(&str, &str)]>,
    ) -> std::result::Result<Vec<u8>, cs_mail_client::member::Error> {
        Err(cs_mail_client::member::Error::Offline)
    }
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() == 3 && args[1] == "validate-backend" {
        let plan: Value = serde_json::from_slice(&std::fs::read(&args[2])?)?;
        for value in plan["new_aliases"].as_array().ok_or("missing aliases")? {
            let _: cs_mail_accounts::aliases::Alias = serde_json::from_value(value.clone())?;
        }
        for value in plan["new_classes"].as_array().ok_or("missing classes")? {
            let _: cs_mail_protocol::pricing::RecipientRequestClasses =
                serde_json::from_value(value.clone())?;
        }
        for value in plan["new_entries"].as_array().ok_or("missing entries")? {
            let _: cs_mail_accounts::directory::DirectoryEntry =
                serde_json::from_value(value.clone())?;
        }
        for value in plan["new_requests"].as_array().ok_or("missing requests")? {
            let _: cs_mail_protocol::RelationshipRequest = serde_json::from_value(value.clone())?;
        }
        for value in plan["scopes"].as_array().ok_or("missing scopes")? {
            let _: cs_mail_application::aliases::SenderRouteScope =
                serde_json::from_value(value.clone())?;
        }
        println!(
            "PASS current checked alias, publication, request and receiving-scope restoration"
        );
        return Ok(());
    }
    check(args.len() == 4 && args[1] == "device")?;
    let plan: Value = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    check(!Path::new(&args[3]).exists())?;
    let mut bytes = Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(64 * 1024 * 1024)
        .read_to_end(&mut bytes)?;
    let input: Value = serde_json::from_slice(&bytes)?;
    let credential = Zeroizing::new(serde_json::from_value::<Vec<u8>>(
        input["credential"].clone(),
    )?);
    let secrets: Value = serde_json::from_slice(&credential)?;
    let key = Zeroizing::new(serde_json::from_value::<[u8; 32]>(
        secrets["cache"].clone(),
    )?);
    let mut output = json!({"revision":input["revision"],"account":null});
    for (name, old, new) in [
        (
            "cache",
            "cs-mail/apple-member-cache/v4",
            "cs-mail/apple-member-cache/v5",
        ),
        (
            "metadata",
            "cs-mail/retained-metadata/v2",
            "cs-mail/retained-metadata/v3",
        ),
        (
            "workspace",
            "cs-mail/member-workspace/v2",
            "cs-mail/member-workspace/v3",
        ),
    ] {
        if input[name].is_null() {
            output[name] = Value::Null;
            continue;
        }
        let cipher: Vec<u8> = serde_json::from_value(input[name].clone())?;
        let plain = decrypt(&cipher, &key, old.as_bytes())?;
        let original: Value = serde_json::from_slice(&plain)?;
        let mut updated = original.clone();
        if name == "cache" {
            let active = updated
                .get_mut("Active")
                .ok_or("not an active original account")?;
            check(
                active["format"] == 4
                    && number(&active["profile"]["account"])
                        == plan["account"].as_str().ok_or("missing account")?,
            )?;
            for field in [
                "setup",
                "pending",
                "sending",
                "requesting",
                "editing_conversation",
            ] {
                check(active[field].is_null())?;
            }
            check(active["external"]["pending"].is_null())?;
            output["account"] = active["profile"]["account"].clone();
            active["format"] = json!(5);
        }
        transform(&mut updated, &plan)?;
        let cipher = encrypt(&updated, &key, new.as_bytes())?;
        check(
            serde_json::from_slice::<Value>(&decrypt(&cipher, &key, new.as_bytes())?)? == updated,
        )?;
        output[name] = serde_json::to_value(cipher)?;
    }
    let store = ProposedStore {
        data: output.clone(),
        secret: Zeroizing::new(credential.to_vec()),
        personal: serde_json::from_value(input["personal"].clone()).ok(),
    };
    let client = cs_mail_client::member::MemberClient::new(
        std::sync::Arc::new(store),
        std::sync::Arc::new(Offline),
        std::sync::Arc::new(|| cs_mail_primitives::CanonicalTime(1792600000000)),
    );
    check(client.cached_inbox()?.account == plan["account"].as_str().ok_or("missing account")?)?;
    let _ = client.drafts()?;
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[3])?;
    file.write_all(&serde_json::to_vec(&output)?)?;
    file.sync_all()?;
    println!(
        "PASS authenticated original-account cache, alias metadata and writing conversion; credential never written"
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Unified alias device upgrade refused: {error}");
        std::process::exit(1);
    }
}
