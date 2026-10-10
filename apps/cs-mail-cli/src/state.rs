//! The member's local state: its keys, enrollment and operation journal, in one file
//! written with mode 0600. Secrets are never printed.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemberState {
    /// Service base URL: HTTPS, or HTTP on a loopback host for development.
    pub server: String,
    /// The enrolling device's Ed25519 seed; enrollment registers it as the sender key.
    pub device_secret: [u8; 32],
    /// The seed of the key this member signs recipient decisions with, once registered.
    pub recipient_secret: [u8; 32],
    /// The endpoint content key's X25519 secret and reference.
    pub content_secret: [u8; 32],
    pub content_key: u128,
    /// The enrollment operation, once begun.
    pub enrollment: Option<String>,
    /// The enrolled account, once enrollment completes.
    pub account: Option<u128>,
    /// Signed requests not yet known to have been answered, and their answers once
    /// known, by step. A retry resends exactly the recorded bytes.
    #[serde(default)]
    pub journal: BTreeMap<String, JournalEntry>,
    /// Values chosen for multi-step operations, by name.
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JournalEntry {
    pub route: String,
    pub body: Value,
    pub answer: Option<Value>,
}

pub struct Store {
    path: PathBuf,
    pub state: MemberState,
}
impl Store {
    pub fn file(home: &Path) -> PathBuf {
        home.join("member.json")
    }
    pub fn create(home: &Path, state: MemberState) -> Result<Self, String> {
        std::fs::create_dir_all(home)
            .map_err(|e| format!("cannot create {}: {e}", home.display()))?;
        let path = Self::file(home);
        if path.exists() {
            return Err(format!("{} already exists", path.display()));
        }
        let store = Self { path, state };
        store.save()?;
        Ok(store)
    }
    pub fn open(home: &Path) -> Result<Self, String> {
        let path = Self::file(home);
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("cannot read {} (run init first): {e}", path.display()))?;
        let state = serde_json::from_slice(&bytes)
            .map_err(|e| format!("invalid member state {}: {e}", path.display()))?;
        Ok(Self { path, state })
    }
    /// Writes the whole state to a temporary file and renames it into place, so a crash
    /// leaves either the old or the new state.
    pub fn save(&self) -> Result<(), String> {
        let temporary = self.path.with_extension("json.tmp");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options
            .open(&temporary)
            .map_err(|e| format!("cannot write {}: {e}", temporary.display()))?;
        std::io::Write::write_all(
            &mut file,
            &serde_json::to_vec_pretty(&self.state).map_err(|e| e.to_string())?,
        )
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("cannot write {}: {e}", temporary.display()))?;
        std::fs::rename(&temporary, &self.path)
            .map_err(|e| format!("cannot replace {}: {e}", self.path.display()))
    }
}
