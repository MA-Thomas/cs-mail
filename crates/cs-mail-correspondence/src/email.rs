//! Preserved email and deterministic presentation. Parsing never performs I/O.
use crate::CorrespondenceError;
use mail_parser::{HeaderName, Message, MessageParser, MimeHeaders, PartType};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

pub const MAX_EMAIL_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_EMAIL_PARTS: usize = 64;
pub const MAX_REMOTE_PICTURES: usize = 64;

/// Supplied MIME bytes, distinct from sanitized presentation and later retrievals.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "EmailRecord", into = "EmailRecord")]
pub struct EmailContent(EmailRecord);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmailRecord {
    format: EmailFormat,
    mime: Vec<u8>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum EmailFormat {
    MimeV1,
}
impl TryFrom<EmailRecord> for EmailContent {
    type Error = CorrespondenceError;
    fn try_from(record: EmailRecord) -> Result<Self, Self::Error> {
        validate_mime(&record.mime)?;
        Ok(Self(record))
    }
}
impl From<EmailContent> for EmailRecord {
    fn from(email: EmailContent) -> Self {
        email.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmailPicture {
    pub id: String,
    pub url: String,
}
/// A selected picture bound to this immutable email, produced only by explicit
/// approval. It is not a navigation destination or a reusable sender permission.
#[derive(Clone, Debug)]
pub struct ApprovedPicture(EmailPicture);
impl ApprovedPicture {
    pub fn id(&self) -> &str {
        &self.0.id
    }
    pub fn url(&self) -> &str {
        &self.0.url
    }
}
#[derive(Clone, Debug)]
pub struct EmailResource {
    pub id: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct EmailPresentation {
    pub subject: Option<String>,
    pub html: Option<String>,
    pub text: String,
    pub supplied_text: Option<String>,
    pub resources: Vec<EmailResource>,
    pub pictures: Vec<EmailPicture>,
}

/// A navigation destination, never authorization to visit it.
pub fn web_destination(value: &str) -> Option<String> {
    let url = url::Url::parse(value).ok()?;
    (matches!(url.scheme(), "https" | "http")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.to_string())
}
pub fn resource_id(value: &[u8]) -> String {
    use std::fmt::Write;
    Sha256::digest(value)
        .iter()
        .fold(String::with_capacity(64), |mut out, byte| {
            write!(out, "{byte:02x}").expect("writing to a string");
            out
        })
}
impl EmailContent {
    /// Accepts only the supported complete MIME tree; never silently drops attachments.
    /// # Errors
    /// Malformed, oversized or unsupported content.
    pub fn from_mime(mime: Vec<u8>) -> Result<Self, CorrespondenceError> {
        EmailRecord {
            format: EmailFormat::MimeV1,
            mime,
        }
        .try_into()
    }
    pub fn mime(&self) -> &[u8] {
        &self.0.mime
    }
    /// # Errors
    /// Empty, duplicated, excessive or unrelated picture selections.
    pub fn approve_pictures(
        &self,
        ids: &[String],
    ) -> Result<Vec<ApprovedPicture>, CorrespondenceError> {
        if ids.is_empty()
            || ids.len() > MAX_REMOTE_PICTURES
            || ids.iter().collect::<std::collections::BTreeSet<_>>().len() != ids.len()
        {
            return Err(CorrespondenceError::Invalid);
        }
        let presentation = self.presentation()?;
        ids.iter()
            .map(|id| {
                presentation
                    .pictures
                    .iter()
                    .find(|p| &p.id == id)
                    .cloned()
                    .map(ApprovedPicture)
                    .ok_or(CorrespondenceError::Invalid)
            })
            .collect()
    }
    /// Stable delivery content identity excludes transport trace headers and MIME
    /// transfer encodings, but includes every supplied representation and resource.
    #[allow(clippy::missing_panics_doc)] // Construction checked this immutable MIME tree.
    pub fn delivery_commitment(&self) -> [u8; 32] {
        let message = validate_mime(self.mime()).expect("checked immutable MIME");
        let mut hash = Sha256::new();
        hash.update(b"cs-mail/email-delivery-content/v1");
        let mut field = |tag: &[u8], value: &[u8]| {
            hash.update(tag);
            hash.update((value.len() as u64).to_be_bytes());
            hash.update(value);
        };
        field(b"subject", message.subject().unwrap_or_default().as_bytes());
        for part in &message.parts {
            if let PartType::Text(text) = &part.body {
                field(b"text", text.as_bytes());
            }
        }
        for part in &message.parts {
            if let PartType::Html(html) = &part.body {
                field(b"html", html.as_bytes());
            }
        }
        let mut resources = BTreeMap::new();
        for part in &message.parts {
            if let (
                Some(cid),
                Some(kind),
                PartType::Binary(bytes) | PartType::InlineBinary(bytes),
            ) = (part.content_id(), raster_mime(part), &part.body)
            {
                resources.insert(cid, (kind, bytes.as_ref()));
            }
        }
        for (cid, (kind, bytes)) in resources {
            field(b"cid", cid.as_bytes());
            field(b"kind", kind.as_bytes());
            field(b"bytes", bytes);
        }
        hash.finalize().into()
    }
    /// # Errors
    /// Invalid restored MIME or excessive remote resources.
    pub fn presentation(&self) -> Result<EmailPresentation, CorrespondenceError> {
        let message = validate_mime(self.mime())?;
        let supplied_text = message.parts.iter().find_map(|p| match &p.body {
            PartType::Text(s) => Some(s.to_string()),
            _ => None,
        });
        let html = message.parts.iter().find_map(|p| match &p.body {
            PartType::Html(s) => Some(s.as_ref()),
            _ => None,
        });
        let resources = embedded_resources(&message);
        let pictures = Arc::new(Mutex::new(BTreeMap::<String, String>::new()));
        let mut output = EmailPresentation {
            subject: message
                .subject()
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            html: None,
            text: supplied_text.clone().unwrap_or_default(),
            supplied_text,
            resources: resources.values().cloned().collect(),
            pictures: Vec::new(),
        };
        if let Some(html) = html {
            let observed = pictures.clone();
            let mut sanitizer = ammonia::Builder::default();
            // Isolated document: CSS may describe appearance, but the renderer denies
            // all network, script, frame, form and navigation authority independently.
            sanitizer
                .add_tags(&["style", "center", "font"])
                .clean_content_tags(
                    [
                        "script", "iframe", "object", "embed", "svg", "math", "template",
                    ]
                    .into(),
                )
                .add_generic_attributes(&[
                    "style", "class", "id", "dir", "lang", "align", "width", "height", "bgcolor",
                ])
                .add_tag_attributes("font", &["color", "face", "size"])
                .add_tag_attributes("table", &["cellpadding", "cellspacing", "border", "role"])
                .url_schemes(["http", "https", "cid", "cs-mail-resource"].into())
                .url_relative(ammonia::UrlRelative::Deny)
                .attribute_filter(move |tag, attr, value| {
                    if attr == "src" {
                        if tag != "img" {
                            return None;
                        }
                        if let Some(cid) = value.strip_prefix("cid:") {
                            return resources
                                .get(cid)
                                .map(|r| Cow::Owned(format!("cs-mail-resource://image/{}", r.id)));
                        }
                        let url = web_destination(value)?;
                        let id = resource_id(url.as_bytes());
                        observed.lock().ok()?.insert(id.clone(), url);
                        return Some(Cow::Owned(format!("cs-mail-resource://image/{id}")));
                    }
                    if attr == "href" {
                        return (tag == "a")
                            .then(|| web_destination(value))
                            .flatten()
                            .map(Cow::Owned);
                    }
                    Some(Cow::Borrowed(value))
                });
            let fragment = sanitizer.clean(html).to_string();
            output.text = super::email_text::extract(&fragment);
            output.html = Some(format!(
                "<!doctype html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src cs-mail-resource:; font-src 'none'; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'\"><meta name=\"referrer\" content=\"no-referrer\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><style>body{{margin:12px;background:white;color:#222;overflow-wrap:anywhere}}img{{max-width:100%;height:auto}}table{{max-width:100%}}</style></head><body>{fragment}</body></html>"
            ));
            output.pictures = pictures
                .lock()
                .map_err(|_| CorrespondenceError::Invalid)?
                .iter()
                .map(|(id, url)| EmailPicture {
                    id: id.clone(),
                    url: url.clone(),
                })
                .collect();
            if output.pictures.len() > MAX_REMOTE_PICTURES {
                return Err(CorrespondenceError::Invalid);
            }
        }
        if let Some(subject) = message.subject().filter(|s| !s.is_empty()) {
            output.text = format!("Subject: {subject}\n\n{}", output.text);
        }
        Ok(output)
    }
}
fn embedded_resources(message: &Message<'_>) -> BTreeMap<String, EmailResource> {
    let mut resources = BTreeMap::new();
    for part in &message.parts {
        if let Some(cid) = part.content_id() {
            if let (Some(kind), PartType::InlineBinary(bytes) | PartType::Binary(bytes)) =
                (raster_mime(part), &part.body)
            {
                resources.insert(
                    cid.to_owned(),
                    EmailResource {
                        id: resource_id(bytes),
                        mime_type: kind.into(),
                        bytes: bytes.to_vec(),
                    },
                );
            }
        }
    }
    resources
}

fn raster_mime(part: &mail_parser::MessagePart<'_>) -> Option<&'static str> {
    ["png", "jpeg", "webp"]
        .into_iter()
        .find(|s| part.is_content_type("image", s))
        .map(|s| match s {
            "png" => "image/png",
            "jpeg" => "image/jpeg",
            _ => "image/webp",
        })
}
#[allow(clippy::too_many_lines)] // Complete MIME-tree acceptance must reject every unsupported sibling.
fn validate_mime(raw: &[u8]) -> Result<Message<'_>, CorrespondenceError> {
    let invalid = CorrespondenceError::Invalid;
    if raw.is_empty() || raw.len() > MAX_EMAIL_BYTES {
        return Err(invalid);
    }
    let message = MessageParser::default().parse(raw).ok_or(invalid)?;
    if message.parts.is_empty() || message.parts.len() > MAX_EMAIL_PARTS {
        return Err(invalid);
    }
    let mut texts = 0;
    let mut htmls = 0;
    let mut cids = std::collections::BTreeSet::new();
    for part in &message.parts {
        if part.is_encoding_problem
            || (part.attachment_name().is_some()
                && !(raster_mime(part).is_some() && part.content_id().is_some()))
            || part
                .content_disposition()
                .is_some_and(|d| !d.c_type.eq_ignore_ascii_case("inline"))
        {
            return Err(invalid);
        }
        for header in [
            HeaderName::ContentType,
            HeaderName::ContentTransferEncoding,
            HeaderName::ContentDisposition,
        ] {
            let mut headers = part.headers.iter().filter(|h| h.name == header);
            if let Some(h) = headers.next() {
                if headers.next().is_some()
                    || match header {
                        HeaderName::ContentTransferEncoding => h.value.as_text().is_none(),
                        _ => h.value.as_content_type().is_none(),
                    }
                {
                    return Err(invalid);
                }
            }
        }
        if let Some(encoding) = part.content_transfer_encoding() {
            if !["7bit", "8bit", "binary", "base64", "quoted-printable"]
                .iter()
                .any(|e| encoding.eq_ignore_ascii_case(e))
                || (matches!(part.body, PartType::Multipart(_))
                    && ["base64", "quoted-printable"]
                        .iter()
                        .any(|e| encoding.eq_ignore_ascii_case(e)))
            {
                return Err(invalid);
            }
        }
        if let Some(kind) = part.content_type() {
            if let Some(attrs) = kind.attributes() {
                for (i, attr) in attrs.iter().enumerate() {
                    if attrs[..i]
                        .iter()
                        .any(|a| a.name.eq_ignore_ascii_case(&attr.name))
                    {
                        return Err(invalid);
                    }
                }
            }
            if let Some(charset) = kind.attribute("charset") {
                if !charset.eq_ignore_ascii_case("utf-8")
                    && !charset.eq_ignore_ascii_case("utf8")
                    && mail_parser::decoders::charsets::map::charset_decoder(charset.as_bytes())
                        .is_none()
                {
                    return Err(invalid);
                }
            }
        }
        match &part.body {
            PartType::Text(s)
                if part.content_type().is_none() || part.is_content_type("text", "plain") =>
            {
                texts += 1;
                if s.contains('\u{fffd}') {
                    return Err(invalid);
                }
            }
            PartType::Html(s) if part.is_content_type("text", "html") => {
                htmls += 1;
                if s.contains('\u{fffd}') {
                    return Err(invalid);
                }
            }
            PartType::Multipart(ids)
                if part.is_content_type("multipart", "alternative")
                    || part.is_content_type("multipart", "related") =>
            {
                if ids.is_empty() || ids.iter().any(|i| *i as usize >= message.parts.len()) {
                    return Err(invalid);
                }
                validate_framing(&message, part, ids)?;
            }
            PartType::InlineBinary(bytes) | PartType::Binary(bytes)
                if raster_mime(part).is_some() && !bytes.is_empty() =>
            {
                let cid = part.content_id().ok_or(invalid)?;
                if !cids.insert(cid) {
                    return Err(invalid);
                }
            }
            _ => return Err(invalid),
        }
    }
    if texts > 1 || htmls > 1 || texts + htmls == 0 {
        return Err(invalid);
    }
    Ok(message)
}

fn validate_framing(
    message: &Message<'_>,
    part: &mail_parser::MessagePart<'_>,
    ids: &[u32],
) -> Result<(), CorrespondenceError> {
    let invalid = CorrespondenceError::Invalid;
    let boundary = part
        .content_type()
        .and_then(|t| t.attribute("boundary"))
        .ok_or(invalid)?;
    if boundary.is_empty()
        || boundary.len() > 70
        || boundary.ends_with(' ')
        || !boundary
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"'()+_,-./:=? ".contains(&b))
    {
        return Err(invalid);
    }
    let opening = format!("--{boundary}");
    let closing = format!("--{boundary}--");
    let mut position = part.offset_body as usize;
    let mut delimiters = 0;
    let body = message
        .raw_message
        .get(position..part.offset_end as usize)
        .ok_or(invalid)?;
    for line in body.split_inclusive(|b| *b == b'\n') {
        position += line.len();
        let line = line.trim_ascii_end();
        if line.starts_with(opening.as_bytes()) {
            let valid = if delimiters < ids.len() {
                line == opening.as_bytes()
                    && position == message.parts[ids[delimiters] as usize].offset_header as usize
            } else {
                delimiters == ids.len() && line == closing.as_bytes()
            };
            if !valid {
                return Err(invalid);
            }
            delimiters += 1;
        }
    }
    if delimiters != ids.len() + 1 {
        return Err(invalid);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    // Conjecture: rich presentation survives while resource discovery conveys no authority.
    // Losing a warning/table or retaining an active navigation mechanism falsifies it.
    #[test]
    fn rich_email_preserves_supplied_content_and_separates_external_pictures() {
        let raw = b"Subject: Clinical instructions\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<style>.warning{color:red;font-weight:bold}</style><h1>Instructions</h1><table><tr><th>Dose</th><th>Time</th></tr><tr><td>5 mg</td><td>Evening</td></tr></table><p class=warning style='border:2px solid red'>Call before changing</p><img src='https://pictures.example.test/patient-123' alt='Chart'><a href='https://example.test/action'>Visit</a><script>fetch('https://example.test/leak')</script><iframe src='https://example.test/frame'></iframe><form action='https://example.test/submit'><input name=secret></form><meta http-equiv=refresh content='0;url=https://example.test/redirect'>";
        let email = EmailContent::from_mime(raw.to_vec()).unwrap();
        let p = email.presentation().unwrap();
        assert_eq!(email.mime(), raw);
        let html = p.html.unwrap();
        assert!(
            html.contains("<table>") && html.contains("<th>Dose</th>") && html.contains(".warning")
        );
        assert!(html.contains("font-weight:bold") && html.contains("Call before changing"));
        assert!(
            !html.contains("<script>")
                && !html.contains("<iframe")
                && !html.contains("<form")
                && !html.contains("http-equiv=refresh")
        );
        assert!(!html.contains("src=\"https:"));
        assert_eq!(p.pictures.len(), 1);
        assert_eq!(
            p.pictures[0].url,
            "https://pictures.example.test/patient-123"
        );
        assert!(html.contains(&format!("cs-mail-resource://image/{}", p.pictures[0].id)));
        assert!(p.text.contains("5 mg") && p.text.contains("Evening"));
        assert!(
            p.text.contains("Instructions\nDose\nTime\n5 mg\nEvening"),
            "headings and table cells must not merge into unsearchable words"
        );
        assert!(
            !p.text.contains("font-weight"),
            "style code is not correspondence text"
        );
        let restored: EmailContent =
            serde_json::from_slice(&serde_json::to_vec(&email).unwrap()).unwrap();
        assert_eq!(restored, email);
        // Unsupported structure may not be accepted by silently dropping content.
        assert!(EmailContent::from_mime(b"Content-Type: multipart/mixed; boundary=x\r\n\r\n--x\r\nContent-Type: text/plain\r\n\r\ntext\r\n--x\r\nContent-Type: application/pdf\r\n\r\ndata\r\n--x--\r\n".to_vec()).is_err());
    }
    // Conjecture: supplied CID resources remain local and message alternatives remain distinct.
    #[test]
    fn supplied_resources_and_alternatives_are_preserved_without_network_destinations() {
        let raw = b"Content-Type: multipart/related; boundary=outer\r\n\r\n--outer\r\nContent-Type: multipart/alternative; boundary=inner\r\n\r\n--inner\r\nContent-Type: text/plain\r\n\r\nplain alternative\r\n--inner\r\nContent-Type: text/html\r\n\r\n<p>Formatted warning</p><img src='cid:chart' alt='chart'>\r\n--inner--\r\n--outer\r\nContent-Type: image/png\r\nContent-ID: <chart>\r\nContent-Transfer-Encoding: base64\r\n\r\naW1hZ2U=\r\n--outer--\r\n";
        let email = EmailContent::from_mime(raw.to_vec()).unwrap();
        let p = email.presentation().unwrap();
        assert!(p.supplied_text.unwrap().contains("plain alternative"));
        assert!(p.text.contains("Formatted warning"));
        assert_eq!(p.resources.len(), 1);
        assert_eq!(p.resources[0].bytes, b"image"); // Decoder acceptance is a separate boundary.
        assert!(p.pictures.is_empty());
        assert!(p.html.unwrap().contains(&p.resources[0].id));
        assert!(EmailContent::from_mime(raw[..raw.len() - 13].to_vec()).is_err());
    }
}
