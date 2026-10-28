//! One document projection for reading and endpoint search. References are inert.
use super::{
    Error, MemberClient, State,
    storage::{self, Secrets},
};
use cs_mail_correspondence::{ContentBlock, Document, DocumentPart, ImageContent};
use cs_mail_primitives::MessageId;
#[derive(Clone, Debug)]
pub struct ReadingEmail {
    pub files: Vec<cs_mail_correspondence::composed::AttachedFile>,
    pub subject: Option<String>,
    pub html: Option<String>,
    pub text: String,
    pub supplied_text: Option<String>,
    pub resources: Vec<cs_mail_correspondence::email::EmailResource>,
    pub pictures: Vec<ReadingPicture>,
}
#[derive(Clone, Debug)]
pub struct ReadingPicture {
    pub id: String,
    pub url: String,
    pub status: PictureStatus,
}
#[derive(Clone, Copy, Debug)]
pub enum PictureStatus {
    NotLoaded,
    Attempted,
    Loaded,
}
impl From<cs_mail_correspondence::email::EmailPresentation> for ReadingEmail {
    fn from(p: cs_mail_correspondence::email::EmailPresentation) -> Self {
        Self {
            files: vec![],
            subject: p.subject,
            html: p.html,
            text: p.text,
            supplied_text: p.supplied_text,
            resources: p.resources,
            pictures: p
                .pictures
                .into_iter()
                .map(|p| ReadingPicture {
                    id: p.id,
                    url: p.url,
                    status: PictureStatus::NotLoaded,
                })
                .collect(),
        }
    }
}
#[derive(Clone, Debug)]
pub enum ReadingBlock {
    Text(String),
    Image { bytes: Vec<u8> },
}
#[derive(Clone, Debug)]
pub enum ReadingPart {
    Email(ReadingEmail),
    Text(String),
    Image {
        bytes: Vec<u8>,
    },
    Quotation {
        source: String,
        blocks: Vec<ReadingBlock>,
    },
    SharedCopy {
        source: String,
        blocks: Vec<ReadingBlock>,
    },
    Reference {
        source: String,
        part: u32,
    },
}
#[derive(Clone, Debug)]
pub enum ReadingContent {
    NotRetained,
    Readable(Vec<ReadingPart>),
    Unavailable,
    AccessDenied,
}
impl ReadingContent {
    pub(super) fn text_parts(&self) -> Vec<(cs_mail_search::TextPart, &str)> {
        use cs_mail_search::TextPart;
        let Self::Readable(parts) = self else {
            return vec![];
        };
        let mut result = Vec::new();
        for (part, value) in parts.iter().enumerate() {
            let part = u32::try_from(part).expect("checked document bounds part count");
            match value {
                ReadingPart::Text(text) => result.push((TextPart::Body { part }, text.as_str())),
                ReadingPart::Email(email) => {
                    if let Some(subject) = &email.subject {
                        result.push((TextPart::Subject, subject.as_str()));
                    }
                    result.push((TextPart::Body { part }, email.text.as_str()));
                }
                ReadingPart::Quotation { blocks, .. } | ReadingPart::SharedCopy { blocks, .. } => {
                    let quotation = matches!(value, ReadingPart::Quotation { .. });
                    for (block, value) in blocks.iter().enumerate() {
                        if let ReadingBlock::Text(text) = value {
                            let block =
                                u32::try_from(block).expect("checked document bounds block count");
                            let location = if quotation {
                                TextPart::Quotation { part, block }
                            } else {
                                TextPart::SharedCopy { part, block }
                            };
                            result.push((location, text.as_str()));
                        }
                    }
                }
                ReadingPart::Image { .. } | ReadingPart::Reference { .. } => (),
            }
        }
        result
    }
    pub(super) fn from_external_document(doc: &Document) -> Self {
        let mut content = Self::from_document(doc);
        if let (Some(subject), Self::Readable(parts)) = (doc.subject(), &mut content) {
            for part in parts {
                if let ReadingPart::Email(email) = part {
                    if email.subject.is_some() {
                        continue;
                    }
                    email.subject = Some(subject.text().to_owned());
                    email.text = format!("Subject: {}\n\n{}", subject.text(), email.text);
                }
            }
        }
        content
    }

