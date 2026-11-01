//! Offline v5-to-v6 conversion. Credentials enter only through an anonymous pipe.
use cs_mail_client::member::{
    self as client, CacheCommit, DeviceStore, Error, RetainedCommit, RetainedIndexRow,
    RetainedRecordId,
};
use cs_mail_primitives::{AccountId, ProtocolIdentity};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path, sync::Mutex};
use zeroize::Zeroizing;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
struct Store {
    credential: Zeroizing<Vec<u8>>,
    head: Mutex<Option<Vec<u8>>>,
    records: Mutex<std::collections::BTreeMap<RetainedRecordId, Vec<u8>>>,
}
impl DeviceStore for Store {
    fn load_secret(&self) -> std::result::Result<Vec<u8>, Error> {
        Ok(self.credential.to_vec())
    }
    fn create_secret(&self, _: &[u8]) -> std::result::Result<(), Error> {
        Err(Error::KeyUnavailable)
    }
    fn load_cache(&self) -> std::result::Result<Option<Vec<u8>>, Error> {
        Ok(self.head.lock().unwrap().clone())
    }
    fn save_cache(&self, _: &[u8]) -> std::result::Result<(), Error> {
        Err(Error::Unavailable)
    }
    fn cache_record(&self, id: RetainedRecordId) -> std::result::Result<Option<Vec<u8>>, Error> {
        Ok(self.records.lock().unwrap().get(&id).cloned())
    }
    fn commit_cache(&self, c: CacheCommit) -> std::result::Result<(), Error> {
        let mut r = self.records.lock().unwrap();
        for w in c.writes {
            r.insert(w.id, w.ciphertext);
        }
        r.retain(|id, _| c.keep.contains(id));
        *self.head.lock().unwrap() = Some(c.head);
        Ok(())
    }
    fn retained_state(&self) -> std::result::Result<(u64, Option<Vec<u8>>), Error> {
        Ok((0, None))
    }
    fn retained_indexes(
        &self,
        _: Option<RetainedRecordId>,
        _: u16,
    ) -> std::result::Result<Vec<RetainedIndexRow>, Error> {
        Ok(vec![])
    }
    fn retained_copy(&self, _: RetainedRecordId) -> std::result::Result<Option<Vec<u8>>, Error> {
        Ok(None)
    }
    fn commit_retained(&self, _: RetainedCommit) -> std::result::Result<(), Error> {
        Err(Error::Unavailable)
    }
    fn load_workspace(&self) -> std::result::Result<Option<Vec<u8>>, Error> {
        Ok(None)
    }
    fn save_workspace(&self, _: &[u8]) -> std::result::Result<(), Error> {
        Err(Error::Unavailable)
    }
    fn load_personal(&self) -> std::result::Result<Option<Vec<u8>>, Error> {
        Ok(None)
    }
    fn save_personal(&self, _: &[u8]) -> std::result::Result<(), Error> {
        Err(Error::Unavailable)
    }
    fn save_submission(
        &self,
        _: CacheCommit,
        _: &[u8],
        _: &[u8],
    ) -> std::result::Result<(), Error> {
        Err(Error::Unavailable)
    }
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4
        || (args[1] != "--verify"
            && (!Path::new(&args[1]).is_absolute() || Path::new(&args[1]).exists()))
    {
        return Err("explicit new output path, account and persona required".into());
    }
    let mut bytes = Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(256 * 1024 * 1024)
        .read_to_end(&mut bytes)?;
    let mut input: Value = serde_json::from_slice(&bytes)?;
    let credential = Zeroizing::new(serde_json::from_value::<Vec<u8>>(
        input["credential"].take(),
    )?);
    let cache: Vec<u8> = serde_json::from_value(input["cache"].take())?;
    let digest = Sha256::digest(&cache).to_vec();
    let store = Store {
        credential,
        head: Mutex::new(Some(cache)),
        records: Mutex::new(Default::default()),
    };
    if args[1] == "--verify" {
        if let Some(rows) = input["records"].as_array() {
            for row in rows {
                let id: Vec<u8> = serde_json::from_value(row["id"].clone())?;
                let bytes: Vec<u8> = serde_json::from_value(row["ciphertext"].clone())?;
                store.records.lock().unwrap().insert(id.try_into()?, bytes);
            }
        }
        let runtime = client::MemberClient::new(
            std::sync::Arc::new(store),
            std::sync::Arc::new(client::HttpTransport::new()?),
            std::sync::Arc::new(|| cs_mail_primitives::CanonicalTime(0)),
        );
        let reopened = runtime.cached_inbox()?;
        if reopened.account != args[2] || reopened.persona != args[3] {
            return Err("restored account association changed".into());
        }
        println!(
            "PASS supported reader reopened the original account and authenticated every mailbox record"
        );
        return Ok(());
    }
    let change = client::prepare_mailbox_upgrade(
        &store,
        AccountId(args[2].parse()?),
        ProtocolIdentity(args[3].parse()?),
    )?;
    let plan = json!({"source_digest":digest,"account":args[2],"persona":args[3],"head":change.head,"keep":change.keep.iter().map(|id|id.bytes().to_vec()).collect::<Vec<_>>(),"writes":change.writes.iter().map(|w|json!({"id":w.id.bytes().to_vec(),"ciphertext":w.ciphertext})).collect::<Vec<_>>()});
    store.commit_cache(change)?;
    // The supported reader validates every detached record and its account/message bindings.
    let runtime = client::MemberClient::new(
        std::sync::Arc::new(store),
        std::sync::Arc::new(client::HttpTransport::new()?),
        std::sync::Arc::new(|| cs_mail_primitives::CanonicalTime(0)),
    );
    let reopened = runtime.cached_inbox()?;
    if reopened.account != args[2] || reopened.persona != args[3] {
        return Err("restored account association changed".into());
    }
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&args[1])?;
    file.write_all(&serde_json::to_vec(&plan)?)?;
    file.sync_all()?;
    println!(
        "PASS original account, persona and every mailbox record preserved; ciphertext-only conversion plan prepared"
    );
    Ok(())
}
