//! Explicit picture retrieval, separate from message reading, sync and delivery.
use super::{
    Error, MailKey, MemberClient, PictureStatus, ReadingContent, ReadingPart, State,
    personal::Personal,
    storage::{self, Secrets},
};
use cs_mail_correspondence::{Document, MessageVersion, email::EmailResource};
use cs_mail_primitives::{CanonicalTime, MessageId};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    net::{IpAddr, ToSocketAddrs},
};

const MAX_PICTURE: usize = 4 * 1024 * 1024;
const MAX_CACHE: usize = 16 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
pub(super) struct ResourceRecord {
    key: MailKey,
    message: MessageId,
    version: MessageVersion,
    id: String,
    url: String,
    expires: CanonicalTime,
    attempted_at: CanonicalTime,
    attempt: [u8; 16],
    outcome: Retrieval,
}
#[derive(Serialize, Deserialize)]
enum Retrieval {
    Attempted,
    Retained {
        mime_type: String,
        bytes: Vec<u8>,
        retrieved_at: CanonicalTime,
    },
}
pub(super) fn validate_records(records: &[ResourceRecord]) -> Result<(), Error> {
    let size: usize = records
        .iter()
        .map(|r| match &r.outcome {
            Retrieval::Attempted => 0,
            Retrieval::Retained { bytes, .. } => bytes.len(),
        })
        .sum();
    let unique: std::collections::BTreeSet<_> = records
        .iter()
        .map(|r| (r.key, r.message, r.id.as_str()))
        .collect();
    if unique.len() != records.len() || records.len() > 256 || size > MAX_CACHE || records.iter().any(|r| {
        cs_mail_correspondence::email::web_destination(&r.url).as_deref() != Some(r.url.as_str()) || r.message.0 == 0 || r.attempt == [0; 16] || r.attempted_at >= r.expires || r.id != cs_mail_correspondence::email::resource_id(r.url.as_bytes()) ||
        matches!(&r.outcome, Retrieval::Retained { bytes, mime_type, retrieved_at } if *retrieved_at < r.attempted_at || *retrieved_at >= r.expires || bytes.is_empty() || bytes.len() > MAX_PICTURE || !matches!(mime_type.as_str(), "image/png" | "image/jpeg" | "image/webp"))
    }) { return Err(Error::InvalidData); }
    Ok(())
}
impl MemberClient {
    pub(super) fn forget_resources(
        &self,
        state: &State,
        keys: &Secrets,
        key: MailKey,
    ) -> Result<(), Error> {
        let _guard = self.workspace.lock().map_err(|_| Error::Unavailable)?;
        let mut personal = Personal::load(self, keys, state)?;
        let before = personal.resources.len();
        personal.resources.retain(|r| r.key != key);
        if personal.resources.len() != before {
            storage::check_keys(self.store.as_ref(), keys)?;
            self.store.save_personal(&personal.bytes(keys)?)?;
        }
        Ok(())
    }
    fn resource_document(
        &self,
        state: &State,
        keys: &Secrets,
        key: MailKey,
        message: MessageId,
    ) -> Result<(Document, MessageVersion, CanonicalTime), Error> {
        if let MailKey::Native(id) = key {
            let record = state
                .mailbox
                .records
                .iter()
                .map(|(_, r)| r)
                .find(|r| r.manifest.message() == message && r.manifest.conversation() == id)
                .ok_or(Error::SourceUnavailable)?;
            let doc = self
                .retained_document(state, keys, record)?
                .ok_or(Error::SourceUnavailable)?;
            let retained = super::retained::Retained::load(self, keys, state)?;
            let entry = retained
                .entry(
                    super::retained::CopyKey {
                        thread: key,
                        message,
                    },
                    (self.clock)(),
                )
                .ok_or(Error::SourceUnavailable)?;
            return Ok((doc, record.manifest.version(), entry.expires));
        }
        let MailKey::External(id) = key else {
            unreachable!()
        };
        let record = state
            .external
            .as_ref()
            .and_then(|w| w.records.get(&id.value()))
            .ok_or(Error::SourceUnavailable)?;
        let entry = record
            .messages
            .iter()
            .find(|e| e.message.manifest.message() == message)
            .ok_or(Error::SourceUnavailable)?;
        if entry.message.member_copy.expires_at <= (self.clock)() {
            return Err(Error::SourceUnavailable);
        }
        entry
            .message
            .validate(&record.conversation, &record.relationship)
            .map_err(|_| Error::InvalidData)?;
        let (secret, _) = super::correspondence::own_keys(&state.profile, keys)?;
        let bytes = zeroize::Zeroizing::new(
            cs_mail_content::decrypt(
                &secret,
                entry.message.sender_key,
                &entry.message.member_copy,
            )
            .map_err(|_| Error::InvalidData)?,
        );
        let doc: Document = serde_json::from_slice(&bytes).map_err(|_| Error::InvalidData)?;
        if doc.manifest(message, id).map_err(|_| Error::InvalidData)? != entry.message.manifest {
            return Err(Error::InvalidData);
        }
        Ok((
            doc,
            entry.message.manifest.version(),
            entry.message.member_copy.expires_at,
        ))
    }
    pub(super) fn apply_resources(
        personal: &Personal,
        content: &mut ReadingContent,
        key: MailKey,
        message: MessageId,
        version: MessageVersion,
        now: CanonicalTime,
    ) {
        let ReadingContent::Readable(parts) = content else {
            return;
        };
        for part in parts {
            if let ReadingPart::Email(email) = part {
                for picture in &mut email.pictures {
                    if let Some(record) = personal.resources.iter().find(|r| {
                        r.key == key
                            && r.message == message
                            && r.version == version
                            && r.id == picture.id
                            && r.url == picture.url
                            && now < r.expires
                    }) {
                        picture.status = match &record.outcome {
                            Retrieval::Attempted => PictureStatus::Attempted,
                            Retrieval::Retained {
                                mime_type, bytes, ..
                            } => {
                                email.resources.push(EmailResource {
                                    id: record.id.clone(),
                                    mime_type: mime_type.clone(),
                                    bytes: bytes.clone(),
                                });
                                PictureStatus::Loaded
                            }
                        };
                    }
                }
            }
        }
    }
    /// The caller invokes this only after explicit approval of these exact picture IDs.
    /// Never called by reading, synchronization, search or automatic retries.
    /// # Errors
    /// Stale message, unrelated resource, failed durable intent, or unavailable retrieval.
    pub fn load_email_pictures(
        &self,
        key: MailKey,
        message: MessageId,
        ids: &[String],
    ) -> Result<(), Error> {
        self.load_pictures_with(key, message, ids, fetch_picture)
    }
    pub(super) fn load_pictures_with(
        &self,
        key: MailKey,
        message: MessageId,
        ids: &[String],
        fetch: impl Fn(
            &cs_mail_correspondence::email::ApprovedPicture,
        ) -> Result<(String, Vec<u8>), Error>,
    ) -> Result<(), Error> {
        let approved = {
            let _guard = self.workspace.lock().map_err(|_| Error::Unavailable)?;
            let keys = storage::secrets(self.store.as_ref())?;
            let state = storage::open(self.store.as_ref(), &keys)?;
            let (doc, version, expires) = self.resource_document(&state, &keys, key, message)?;
            let email = picture_source(&doc)?;
            let approved = email
                .approve_pictures(ids)
                .map_err(|_| Error::InvalidData)?;
            let mut personal = Personal::load(self, &keys, &state)?;
            personal.resources.retain(|r| (self.clock)() < r.expires);
            let mut pending = Vec::new();
            for picture in approved {
                if personal.resources.iter().any(|r| {
                    r.key == key
                        && r.message == message
                        && r.version == version
                        && r.id == picture.id()
                        && matches!(r.outcome, Retrieval::Retained { .. })
                }) {
                    continue;
                }
                personal
                    .resources
                    .retain(|r| !(r.key == key && r.message == message && r.id == picture.id()));
                let mut attempt = [0; 16];
                getrandom::fill(&mut attempt).map_err(|_| Error::Unavailable)?;
                personal.resources.push(ResourceRecord {
                    key,
                    message,
                    version,
                    id: picture.id().to_owned(),
                    url: picture.url().to_owned(),
                    expires,
                    attempted_at: (self.clock)(),
                    attempt,
                    outcome: Retrieval::Attempted,
                });
                pending.push((picture, attempt));
            }
            validate_records(&personal.resources)?;
            storage::check_keys(self.store.as_ref(), &keys)?;
            self.store.save_personal(&personal.bytes(&keys)?)?;
            (pending, version)
        };
        let mut failed = false;
        for (picture, attempt) in approved.0 {
            // Every attempt is already durable. No lock is held during external I/O.
            let result = fetch(&picture);
            let _guard = self.workspace.lock().map_err(|_| Error::Unavailable)?;
            let keys = storage::secrets(self.store.as_ref())?;
            let state = storage::open(self.store.as_ref(), &keys)?;
            let (_, current, expires) = self.resource_document(&state, &keys, key, message)?;
            if current != approved.1 || (self.clock)() >= expires {
                return Err(Error::SourceUnavailable);
            }
            let mut personal = Personal::load(self, &keys, &state)?;
            let record = personal
                .resources
                .iter_mut()
                .find(|r| {
                    r.key == key
                        && r.message == message
                        && r.id == picture.id()
                        && r.version == current
                })
                .ok_or(Error::InvalidData)?;
            if record.attempt != attempt {
                continue;
            } // A newer explicit action owns its result.
            match result {
                Ok((mime_type, bytes)) => {
                    record.outcome = Retrieval::Retained {
                        mime_type,
                        bytes,
                        retrieved_at: (self.clock)(),
                    }
                }
                Err(_) => {
                    failed = true;
                }
            }
            validate_records(&personal.resources)?;
            storage::check_keys(self.store.as_ref(), &keys)?;
            self.store.save_personal(&personal.bytes(&keys)?)?;
        }
        if failed {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    }
}
/// A fresh anonymous connection, no credentials, cookies, proxy, redirect or retry.
fn fetch_picture(
    picture: &cs_mail_correspondence::email::ApprovedPicture,
) -> Result<(String, Vec<u8>), Error> {
    let url = reqwest::Url::parse(picture.url()).map_err(|_| Error::InvalidData)?;
    if url.scheme() != "https"
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Error::InvalidData);
    }
    let host = url.host_str().ok_or(Error::InvalidData)?;
    let addresses: Vec<_> = (host, 443)
        .to_socket_addrs()
        .map_err(|_| Error::Offline)?
        .collect();
    if addresses.is_empty() || addresses.iter().any(|a| !public_address(a.ip())) {
        return Err(Error::InvalidData);
    }
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(std::time::Duration::from_secs(15))
        .resolve_to_addrs(host, &addresses)
        .build()
        .map_err(|_| Error::Unavailable)?;
    let response = client
        .get(url)
        .header("Accept", "image/png,image/jpeg,image/webp")
        .send()
        .map_err(|_| Error::Offline)?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|n| n > MAX_PICTURE as u64)
    {
        return Err(Error::InvalidData);
    }
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(';').next())
        .ok_or(Error::InvalidData)?
        .to_ascii_lowercase();
    if !matches!(mime.as_str(), "image/png" | "image/jpeg" | "image/webp") {
        return Err(Error::InvalidData);
    }
    let mut bytes = Vec::new();
    response
        .take((MAX_PICTURE + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Offline)?;
    if bytes.is_empty() || bytes.len() > MAX_PICTURE {
        return Err(Error::InvalidData);
    }
    Ok((mime, bytes))
}
fn public_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ip.is_documentation()
                || a == 0
                || a >= 240
                || (a == 100 && (64..=127).contains(&b))
                || (a == 198 && (b == 18 || b == 19))
                || (a == 192 && b == 0 && c == 0))
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return public_address(IpAddr::V4(v4));
            }
            let s = ip.segments();
            s[0] & 0xe000 == 0x2000 && !(s[0] == 0x2001 && (s[1] == 0xdb8 || s[1] < 0x200))
        }
    }
}

// Authored HTML and preserved MIME use the same exact-resource approval owner.
fn picture_source(doc: &Document) -> Result<cs_mail_correspondence::email::EmailContent, Error> {
    Ok(
        if let [cs_mail_correspondence::DocumentPart::Email(email)] = doc.parts() {
            email.clone()
        } else {
            let mut bodies = doc.parts().iter().filter_map(|part| match part {
                cs_mail_correspondence::DocumentPart::Composed(body) => Some(body),
                _ => None,
            });
            let body = bodies.next().ok_or(Error::SourceUnavailable)?;
            if bodies.next().is_some() {
                return Err(Error::InvalidData);
            }
            cs_mail_correspondence::email::EmailContent::from_mime(
                body.mime(false).map_err(|_| Error::InvalidData)?,
            )
            .map_err(|_| Error::InvalidData)?
        },
    )
}
