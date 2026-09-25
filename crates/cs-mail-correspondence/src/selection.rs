use crate::{
    BlockDescriptor, CorrespondenceError, MAX_BLOCKS, MAX_TEXT_BYTES, MessageManifest,
    MessageVersion,
};
use cs_mail_primitives::MessageId;
use serde::{Deserialize, Serialize};

/// Coordinates in the message's ordered, flattened content blocks. A reference
/// occupies a nonselectable block; a quotation contributes its retained blocks.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SelectionElement {
    TextRange { block: u32, start: u32, end: u32 },
    ImageBlock { block: u32 },
}
impl SelectionElement {
    pub const fn block(&self) -> u32 {
        match self {
            Self::TextRange { block, .. } | Self::ImageBlock { block } => *block,
        }
    }
}

/// Nonempty ordered selection bound to exactly one immutable message version.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SelectionRecord", into = "SelectionRecord")]
pub struct MessageSelection(SelectionRecord);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectionRecord {
    message: MessageId,
    version: MessageVersion,
    elements: Vec<SelectionElement>,
}
impl TryFrom<SelectionRecord> for MessageSelection {
    type Error = CorrespondenceError;
    fn try_from(v: SelectionRecord) -> Result<Self, Self::Error> {
        if v.message.0 == 0 || v.elements.is_empty() || v.elements.len() > MAX_BLOCKS {
            return Err(CorrespondenceError::Invalid);
        }
        let mut previous: Option<&SelectionElement> = None;
        for element in &v.elements {
            if element.block() as usize >= MAX_BLOCKS
                || matches!(element,
                SelectionElement::TextRange { start, end, .. } if start >= end || *end as usize > MAX_TEXT_BYTES)
            {
                return Err(CorrespondenceError::Invalid);
            }
            if let Some(prior) = previous {
                let ordered = prior.block() < element.block()
                    || matches!((prior, element),
                    (SelectionElement::TextRange { block: a, end, .. }, SelectionElement::TextRange { block: b, start, .. })
                    if a == b && end <= start);
                if !ordered {
                    return Err(CorrespondenceError::Invalid);
                }
            }
            previous = Some(element);
        }
        Ok(Self(v))
    }
}
impl From<MessageSelection> for SelectionRecord {
    fn from(v: MessageSelection) -> Self {
        v.0
    }
}
impl MessageSelection {
    /// # Errors
    /// Rejects empty, overlapping, duplicate, out-of-order or unbounded elements.
    /// Target types/bounds require the manifest; UTF-8 requires endpoint plaintext.
    pub fn new(
        message: MessageId,
        version: MessageVersion,
        elements: Vec<SelectionElement>,
    ) -> Result<Self, CorrespondenceError> {
        SelectionRecord {
            message,
            version,
            elements,
        }
        .try_into()
    }
    pub const fn message(&self) -> MessageId {
        self.0.message
    }
    pub const fn version(&self) -> MessageVersion {
        self.0.version
    }
    pub fn elements(&self) -> &[SelectionElement] {
        &self.0.elements
    }
    /// # Errors
    /// Rejects substituted versions, missing blocks, type mismatches and invalid bounds.
    pub fn validate_target(&self, manifest: &MessageManifest) -> Result<(), CorrespondenceError> {
        if self.message() != manifest.message() || self.version() != manifest.version() {
            return Err(CorrespondenceError::Invalid);
        }
        for element in self.elements() {
            let valid = match (element, manifest.blocks().get(element.block() as usize)) {
                (
                    SelectionElement::TextRange { end, .. },
                    Some(BlockDescriptor::Text { bytes }),
                ) => end <= bytes,
                (SelectionElement::ImageBlock { .. }, Some(BlockDescriptor::Image { .. })) => true,
                _ => false,
            };
            if !valid {
                return Err(CorrespondenceError::Invalid);
            }
        }
        Ok(())
    }
}
