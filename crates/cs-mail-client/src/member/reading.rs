//! One document projection for reading and endpoint search. References are inert.
use super::{
    Error, MemberClient, State,
    storage::{self, Secrets},
};
use cs_mail_correspondence::{ContentBlock, Document, DocumentPart, ImageContent};
use cs_mail_primitives::MessageId;
#[derive(Clone, Debug)]
pub enum ReadingBlock {
    Text(String),
    Image { bytes: Vec<u8> },
}
#[derive(Clone, Debug)]
pub enum ReadingPart {
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
    pub(super) fn from_document(doc: &Document) -> Self {
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
        let result = if let Some(doc) = self.retained_document(state, keys, record)? {
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
        ReadingContent::from_document(&doc).search_text()
    }
}
