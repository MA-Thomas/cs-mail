//! SMTP2GO wire adapter. Never retries a submission, follows a redirect, or logs
//! credentials, request bodies or provider error text. Acceptance is not delivery.
use base64::{Engine, engine::general_purpose::STANDARD};
use cs_mail_adapters::DomainIdentity;
use cs_mail_correspondence::external::delivery::{
    ProviderEvent, ProviderEventId, ProviderEventKind, ProviderReference,
};
use cs_mail_guests::GuestMailbox;
use cs_mail_primitives::{MessageId, ProtocolIdentity};
use reqwest::{Url, blocking::Client, header::HeaderValue};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::io::Read;
use subtle::ConstantTimeEq;

pub enum Submission {
    Accepted(ProviderReference),
    Rejected,
    Unknown,
}
#[derive(Clone, Copy, Debug)]
pub enum Error {
    Configuration,
    Unauthorized,
    Invalid,
}
pub struct Smtp2go {
    client: Client,
    endpoint: Url,
    key: HeaderValue,
}
impl Smtp2go {
    /// # Errors
    /// Allows the official HTTPS MIME API, or explicit loopback HTTP for local experiments.
    pub fn new(endpoint: &str, key: &str) -> Result<Self, Error> {
        let endpoint = Url::parse(endpoint).map_err(|_| Error::Configuration)?;
        let official = endpoint.scheme() == "https"
            && matches!(
                endpoint.host_str(),
                Some(
                    "api.smtp2go.com"
                        | "us-api.smtp2go.com"
                        | "eu-api.smtp2go.com"
                        | "au-api.smtp2go.com"
                )
            )
            && endpoint.port().is_none();
        let local = endpoint.scheme() == "http"
            && matches!(endpoint.host_str(), Some("127.0.0.1" | "[::1]"));
        if !(official || local)
            || endpoint.path() != "/v3/email/mime"
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || key.len() < 16
            || key.len() > 256
            || !key.bytes().all(|b| b.is_ascii_graphic())
        {
            return Err(Error::Configuration);
        }
        let mut key = HeaderValue::from_str(key).map_err(|_| Error::Configuration)?;
        key.set_sensitive(true);
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .build()
            .map_err(|_| Error::Configuration)?;
        Ok(Self {
            client,
            endpoint,
            key,
        })
    }
    pub fn submit(&self, mime: &[u8]) -> Submission {
        let Ok(response) = self
            .client
            .post(self.endpoint.clone())
            .header("X-Smtp2go-Api-Key", self.key.clone())
            .json(&serde_json::json!({"mime_email": STANDARD.encode(mime), "fastaccept": true}))
            .send()
        else {
            return Submission::Unknown;
        };
        let status = response.status();
        let mut bytes = Vec::new();
        if response.take(65_537).read_to_end(&mut bytes).is_err() || bytes.len() > 65_536 {
            return Submission::Unknown;
        }
        let Ok(answer) = serde_json::from_slice::<Answer>(&bytes) else {
            return Submission::Unknown;
        };
        let data = answer.data;
        // fastaccept acknowledges queue acceptance with an email_id before
        // per-recipient processing. Missing accounting is expected in that mode.
        // If accounting is supplied, require it to be complete and consistent.
        let accounting_accepts = match (data.succeeded, data.failed, &data.failures) {
            (None, None, None) => true,
            (Some(1), Some(0), Some(failures)) => failures.is_empty(),
            _ => false,
        };
        if status.as_u16() == 200
            && data.error_code.is_none()
            && data.error.is_none()
            && accounting_accepts
        {
            if let Some(reference) = data
                .email_id
                .as_ref()
                .and_then(|id| carrier_reference(id).ok())
            {
                return Submission::Accepted(reference);
            }
        }
        // A partial, contradictory, server-error or unparseable answer is uncertain.
        if data.succeeded.unwrap_or(0) == 0
            && data.email_id.as_deref().is_none_or(str::is_empty)
            && ((status.is_success()
                && data.succeeded == Some(0)
                && data.failed == Some(1)
                && data.failures.as_ref().is_some_and(|f| f.len() == 1))
                || (matches!(status.as_u16(), 400 | 401 | 403 | 422 | 429)
                    && data.error_code.is_some()))
        {
            return Submission::Rejected;
        }
        Submission::Unknown
    }
}
#[derive(Deserialize)]
struct Answer {
    data: AnswerData,
}
#[derive(Deserialize)]
struct AnswerData {
    succeeded: Option<u32>,
    failed: Option<u32>,
    failures: Option<Vec<serde_json::Value>>,
    email_id: Option<String>,
    error_code: Option<String>,
    error: Option<String>,
}
fn carrier_reference(value: &str) -> Result<ProviderReference, Error> {
    ProviderReference::new(value.to_owned()).map_err(|_| Error::Invalid)?;
    ProviderReference::new(format!("smtp2go:{value}")).map_err(|_| Error::Invalid)
}
pub fn message_id(id: MessageId, domain: &DomainIdentity) -> String {
    format!("<cs-{:032x}@{}>", id.0, domain.as_str())
}
pub fn member_local(member: ProtocolIdentity) -> String {
    format!("member-{:032x}", member.0)
}
fn identifier(value: &str, prefix: &str, domain: &DomainIdentity) -> Result<u128, Error> {
    let (local, host) = value.rsplit_once('@').ok_or(Error::Invalid)?;
    let hex = local.strip_prefix(prefix).ok_or(Error::Invalid)?;
    if host != domain.as_str() || hex.len() != 32 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Invalid);
    }
    let id = u128::from_str_radix(hex, 16).map_err(|_| Error::Invalid)?;
    if id == 0 {
        return Err(Error::Invalid);
    }
    Ok(id)
}

