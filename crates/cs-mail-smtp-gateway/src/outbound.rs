//! Outbound notices from the deployment domain: plain-text messages, DKIM-signed with the
//! deployment domain's own RSA-2048 key so reputation accrues to that domain whatever
//! delivers them.
use base64::{Engine, engine::general_purpose::STANDARD};
use core::fmt;
use mail_auth::common::crypto::{RsaKey, Sha256};
use mail_auth::common::headers::HeaderWriter;
use mail_auth::dkim::{DkimSigner, Done};
use rustls_pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer, pem::PemObject};

/// The deployment domain's signing identity: `selector._domainkey.domain`.
pub struct DkimIdentity {
    domain: cs_mail_adapters::DomainIdentity,
    signer: DkimSigner<RsaKey<Sha256>, Done>,
}

/// Configured public request-page origin; no caller-controlled redirect target.
pub struct RequestSite(reqwest::Url);
impl RequestSite {
    /// # Errors
    /// Requires an HTTPS origin, or loopback HTTP for explicit local demonstrations.
    pub fn new(value: &str) -> Result<Self, OutboundError> {
        let url = reqwest::Url::parse(value).map_err(|_| OutboundError::InvalidHeader)?;
        let local = url.scheme() == "http"
            && matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"));
        if (url.scheme() != "https" && !local)
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(OutboundError::InvalidHeader);
        }
        Ok(Self(url))
    }
    pub fn member(&self, member: cs_mail_primitives::ProtocolIdentity) -> String {
        format!("{}r/{}", self.0, member.0)
    }
}
impl DkimIdentity {
    /// Load the deployment's RSA-2048 key from unencrypted PKCS#8 PEM.
    /// # Errors
    /// Rejects invalid domains/selectors, malformed keys and other key sizes before
    /// a delivery worker can begin dispatch. Operator/identity keys are separate.
    pub fn from_pkcs8_pem(domain: &str, selector: &str, pem: &[u8]) -> Result<Self, OutboundError> {
        let domain = cs_mail_adapters::DomainIdentity::parse_ascii(domain)
            .map_err(|_| OutboundError::InvalidHeader)?;
        if selector.len() > 253
            || selector.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
        {
            return Err(OutboundError::InvalidHeader);
        }
        let der = PrivatePkcs8KeyDer::from_pem_slice(pem).map_err(|_| OutboundError::Key)?;
        // ring validates the private key and enforces a minimum 2048-bit modulus.
        // mail-auth does not expose the modulus length of its signing key.
        let checked = ring::signature::RsaKeyPair::from_pkcs8(der.secret_pkcs8_der())
            .map_err(|_| OutboundError::Key)?;
        if checked.public().modulus_len() != 256 {
            return Err(OutboundError::Key);
        }
        let key = RsaKey::<Sha256>::from_key_der(PrivateKeyDer::Pkcs8(der))
            .map_err(|_| OutboundError::Key)?;
        let signer = DkimSigner::from_key(key)
            .domain(domain.as_str())
            .selector(selector)
            .headers([
                "From",
                "To",
                "Subject",
                "Date",
                "Message-ID",
                "Reply-To",
                "In-Reply-To",
                "References",
                "Auto-Submitted",
                "MIME-Version",
                "Content-Type",
                "Content-Transfer-Encoding",
            ]);
        Ok(Self { domain, signer })
    }
    pub fn domain(&self) -> &str {
        self.domain.as_str()
    }
    /// DER-encoded RSA public key for `v=DKIM1; k=rsa; p=<base64>` in DNS.
    pub fn public_key(&self) -> Vec<u8> {
        self.signer.key.public_key()
    }
}

/// One notice to one recipient. Header values are single-line text.
#[derive(Clone)]
pub struct OutboundMail {
    pub thread: Option<MailThread>,
    /// Local part of the sender at the deployment domain, for example `notices`.
    pub from_local: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    /// Unique per notice; a resend of the same notice has the same identifier.
    pub message_id: String,
    /// RFC 5322 date, for example `Sat, 26 Sep 2026 16:00:00 +0000`.
    pub date: String,
}

#[derive(Clone)]
pub struct MailThread {
    pub reply_to: String,
    /// RFC Message-ID of the initial outbound message, including angle brackets.
    pub root: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum OutboundError {
    /// A header value contains a line break or non-ASCII text.
    InvalidHeader,
    Key,
    Signing,
}
impl fmt::Display for OutboundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidHeader => "invalid header value",
            Self::Key => "unusable DKIM key",
            Self::Signing => "DKIM signing failed",
        })
    }
}
impl std::error::Error for OutboundError {}

/// Renders `mail` as an RFC 5322 message with a DKIM-Signature header over its From, To,
/// Subject, Date and Message-ID headers and body.
/// # Errors
/// Rejects header injection and signing failures.
pub fn render_signed(mail: &OutboundMail, dkim: &DkimIdentity) -> Result<Vec<u8>, OutboundError> {
    let safe = |value: &str| !value.is_empty() && value.bytes().all(|b| (32..127).contains(&b));
    for value in [
        &mail.from_local,
        &mail.to,
        &mail.subject,
        &mail.message_id,
        &mail.date,
        dkim.domain(),
    ] {
        if !safe(value) {
            return Err(OutboundError::InvalidHeader);
        }
    }
    let mut thread_headers = String::new();
    if let Some(thread) = &mail.thread {
        if !safe(&thread.reply_to) || cs_mail_guests::GuestMailbox::parse(&thread.reply_to).is_err()
        {
            return Err(OutboundError::InvalidHeader);
        }
        thread_headers.push_str("Reply-To: ");
        thread_headers.push_str(&thread.reply_to);
        thread_headers.push_str("\r\n");
        if let Some(root) = &thread.root {
            if !safe(root) || !root.starts_with('<') || !root.ends_with('>') {
                return Err(OutboundError::InvalidHeader);
            }
            thread_headers.push_str("In-Reply-To: ");
            thread_headers.push_str(root);
            thread_headers.push_str("\r\nReferences: ");
            thread_headers.push_str(root);
            thread_headers.push_str("\r\n");
        }
    }
    let normalized = mail.body.lines().fold(String::new(), |mut body, line| {
        body.push_str(line);
        body.push_str("\r\n");
        body
    });
    let (encoding, body) = if mail.thread.is_some() {
        // A long Unicode paragraph must not create an illegal SMTP line.
        let encoded = STANDARD.encode(mail.body.as_bytes());
        let mut body = String::new();
        for row in encoded.as_bytes().chunks(76) {
            body.extend(row.iter().map(|byte| char::from(*byte)));
            body.push_str("\r\n");
        }
        ("base64", body)
    } else {
        ("8bit", normalized)
    };
    let message = format!(
        "From: {}@{}\r\nTo: {}\r\nSubject: {}\r\nDate: {}\r\nMessage-ID: <{}@{}>\r\n\
         {thread_headers}MIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\
         Content-Transfer-Encoding: {encoding}\r\n\r\n{body}",
        mail.from_local,
        dkim.domain(),
        mail.to,
        mail.subject,
        mail.date,
        mail.message_id,
        dkim.domain(),
    );
    let message = if mail.from_local == "notices" {
        format!("Auto-Submitted: auto-generated\r\n{message}")
    } else {
        message
    };
    let signature = dkim
        .signer
        .sign(message.as_bytes())
        .map_err(|_| OutboundError::Signing)?;
    let mut signed = signature.to_header().into_bytes();
    signed.extend_from_slice(message.as_bytes());
    Ok(signed)
}
