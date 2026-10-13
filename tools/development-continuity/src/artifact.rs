//! Durable encrypted conversion intent. Database acknowledgement loss never discards evidence.
use super::*;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
pub(super) fn durable(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_extension("prepared.tmp");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)?;
    std::fs::File::open(path.parent().ok_or("missing artifact parent")?)?.sync_all()?;
    Ok(())
}
pub(super) fn prepare(output: &Path, profile: &Value, cache: &[u8]) -> Result<()> {
    require(!output.exists())?;
    let intent = output.with_extension("continuity.json");
    require(!intent.exists())?;
    let metadata = json!({"account":profile["account"],"persona":profile["persona"],"cache_digest":<[u8;32]>::from(Sha256::digest(cache)),"cache":cache});
    durable(&intent, &serde_json::to_vec(&metadata)?)?;
    durable(output, cache)
}
pub(super) fn recover(output: &Path, profile: &Value, committed: bool) -> Result<()> {
    let intent = output.with_extension("continuity.json");
    let metadata: Value = serde_json::from_slice(&std::fs::read(&intent)?)?;
    let cache: Vec<u8> = serde_json::from_value(metadata["cache"].clone())?;
    require(
        metadata["account"] == profile["account"]
            && metadata["persona"] == profile["persona"]
            && metadata["cache_digest"]
                == serde_json::to_value(<[u8; 32]>::from(Sha256::digest(&cache)))?,
    )?;
    if committed {
        if output.exists() {
            require(std::fs::read(output)? == cache)?;
        } else {
            durable(output, &cache)?;
        }
    } else {
        // Only a protected read proving schema 32 authorizes treating this attempt as rolled back.
        if output.exists() {
            std::fs::rename(output, output.with_extension("rolled-back-cache"))?;
        }
        std::fs::rename(intent, output.with_extension("rolled-back-intent"))?;
        std::fs::File::open(output.parent().ok_or("missing artifact parent")?)?.sync_all()?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lost_commit_acknowledgement_retains_a_recoverable_encrypted_artifact() {
        // Conjecture: interrupted artifact publication cannot discard the cache for a committed DB.
        // A missing/unrecoverable cache after reconciliation would strand the preserved account.
        let directory =
            std::env::temp_dir().join(format!("continuity-artifact-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("cache.bin");
        let profile = json!({"account":17,"persona":2});
        let ciphertext = [0xa5; 64];
        prepare(&path, &profile, &ciphertext).unwrap();
        std::fs::remove_file(&path).unwrap(); // interrupt before the final cache artifact is available
        recover(&path, &profile, true).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), ciphertext);
        recover(&path, &profile, true).unwrap();
        assert!(recover(&path, &json!({"account":18,"persona":2}), true).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