/// A dedicated webhook bearer secret, independent of the sending API credential.
pub struct Webhook {
    authorization_hash: [u8; 32],
    domain: DomainIdentity,
}
pub enum Feedback {
    Conversation(ProviderEvent),
    Notice(cs_mail_correspondence::external::delivery::NoticeProviderEvent),
}
impl Webhook {
    /// # Errors
    /// Requires a nonzero random 32-byte secret, configured as a hex bearer token at SMTP2GO.
    pub fn new(secret: [u8; 32], domain: DomainIdentity) -> Result<Self, Error> {
        if secret == [0; 32] {
            return Err(Error::Configuration);
        }
        let mut token = String::new();
        for byte in secret {
            write!(token, "{byte:02x}").map_err(|_| Error::Configuration)?;
        }
        Ok(Self {
            authorization_hash: Sha256::digest(format!("Bearer {token}").as_bytes()).into(),
            domain,
        })
    }
    /// # Errors
    /// Authenticates before parsing. Unknown fields (including auth/subject/body) are discarded.
    pub fn authenticate(&self, authorization: &str, body: &[u8]) -> Result<Feedback, Error> {
        let actual: [u8; 32] = Sha256::digest(authorization.as_bytes()).into();
        if !bool::from(actual.ct_eq(&self.authorization_hash)) {
            return Err(Error::Unauthorized);
        }
        if body.len() > 65_536 {
            return Err(Error::Invalid);
        }
        let wire: WireEvent = serde_json::from_slice(body).map_err(|_| Error::Invalid)?;
        let kind = match wire.event.as_str() {
            "processed" => ProviderEventKind::Processed,
            "delivered" => ProviderEventKind::Delivered,
            "reject" => ProviderEventKind::Rejected,
            "spam" => ProviderEventKind::SpamComplaint,
            "bounce" => match wire.bounce.as_deref() {
                Some("hard") => ProviderEventKind::HardBounce,
                Some("soft") => ProviderEventKind::SoftBounce,
                _ => return Err(Error::Invalid),
            },
            _ => return Err(Error::Invalid),
        };
        let id = wire
            .message_id
            .strip_prefix('<')
            .and_then(|s| s.strip_suffix('>'))
            .ok_or(Error::Invalid)?;
        ProviderEventId::new(wire.id.clone()).map_err(|_| Error::Invalid)?;
        if wire.from_address == format!("notices@{}", self.domain.as_str()) {
            return Ok(Feedback::Notice(
                cs_mail_correspondence::external::delivery::NoticeProviderEvent {
                    id: ProviderEventId::new(format!("smtp2go:{}", wire.id))
                        .map_err(|_| Error::Invalid)?,
                    reference: carrier_reference(&wire.email_id)?,
                    notice: cs_mail_guests::NoticeId::try_from(identifier(
                        id,
                        "notice-",
                        &self.domain,
                    )?)
                    .map_err(|_| Error::Invalid)?,
                    recipient: GuestMailbox::parse(&wire.rcpt).map_err(|_| Error::Invalid)?,
                    kind,
                    provider_time: wire.time.try_into().map_err(|_| Error::Invalid)?,
                },
            ));
        }
        Ok(Feedback::Conversation(ProviderEvent {
            id: ProviderEventId::new(format!("smtp2go:{}", wire.id)).map_err(|_| Error::Invalid)?,
            reference: carrier_reference(&wire.email_id)?,
            message: MessageId(identifier(id, "cs-", &self.domain)?),
            sender: ProtocolIdentity(identifier(&wire.from_address, "member-", &self.domain)?),
            recipient: GuestMailbox::parse(&wire.rcpt).map_err(|_| Error::Invalid)?,
            kind,
            provider_time: wire.time.try_into().map_err(|_| Error::Invalid)?,
        }))
    }
}
#[derive(Deserialize)]
struct WireEvent {
    id: String,
    email_id: String,
    event: String,
    time: String,
    from_address: String,
    rcpt: String,
    #[serde(rename = "message-id")]
    message_id: String,
    bounce: Option<String>,
}
