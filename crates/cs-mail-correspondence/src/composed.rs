//! Authored presentation and retained files. Construction and MIME generation do no I/O.
use crate::{CorrespondenceError, ImageContent, ImageFormat};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

pub const MAX_COMPOSED_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_FILES: usize = 24;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CompositionFormat {
    Editable,
    Imported,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "FileRecord", into = "FileRecord")]
pub struct AttachedFile(FileRecord);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRecord {
    name: String,
    #[serde(with = "encoded_bytes")]
    bytes: Vec<u8>,
}
impl TryFrom<FileRecord> for AttachedFile {
    type Error = CorrespondenceError;
    fn try_from(v: FileRecord) -> Result<Self, Self::Error> {
        if v.name.is_empty()
            || v.name.len() > 255
            || v.name
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\'))
            || v.bytes.len() > MAX_COMPOSED_BYTES
        {
            return Err(CorrespondenceError::Invalid);
        }
        Ok(Self(v))
    }
}
impl From<AttachedFile> for FileRecord {
    fn from(v: AttachedFile) -> Self {
        v.0
    }
}
impl AttachedFile {
    /// # Errors
    /// Refuses path names, control characters and oversized files.
    pub fn new(name: String, bytes: Vec<u8>) -> Result<Self, CorrespondenceError> {
        FileRecord { name, bytes }.try_into()
    }
    pub fn name(&self) -> &str {
        &self.0.name
    }
    pub fn bytes(&self) -> &[u8] {
        &self.0.bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ComposedRecord", into = "ComposedRecord")]
pub struct ComposedBody {
    source: ComposedRecord,
    text: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ComposedRecord {
    format: CompositionFormat,
    html: String,
    #[serde(with = "encoded_images")]
    images: Vec<ImageContent>,
    files: Vec<AttachedFile>,
}
impl TryFrom<ComposedRecord> for ComposedBody {
    type Error = CorrespondenceError;
    fn try_from(v: ComposedRecord) -> Result<Self, Self::Error> {
        let total = v
            .html
            .len()
            .checked_add(v.images.iter().map(|i| i.bytes().len()).sum::<usize>())
            .and_then(|n| n.checked_add(v.files.iter().map(|f| f.bytes().len()).sum::<usize>()));
        if total.is_none_or(|n| n > MAX_COMPOSED_BYTES)
            || v.images.len() + v.files.len() > MAX_FILES
        {
            return Err(CorrespondenceError::Invalid);
        }
        let mut ids = BTreeSet::new();
        if v.images
            .iter()
            .any(|i| !ids.insert(crate::email::resource_id(i.bytes())))
        {
            return Err(CorrespondenceError::Invalid);
        }
        let mut body = Self {
            source: v,
            text: String::new(),
        };
        // Stored source is sanitized on export/preview, never executable markup.
        body.text = crate::email_text::extract(&body.clean_html()?);
        if body.text.len() > crate::MAX_TEXT_BYTES {
            return Err(CorrespondenceError::Invalid);
        }
        Ok(body)
    }
}
impl From<ComposedBody> for ComposedRecord {
    fn from(v: ComposedBody) -> Self {
        v.source
    }
}
impl ComposedBody {
    /// # Errors
    /// Refuses excessive content, missing inline images and unsafe image destinations.
    pub fn new(
        format: CompositionFormat,
        html: String,
        images: Vec<ImageContent>,
        files: Vec<AttachedFile>,
    ) -> Result<Self, CorrespondenceError> {
        ComposedRecord {
            format,
            html,
            images,
            files,
        }
        .try_into()
    }
    pub fn html(&self) -> &str {
        &self.source.html
    }
    pub fn format(&self) -> &CompositionFormat {
        &self.source.format
    }
    pub fn images(&self) -> &[ImageContent] {
        &self.source.images
    }
    pub fn has_content(&self) -> bool {
        !self.text.trim().is_empty()
            || !self.source.images.is_empty()
            || !self.source.files.is_empty()
            || self
                .clean_html()
                .is_ok_and(|html| image_sources(&html).is_ok_and(|sources| !sources.is_empty()))
    }
    pub fn files(&self) -> &[AttachedFile] {
        &self.source.files
    }
    /// # Errors
    /// An inline resource is missing or a local/unsupported image address remains.
    pub fn clean_html(&self) -> Result<String, CorrespondenceError> {
        sanitize(self.html(), self.images(), BTreeMap::new())
    }
    /// # Errors
    /// Every local inline image must be explicitly supplied with its original source address.
    pub fn import(
        html: &str,
        resources: Vec<(String, ImageContent)>,
        files: Vec<AttachedFile>,
    ) -> Result<Self, CorrespondenceError> {
        if html.len() > MAX_COMPOSED_BYTES {
            return Err(CorrespondenceError::Invalid);
        }
        let replacements = resources
            .iter()
            .map(|(source, image)| (source.clone(), crate::email::resource_id(image.bytes())))
            .collect();
        let mut images = Vec::new();
        let mut ids = BTreeSet::new();
        for (_, image) in resources {
            if ids.insert(crate::email::resource_id(image.bytes())) {
                images.push(image);
            }
        }
        let html = sanitize(html, &images, replacements)?;
        Self::new(CompositionFormat::Imported, html, images, files)
    }
    /// # Errors
    /// Invalid supplied HTML resources.
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Preview uses exactly the sanitized outgoing HTML and retained images, with no file attachments or network authority.
    /// # Errors
    /// Invalid content or presentation size.
    pub fn presentation(&self) -> Result<crate::email::EmailPresentation, CorrespondenceError> {
        crate::email::EmailContent::from_mime(self.mime(false)?).and_then(|e| e.presentation())
    }
    /// Deterministic content-only MIME; envelope and subject are supplied by the sending adapter.
    /// # Errors
    /// Invalid restored content.
    pub fn mime(&self, include_files: bool) -> Result<Vec<u8>, CorrespondenceError> {
        let html = self.clean_html()?;
        let text = crate::email_text::extract(&html);
        let digest = crate::email::resource_id(
            &serde_json::to_vec(self).map_err(|_| CorrespondenceError::Invalid)?,
        );
        let digest = &digest[..32];
        let alternative = format!("cs-alt-{digest}");
        let related = format!("cs-img-{digest}");
        let mixed = format!("cs-file-{digest}");
        let leaf = |kind: &str, bytes: &[u8]| {
            format!(
                "Content-Type: {kind}\r\nContent-Transfer-Encoding: base64\r\n\r\n{}",
                encoded_lines(bytes)
            )
        };
        let mut body = format!(
            "Content-Type: multipart/alternative; boundary=\"{alternative}\"\r\n\r\n--{alternative}\r\n{}--{alternative}\r\n{}--{alternative}--\r\n",
            leaf("text/plain; charset=utf-8", text.as_bytes()),
            leaf("text/html; charset=utf-8", html.as_bytes())
        );
        if !self.images().is_empty() {
            body = format!(
                "Content-Type: multipart/related; boundary=\"{related}\"\r\n\r\n--{related}\r\n{body}"
            );
            for image in self.images() {
                let id = crate::email::resource_id(image.bytes());
                let kind = match image.format() {
                    ImageFormat::Png => "image/png",
                    ImageFormat::Jpeg => "image/jpeg",
                    ImageFormat::WebP => "image/webp",
                };
                write!(
                    body,
                    "--{related}\r\nContent-ID: <{id}>\r\nContent-Disposition: inline\r\n{}",
                    leaf(kind, image.bytes())
                )
                .map_err(|_| CorrespondenceError::Invalid)?;
            }
            write!(body, "--{related}--\r\n").map_err(|_| CorrespondenceError::Invalid)?;
        }
        if include_files && !self.files().is_empty() {
            body = format!(
                "Content-Type: multipart/mixed; boundary=\"{mixed}\"\r\n\r\n--{mixed}\r\n{body}"
            );
            for file in self.files() {
                let name: String =
                    file.name()
                        .as_bytes()
                        .iter()
                        .fold(String::new(), |mut out, b| {
                            let _ = write!(out, "%{b:02X}");
                            out
                        });
                write!(
                    body,
                    "--{mixed}\r\nContent-Disposition: attachment; filename*=utf-8''{name}\r\n{}",
                    leaf("application/octet-stream", file.bytes())
                )
                .map_err(|_| CorrespondenceError::Invalid)?;
            }
            write!(body, "--{mixed}--\r\n").map_err(|_| CorrespondenceError::Invalid)?;
        }
        Ok(body.into_bytes())
    }
}
fn encoded_lines(bytes: &[u8]) -> String {
    let encoded = STANDARD.encode(bytes);
    encoded
        .as_bytes()
        .chunks(76)
        .fold(String::new(), |mut out, line| {
            out.push_str(std::str::from_utf8(line).expect("base64 ASCII"));
            out.push_str("\r\n");
            out
        })
}

fn sanitize(
    html: &str,
    images: &[ImageContent],
    replacements: BTreeMap<String, String>,
) -> Result<String, CorrespondenceError> {
    let ids: BTreeSet<_> = images
        .iter()
        .map(|i| crate::email::resource_id(i.bytes()))
        .collect();
    let mut remote = BTreeSet::new();
    for source in image_sources(html)? {
        if replacements.contains_key(&source)
            || source
                .strip_prefix("cid:")
                .is_some_and(|id| ids.contains(id))
        {
            continue;
        }
        let destination =
            crate::email::web_destination(&source).ok_or(CorrespondenceError::Invalid)?;
        remote.insert(destination);
    }
    if remote.len() > crate::email::MAX_REMOTE_PICTURES {
        return Err(CorrespondenceError::Invalid);
    }
    let missing = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let invalid = missing.clone();
    let mut builder = ammonia::Builder::default();
    builder
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
        .url_schemes(["http", "https", "cid"].into())
        .url_relative(ammonia::UrlRelative::PassThrough)
        .attribute_filter(move |tag, attr, value| {
            if attr == "src" {
                if tag != "img" {
                    return None;
                }
                if let Some(id) = replacements.get(value) {
                    return Some(Cow::Owned(format!("cid:{id}")));
                }
                if value
                    .strip_prefix("cid:")
                    .is_some_and(|id| ids.contains(id))
                {
                    return Some(Cow::Borrowed(value));
                }
                if let Some(url) = crate::email::web_destination(value) {
                    return Some(Cow::Owned(url));
                }
                invalid.store(true, std::sync::atomic::Ordering::Relaxed);
                return None;
            }
            if attr == "href" {
                return (tag == "a")
                    .then(|| crate::email::web_destination(value))
                    .flatten()
                    .map(Cow::Owned);
            }
            Some(Cow::Borrowed(value))
        });
    let html = builder.clean(html).to_string();
    if missing.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(CorrespondenceError::Invalid);
    }
    Ok(html)
}

/// Lists inline image dependencies without resolving or retrieving any address.
/// # Errors
/// Oversized design source.
pub fn image_sources(html: &str) -> Result<Vec<String>, CorrespondenceError> {
    use html5ever::{
        buffer_queue::BufferQueue,
        tokenizer::{
            TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts, states::RawKind,
        },
    };
    use std::cell::RefCell;
    struct Images(RefCell<BTreeSet<String>>);
    impl TokenSink for Images {
        type Handle = ();
        fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
            if let Token::TagToken(tag) = token {
                if tag.kind == TagKind::StartTag {
                    if ["script", "style"].contains(&tag.name.as_ref()) {
                        return TokenSinkResult::RawData(RawKind::Rawtext);
                    }
                    if &*tag.name == "img" {
                        for attr in tag.attrs {
                            if &*attr.name.local == "src" {
                                self.0.borrow_mut().insert(attr.value.to_string());
                            }
                        }
                    }
                }
            }
            TokenSinkResult::Continue
        }
    }
    if html.len() > MAX_COMPOSED_BYTES {
        return Err(CorrespondenceError::Invalid);
    }
    let tokenizer = Tokenizer::new(
        Images(RefCell::new(BTreeSet::new())),
        TokenizerOpts::default(),
    );
    let queue = BufferQueue::default();
    queue.push_back(html.into());
    let _ = tokenizer.feed(&queue);
    tokenizer.end();
    Ok(tokenizer.sink.0.into_inner().into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mail_parser::MimeHeaders;
    // Conjecture: saved presentation and files identify one complete message; a
    // changed attachment cannot preserve its manifest or deterministic MIME bytes.
    #[test]
    fn presentation_files_and_retries_share_one_checked_document() {
        let body = ComposedBody::new(
            CompositionFormat::Editable,
            "<h2>Plans</h2><p><b>Bring the report</b></p>".into(),
            vec![],
            vec![
                AttachedFile::new("résumé.pdf".into(), b"attachment-private-canary".to_vec())
                    .unwrap(),
            ],
        )
        .unwrap();
        let restored: ComposedBody =
            serde_json::from_slice(&serde_json::to_vec(&body).unwrap()).unwrap();
        assert_eq!(body, restored);
        assert_eq!(body.mime(true).unwrap(), restored.mime(true).unwrap());
        let mime = body.mime(true).unwrap();
        let parsed = mail_parser::MessageParser::default().parse(&mime).unwrap();
        assert_eq!(
            parsed.attachment(0).unwrap().attachment_name(),
            Some("résumé.pdf")
        );
        assert_eq!(
            parsed.attachment(0).unwrap().contents(),
            b"attachment-private-canary"
        );
        let preview = body.presentation().unwrap();
        assert!(preview.html.unwrap().contains("<b>Bring the report</b>"));
        assert!(preview.text.contains("Plans\nBring the report"));
        let changed = ComposedBody::new(
            CompositionFormat::Editable,
            body.html().into(),
            vec![],
            vec![AttachedFile::new("résumé.pdf".into(), b"changed".to_vec()).unwrap()],
        )
        .unwrap();
        let document = |body| {
            crate::Document::new(vec![crate::DocumentPart::Composed(body)], [7; 32])
                .unwrap()
                .manifest(
                    cs_mail_primitives::MessageId(1),
                    crate::ConversationId::new(2).unwrap(),
                )
                .unwrap()
        };
        assert_ne!(document(body.clone()), document(changed));
        let manifest = document(body);
        assert!(matches!(
            manifest.blocks(),
            [
                crate::BlockDescriptor::Text { .. },
                crate::BlockDescriptor::Attachment { .. }
            ]
        ));
    }
    // Conjecture: an imported design cannot hide missing local pictures, and
    // supplied image dependencies never become authority to load external content.
    #[test]
    fn imports_bind_local_images_and_preview_remains_inert() {
        let html = "<style>.title{color:red}</style><h1 class=title>Invitation</h1><img src='pictures/cover.png'><img src='https://example.test/track'><script>fetch('https://example.test/leak')</script>";
        assert_eq!(
            image_sources(html).unwrap(),
            vec!["https://example.test/track", "pictures/cover.png"]
        );
        assert!(
            ComposedBody::new(CompositionFormat::Imported, html.into(), vec![], vec![]).is_err()
        );
        for invalid in [
            "<img src='file:///private/picture.png'>",
            "<img src='data:image/png;base64,aW1hZ2U='>",
        ] {
            assert!(
                ComposedBody::new(CompositionFormat::Editable, invalid.into(), vec![], vec![])
                    .is_err()
            );
        }
        let image = ImageContent::new(ImageFormat::Png, b"image bytes".to_vec()).unwrap();
        let body =
            ComposedBody::import(html, vec![("pictures/cover.png".into(), image)], vec![]).unwrap();
        let preview = body.presentation().unwrap();
        assert_eq!(preview.resources.len(), 1);
        assert_eq!(preview.pictures.len(), 1);
        assert!(preview.html.as_ref().unwrap().contains("script-src 'none'"));
        assert!(!preview.html.as_ref().unwrap().contains("<script>"));
        assert!(!body.clean_html().unwrap().contains("pictures/cover.png"));
        assert!(body.clean_html().unwrap().contains(".title"));
    }
}

mod encoded_bytes {
    use super::{Engine, MAX_COMPOSED_BYTES, STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(value))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.len() > MAX_COMPOSED_BYTES.div_ceil(3) * 4 {
            return Err(serde::de::Error::custom("file exceeds composition bound"));
        }
        STANDARD.decode(value).map_err(serde::de::Error::custom)
    }
}
mod encoded_images {
    use super::{ImageContent, ImageFormat, encoded_bytes};
    use serde::{Deserialize, Serialize};
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct EncodedImage {
        format: ImageFormat,
        #[serde(with = "encoded_bytes")]
        bytes: Vec<u8>,
    }
    pub fn serialize<S: serde::Serializer>(
        value: &[ImageContent],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value
            .iter()
            .map(|i| EncodedImage {
                format: i.format(),
                bytes: i.bytes().to_vec(),
            })
            .collect::<Vec<_>>()
            .serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<ImageContent>, D::Error> {
        Vec::<EncodedImage>::deserialize(deserializer)?
            .into_iter()
            .map(|i| ImageContent::new(i.format, i.bytes).map_err(serde::de::Error::custom))
            .collect()
    }
}