    pub(super) fn from_document(doc: &Document) -> Self {
        if let [DocumentPart::Email(email)] = doc.parts() {
            return match email.presentation() {
                Ok(presentation) => Self::Readable(vec![ReadingPart::Email(presentation.into())]),
                Err(_) => Self::Unavailable,
            };
        }
        let image = |i: &ImageContent| ReadingBlock::Image {
            bytes: i.bytes().to_vec(),
        };
        let blocks = |b: &[ContentBlock]| {
            b.iter()
                .map(|b| match b {
                    ContentBlock::Text(t) => ReadingBlock::Text(t.clone()),
                    ContentBlock::Image(i) => image(i),
                })
                .collect()
        };
        Self::Readable(
            doc.parts()
                .iter()
                .enumerate()
                .map(|(index, p)| match p {
                    DocumentPart::Email(_) => {
                        unreachable!("a checked email is the sole document part")
                    }
                    DocumentPart::Composed(body) => match body.presentation() {
                        Ok(p) => {
                            let mut email: ReadingEmail = p.into();
                            email.files = body.files().to_vec();
                            ReadingPart::Email(email)
                        }
                        Err(_) => ReadingPart::Text(body.text().to_owned()),
                    },
                    DocumentPart::Text(t) => ReadingPart::Text(t.clone()),
                    DocumentPart::Image(i) => ReadingPart::Image {
                        bytes: i.bytes().to_vec(),
                    },
                    DocumentPart::Quotation(q) => ReadingPart::Quotation {
                        source: q.source().message().0.to_string(),
                        blocks: blocks(q.blocks()),
                    },
                    DocumentPart::Share(q) => ReadingPart::SharedCopy {
                        source: q.0.source().message().0.to_string(),
                        blocks: blocks(q.0.blocks()),
                    },
                    DocumentPart::Reference(r) => ReadingPart::Reference {
                        source: r.0.message().0.to_string(),
                        part: u32::try_from(index)
                            .expect("validated document bounds its parts to 128"),
                    },
                })
                .collect(),
        )
    }
    pub(super) fn search_text(&self) -> Result<String, Error> {
        let Self::Readable(parts) = self else {
            return Err(Error::SourceUnavailable);
        };
        let mut text = Vec::new();
        for part in parts {
            match part {
                ReadingPart::Email(email) => text.push(email.text.clone()),
                ReadingPart::Text(t) => text.push(t.clone()),
                ReadingPart::Quotation { blocks, .. } | ReadingPart::SharedCopy { blocks, .. } => {
                    for b in blocks {
                        if let ReadingBlock::Text(t) = b {
                            text.push(t.clone());
                        }
                    }
                }
                ReadingPart::Image { .. } | ReadingPart::Reference { .. } => (),
            }
        }
        Ok(text.join("\n\n"))
    }
}
impl MemberClient {
    pub(super) fn read_native(
        &self,
        state: &State,
        keys: &Secrets,
        record: &cs_mail_application::correspondence::MessageRecord,
    ) -> Result<ReadingContent, Error> {
        let message = record.manifest.message();
        let mut result = if let Some(doc) = self.retained_document(state, keys, record)? {
            ReadingContent::from_document(&doc)
        } else {
            let retained = super::retained::Retained::load(self, keys, state)?;
            if retained.entries.contains_key(&super::retained::CopyKey {
                thread: super::MailKey::Native(record.manifest.conversation()),
                message,
            }) || retained.excluded.iter().any(|key| {
                key.message == message && matches!(key.thread, super::MailKey::Native(_))
            }) {
                ReadingContent::Unavailable
            } else {
                ReadingContent::NotRetained
            }
        };
        let personal = super::personal::Personal::load(self, keys, state)?;
        Self::apply_resources(
            &personal,
            &mut result,
            super::MailKey::Native(record.manifest.conversation()),
            message,
            record.manifest.version(),
            (self.clock)(),
        );
        storage::check_keys(self.store.as_ref(), keys)?;
        Ok(result)
    }
    pub(super) fn external_text(
        &self,
        state: &State,
        keys: &Secrets,
        key: super::MailKey,
        message: MessageId,
    ) -> Result<String, Error> {
        let (doc, _, _) = self.external_source(state, keys, key, message)?;
        ReadingContent::from_external_document(&doc).search_text()
    }
    pub(super) fn external_source(
        &self,
        state: &State,
        keys: &Secrets,
        key: super::MailKey,
        message: MessageId,
    ) -> Result<
        (
            Document,
            cs_mail_correspondence::MessageVersion,
            cs_mail_primitives::CanonicalTime,
        ),
        Error,
    > {
        let super::MailKey::External(id) = key else {
            return Err(Error::InvalidData);
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
        let plain = zeroize::Zeroizing::new(
            cs_mail_content::decrypt(
                &secret,
                entry.message.sender_key,
                &entry.message.member_copy,
            )
            .map_err(|_| Error::InvalidData)?,
        );
        let doc: Document = serde_json::from_slice(&plain).map_err(|_| Error::InvalidData)?;
        if doc.manifest(message, id).map_err(|_| Error::InvalidData)? != entry.message.manifest
            || !entry.message.manifest.sources().is_empty()
        {
            return Err(Error::InvalidData);
        }
        Ok((
            doc,
            entry.message.manifest.version(),
            entry.message.committed_at,
        ))
    }
}
