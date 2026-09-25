use crate::{
    ConversationId, CorrespondenceError, ImageContent, MessageSelection, SelectionElement,
};
use cs_mail_primitives::MessageId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_PARTS: usize = 128;
pub const MAX_BLOCKS: usize = 128;
pub const MAX_TEXT_BYTES: usize = 1_048_576;
pub const MAX_IMAGE_BYTES: usize = 4 * 1_048_576;
/// Bounds the compact JSON representation, including escaped text and image arrays.
pub const MAX_ENCODED_DOCUMENT_BYTES: usize = 24 * 1_048_576;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MessageVersion(pub [u8; 32]);

/// Nonplaintext shape used to check selection types and bounds at the service.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum BlockDescriptor {
    Text { bytes: u32 },
    Image { bytes: u32 },
    Reference,
}

/// Retained selected content; references cannot masquerade as retained image bytes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ContentBlock {
    Text(String),
    Image(ImageContent),
}
impl ContentBlock {
    fn borrowed(&self) -> BlockRef<'_> {
        match self {
            Self::Text(t) => BlockRef::Text(t),
            Self::Image(i) => BlockRef::Image(i),
        }
    }
}
#[derive(Clone, Copy)]
enum BlockRef<'a> {
    Text(&'a str),
    Image(&'a ImageContent),
    Reference,
}
impl BlockRef<'_> {
    fn descriptor(self) -> Result<BlockDescriptor, CorrespondenceError> {
        let bounded = |len| u32::try_from(len).map_err(|_| CorrespondenceError::Invalid);
        Ok(match self {
            Self::Text(t) => BlockDescriptor::Text {
                bytes: bounded(t.len())?,
            },
            Self::Image(i) => BlockDescriptor::Image {
                bytes: bounded(i.bytes().len())?,
            },
            Self::Reference => BlockDescriptor::Reference,
        })
    }
}
fn validate_blocks(blocks: &[BlockDescriptor]) -> Result<(), CorrespondenceError> {
    let mut text = 0_u64;
    let mut images = 0_u64;
    if blocks.is_empty() || blocks.len() > MAX_BLOCKS {
        return Err(CorrespondenceError::Invalid);
    }
    for block in blocks {
        match block {
            BlockDescriptor::Text { bytes } => text += u64::from(*bytes),
            BlockDescriptor::Image { bytes } if *bytes > 0 => images += u64::from(*bytes),
            BlockDescriptor::Image { .. } => return Err(CorrespondenceError::Invalid),
            BlockDescriptor::Reference => {}
        }
    }
    if text > MAX_TEXT_BYTES as u64 || images > MAX_IMAGE_BYTES as u64 {
        return Err(CorrespondenceError::Invalid);
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceReference(pub MessageSelection);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ExcerptRecord", into = "ExcerptRecord")]
pub struct QuotedExcerpt(ExcerptRecord);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExcerptRecord {
    source: MessageSelection,
    blocks: Vec<ContentBlock>,
}
impl TryFrom<ExcerptRecord> for QuotedExcerpt {
    type Error = CorrespondenceError;
    fn try_from(v: ExcerptRecord) -> Result<Self, Self::Error> {
        if v.blocks.len() != v.source.elements().len() {
            return Err(CorrespondenceError::Invalid);
        }
        for (element, block) in v.source.elements().iter().zip(&v.blocks) {
            let valid = match (element, block) {
                (SelectionElement::TextRange { start, end, .. }, ContentBlock::Text(t)) => {
                    t.len() == (end - start) as usize
                }
                (SelectionElement::ImageBlock { .. }, ContentBlock::Image(_)) => true,
                _ => false,
            };
            if !valid {
                return Err(CorrespondenceError::Invalid);
            }
        }
        validate_blocks(
            &v.blocks
                .iter()
                .map(|b| b.borrowed().descriptor())
                .collect::<Result<Vec<_>, _>>()?,
        )?;
        Ok(Self(v))
    }
}
impl From<QuotedExcerpt> for ExcerptRecord {
    fn from(v: QuotedExcerpt) -> Self {
        v.0
    }
}
impl QuotedExcerpt {
    pub fn source(&self) -> &MessageSelection {
        &self.0.source
    }
    pub fn blocks(&self) -> &[ContentBlock] {
        &self.0.blocks
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SharedCopy(pub QuotedExcerpt);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DocumentPart {
    Text(String),
    Image(ImageContent),
    Reference(SourceReference),
    Quotation(QuotedExcerpt),
    Share(SharedCopy),
}
impl DocumentPart {
    fn blocks(&self) -> Vec<BlockRef<'_>> {
        match self {
            Self::Text(t) => vec![BlockRef::Text(t)],
            Self::Image(i) => vec![BlockRef::Image(i)],
            Self::Reference(_) => vec![BlockRef::Reference],
            Self::Quotation(q) => q.blocks().iter().map(ContentBlock::borrowed).collect(),
            Self::Share(s) => s.0.blocks().iter().map(ContentBlock::borrowed).collect(),
        }
    }
    pub fn source(&self) -> Option<SourceUse> {
        match self {
            Self::Text(_) | Self::Image(_) => None,
            Self::Reference(r) => Some(SourceUse::Reference(r.0.clone())),
            Self::Quotation(q) => Some(SourceUse::Quotation(q.source().clone())),
            Self::Share(s) => Some(SourceUse::Sharing(s.0.source().clone())),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SourceUse {
    Reference(MessageSelection),
    Quotation(MessageSelection),
    Sharing(MessageSelection),
}
impl SourceUse {
    pub fn selection(&self) -> &MessageSelection {
        match self {
            Self::Reference(s) | Self::Quotation(s) | Self::Sharing(s) => s,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "DocumentRecord", into = "DocumentRecord")]
pub struct Document(DocumentRecord);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DocumentRecord {
    version_secret: [u8; 32],
    parts: Vec<DocumentPart>,
}
impl TryFrom<DocumentRecord> for Document {
    type Error = CorrespondenceError;
    fn try_from(v: DocumentRecord) -> Result<Self, Self::Error> {
        if v.version_secret == [0; 32] || v.parts.is_empty() || v.parts.len() > MAX_PARTS {
            return Err(CorrespondenceError::Invalid);
        }
        let document = Self(v);
        validate_blocks(&document.block_descriptors()?)?;
        Ok(document)
    }
}
impl From<Document> for DocumentRecord {
    fn from(v: Document) -> Self {
        v.0
    }
}
impl Document {
    /// Supply a fresh cryptographically random secret per document. The secret
    /// stays encrypted so public versions cannot fingerprint guessable content.
    /// Preserve the document for exact retries.
    /// # Errors
    /// Rejects zero secrets, empty documents or oversized content/block counts.
    pub fn new(
        parts: Vec<DocumentPart>,
        version_secret: [u8; 32],
    ) -> Result<Self, CorrespondenceError> {
        DocumentRecord {
            version_secret,
            parts,
        }
        .try_into()
    }
    pub fn parts(&self) -> &[DocumentPart] {
        &self.0.parts
    }
    fn blocks(&self) -> Vec<BlockRef<'_>> {
        self.0.parts.iter().flat_map(DocumentPart::blocks).collect()
    }
    fn block_descriptors(&self) -> Result<Vec<BlockDescriptor>, CorrespondenceError> {
        self.blocks()
            .into_iter()
            .map(BlockRef::descriptor)
            .collect()
    }
    /// # Errors
    /// Rejects serialization failure, oversized encoding or invalid message identity.
    pub fn manifest(
        &self,
        message: MessageId,
        conversation: ConversationId,
    ) -> Result<MessageManifest, CorrespondenceError> {
        let bytes = serde_json::to_vec(self).map_err(|_| CorrespondenceError::Invalid)?;
        if bytes.len() > MAX_ENCODED_DOCUMENT_BYTES {
            return Err(CorrespondenceError::Invalid);
        }
        let mut hash = Sha256::new();
        hash.update(b"cs-mail/correspondence-document/v2");
        hash.update(bytes);
        ManifestRecord {
            message,
            conversation,
            version: MessageVersion(hash.finalize().into()),
            blocks: self.block_descriptors()?,
            sources: self
                .0
                .parts
                .iter()
                .filter_map(DocumentPart::source)
                .collect(),
        }
        .try_into()
    }
    /// Resolve a selection locally after decrypting an authorized source message.
    /// The same extraction path supplies quotations and reference rendering.
    /// # Errors
    /// Rejects substituted content, mismatched block types and split UTF-8 characters.
    pub fn select(
        &self,
        manifest: &MessageManifest,
        selection: &MessageSelection,
    ) -> Result<Vec<ContentBlock>, CorrespondenceError> {
        if self.manifest(manifest.message(), manifest.conversation())? != *manifest {
            return Err(CorrespondenceError::Invalid);
        }
        selection.validate_target(manifest)?;
        let blocks = self.blocks();
        selection
            .elements()
            .iter()
            .map(
                |element| match (element, blocks.get(element.block() as usize)) {
                    (SelectionElement::TextRange { start, end, .. }, Some(BlockRef::Text(t))) => t
                        .get(*start as usize..*end as usize)
                        .map(|s| ContentBlock::Text(s.into()))
                        .ok_or(CorrespondenceError::Invalid),
                    (SelectionElement::ImageBlock { .. }, Some(BlockRef::Image(i))) => {
                        Ok(ContentBlock::Image((*i).clone()))
                    }
                    _ => Err(CorrespondenceError::Invalid),
                },
            )
            .collect()
    }
    /// # Errors
    /// Validates the selection and independently retains its text and image bytes.
    pub fn quote(
        &self,
        manifest: &MessageManifest,
        selection: MessageSelection,
    ) -> Result<QuotedExcerpt, CorrespondenceError> {
        let blocks = self.select(manifest, &selection)?;
        ExcerptRecord {
            source: selection,
            blocks,
        }
        .try_into()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ManifestRecord", into = "ManifestRecord")]
pub struct MessageManifest(ManifestRecord);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestRecord {
    message: MessageId,
    conversation: ConversationId,
    version: MessageVersion,
    blocks: Vec<BlockDescriptor>,
    sources: Vec<SourceUse>,
}
impl TryFrom<ManifestRecord> for MessageManifest {
    type Error = CorrespondenceError;
    fn try_from(v: ManifestRecord) -> Result<Self, Self::Error> {
        validate_blocks(&v.blocks)?;
        if v.message.0 == 0
            || v.sources.len() > MAX_PARTS
            || v.sources
                .iter()
                .any(|s| s.selection().message() == v.message)
        {
            return Err(CorrespondenceError::Invalid);
        }
        Ok(Self(v))
    }
}
impl From<MessageManifest> for ManifestRecord {
    fn from(v: MessageManifest) -> Self {
        v.0
    }
}
impl MessageManifest {
    pub const fn message(&self) -> MessageId {
        self.0.message
    }
    pub const fn conversation(&self) -> ConversationId {
        self.0.conversation
    }
    pub const fn version(&self) -> MessageVersion {
        self.0.version
    }
    pub fn blocks(&self) -> &[BlockDescriptor] {
        &self.0.blocks
    }
    pub fn sources(&self) -> &[SourceUse] {
        &self.0.sources
    }
}
