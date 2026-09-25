use crate::{CorrespondenceError, MAX_IMAGE_BYTES};
use serde::{Deserialize, Serialize};

/// Declared encoding, not evidence that an image decoder has accepted the bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ImageFormat {
    Png,
    Jpeg,
    WebP,
}

/// Independently retained image bytes inside endpoint-encrypted content.
/// External URLs are text; no remote resource is fetched implicitly.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ImageRecord", into = "ImageRecord")]
pub struct ImageContent(ImageRecord);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImageRecord {
    format: ImageFormat,
    bytes: Vec<u8>,
}
impl TryFrom<ImageRecord> for ImageContent {
    type Error = CorrespondenceError;
    fn try_from(v: ImageRecord) -> Result<Self, Self::Error> {
        if v.bytes.is_empty() || v.bytes.len() > MAX_IMAGE_BYTES {
            return Err(CorrespondenceError::Invalid);
        }
        Ok(Self(v))
    }
}
impl From<ImageContent> for ImageRecord {
    fn from(v: ImageContent) -> Self {
        v.0
    }
}
impl ImageContent {
    /// # Errors
    /// Rejects empty or oversized payloads. Image decoding belongs to the endpoint.
    pub fn new(format: ImageFormat, bytes: Vec<u8>) -> Result<Self, CorrespondenceError> {
        ImageRecord { format, bytes }.try_into()
    }
    pub const fn format(&self) -> ImageFormat {
        self.0.format
    }
    pub fn bytes(&self) -> &[u8] {
        &self.0.bytes
    }
}
