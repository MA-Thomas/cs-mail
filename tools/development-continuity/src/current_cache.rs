//! Explicit v2 -> v3 preservation upgrade. The runtime has one cache reader, v3.
use super::*;
/// Read only opaque ciphertext from the explicitly selected current SQLite store.
pub(super) fn read_current(path: &Path) -> Result<Vec<u8>> {
    require(path.is_absolute())?;
    let output = std::process::Command::new("/usr/bin/sqlite3")
        .args([
            "-readonly",
            path.to_str().ok_or("invalid cache path")?,
            "SELECT hex(ciphertext) FROM device_cache WHERE id=1;",
        ])
        .output()?;
    require(output.status.success() && output.stdout.len() <= 128 * 1024 * 1024)?;
    let hex = std::str::from_utf8(&output.stdout)?.trim();
    require(hex.len().is_multiple_of(2))?;
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).map_err(Into::into))
        .collect()
}
pub(super) fn upgrade(
    input: &[u8],
    secrets: &Value,
    expected: &Value,
    tx: &mut postgres::Transaction<'_>,
) -> Result<Vec<u8>> {
    require(input.len() > 40)?;
    let cache_key = Zeroizing::new(key(&secrets["cache"])?);
    let cipher = XChaCha20Poly1305::new((&*cache_key).into());
    let plain = Zeroizing::new(
        cipher
            .decrypt(
                &XNonce::try_from(&input[..24]).map_err(|_| "invalid current cache")?,
                Payload {
                    msg: &input[24..],
                    aad: b"cs-mail/apple-member-cache/v2",
                },
            )
            .map_err(|_| "current cache authentication failed")?,
    );
    let mut state: Value = serde_json::from_slice(&plain)?;
    let active = &mut state["Active"];
    require(
        active["format"] == 2
            && active["profile"]["account"] == expected["account"]
            && active["profile"]["persona"] == expected["persona"],
    )?;
    let work_before = serde_json::to_vec(
        &json!({"pending":active["pending"],"sending":active["sending"],"requesting":active["requesting"],"setup":active["setup"],"editing_conversation":active["editing_conversation"]}),
    )?;
    for view in active["inbox"].as_array_mut().ok_or("invalid inbox")? {
        let row=tx.query_one("SELECT relationship_state#>'{relationship,reply_epoch}' FROM cs_relationship_aggregates WHERE relationship_state#>>'{relationship,key,sender}'=$1 AND relationship_state#>>'{relationship,key,recipient}'=$2", &[&number(&view["handle"]["sender"])?.to_string(),&number(&view["handle"]["recipient"])?.to_string()])?;
        view["reply_epoch"] = row.get::<_, Json<Value>>(0).0;
        view.as_object_mut()
            .ok_or("invalid view")?
            .remove("answers");
    }
    active["format"] = json!(3);
    active["synchronized_at"] = Value::Null;
    let work_after = serde_json::to_vec(
        &json!({"pending":active["pending"],"sending":active["sending"],"requesting":active["requesting"],"setup":active["setup"],"editing_conversation":active["editing_conversation"]}),
    )?;
    require(work_before == work_after)?;
    let encoded = Zeroizing::new(serde_json::to_vec(&state)?);
    let mut nonce = [0; 24];
    getrandom::fill(&mut nonce)?;
    let mut output = nonce.to_vec();
    output.extend(
        cipher
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: &encoded,
                    aad: b"cs-mail/apple-member-cache/v3",
                },
            )
            .map_err(|_| "upgraded cache encryption failed")?,
    );
    Ok(output)
}
