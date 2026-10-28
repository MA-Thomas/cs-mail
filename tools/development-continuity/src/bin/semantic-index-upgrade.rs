//! One explicit predecessor conversion. No predecessor reader enters the member runtime.
use chacha20poly1305::{XChaCha20Poly1305,XNonce,aead::{Aead,KeyInit,Payload}};
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::{io::{Read,Write},path::Path};
use zeroize::Zeroizing;
type Result<T> = std::result::Result<T,Box<dyn std::error::Error>>;
fn require(ok:bool)->Result<()> {if ok {Ok(())} else {Err("preservation refused".into())}}
fn bytes(value:&Value)->Result<Vec<u8>> {Ok(serde_json::from_value(value.clone())?)}
fn hex(value:&[u8])->String {Sha256::digest(value).iter().map(|b|format!("{b:02x}")).collect()}
fn decrypt(data:&[u8],key:&[u8;32],context:&[u8])->Result<Value> {
    require(data.len()>40)?;
    let plain=Zeroizing::new(XChaCha20Poly1305::new(key.into()).decrypt(&XNonce::try_from(&data[..24])?,Payload{msg:&data[24..],aad:context}).map_err(|_|"authentication refused")?);
    Ok(serde_json::from_slice(&plain)?)
}
fn encrypt(value:&Value,key:&[u8;32],context:&[u8])->Result<Vec<u8>> {
    let plain=Zeroizing::new(serde_json::to_vec(value)?);let mut nonce=[0;24];getrandom::fill(&mut nonce)?;
    let mut output=nonce.to_vec();output.extend(XChaCha20Poly1305::new(key.into()).encrypt((&nonce).into(),Payload{msg:&plain,aad:context}).map_err(|_|"encryption refused")?);Ok(output)
}
fn run()->Result<()> {
    let output=std::env::args().nth(1).ok_or("missing output")?;require(!Path::new(&output).exists())?;
    let mut input=Zeroizing::new(Vec::new());std::io::stdin().read_to_end(&mut input)?;
    let input:Value=serde_json::from_slice(&input)?;let credential=Zeroizing::new(bytes(&input["credential"])?);
    let secret:Value=serde_json::from_slice(&credential)?;let key=Zeroizing::new(<[u8;32]>::try_from(bytes(&secret["cache"])?) .map_err(|_|"key length")?);
    let cache=bytes(&input["cache"])?;let state=decrypt(&cache,&key,b"cs-mail/apple-member-cache/v5")?;
    let metadata=decrypt(&bytes(&input["metadata"] )?,&key,b"cs-mail/retained-metadata/v3")?;
    require(metadata["account"]==state["Active"]["profile"]["account"] && metadata["owner"]==state["Active"]["profile"]["persona"])?;
    let mut rows=Vec::new();let mut copies=0;
    for row in input["records"].as_array().ok_or("records")? {
        let id=bytes(&row["id"])?;require(id.len()==32)?;
        let old=bytes(&row["index"])?;
        let mut record=decrypt(&old,&key,&[b"cs-mail/retained-index/v2".as_slice(),&id].concat())?;
        if let Some(available)=record.get_mut("Available") {
            let copy=decrypt(&bytes(&row["copy"] )?,&key,&[b"cs-mail/retained-copy/v2".as_slice(),&id].concat())?;
            require(copy["key"]==available["key"] && copy["subject"]==available["subject"] && copy["expires"]==available["expires"])?;
            let text=copy["text"].as_str().ok_or("copy text")?;
            let terms=text.split(|c:char|!c.is_alphanumeric()).filter(|s|!s.is_empty()).map(str::to_lowercase).collect::<std::collections::BTreeSet<_>>();
            require(serde_json::to_value(terms)?==available["terms"])?;
            let fields=available.as_object_mut().ok_or("index")?;require(fields.remove("terms").is_some())?;
            fields.insert("content_digest".into(),json!(Sha256::digest(text.as_bytes()).to_vec()));fields.insert("projection".into(),json!("Pending"));copies+=1;
        } else {require(record.get("Excluded").is_some()&&row["copy"].is_null())?;}
        let next=encrypt(&record,&key,&[b"cs-mail/retained-index/v3".as_slice(),&id].concat())?;
        rows.push(json!({"id":id,"before_sha256":hex(&old),"index":next}));
    }
    let plan=json!({"cache_sha256":hex(&cache),"credential_sha256":hex(&credential),"expected_revision":input["revision"],"copies":copies,"records":rows});
    use std::os::unix::fs::OpenOptionsExt;
    let mut file=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(output)?;
    file.write_all(&serde_json::to_vec(&plan)?)?;file.sync_all()?;Ok(())
}
fn main(){if run().is_err(){eprintln!("Semantic-index cutover refused; no source or credential modified");std::process::exit(1)}}
