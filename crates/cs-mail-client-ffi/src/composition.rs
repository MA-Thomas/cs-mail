//! Presentation bridge for checked authored content; file selection is an Apple adapter concern.
#![allow(clippy::needless_pass_by_value)] // UniFFI exports require owned language-boundary arguments.
use crate::{ClientError, ReadingEmail, core};
use cs_mail_correspondence::{
    ImageContent, ImageFormat,
    composed::{AttachedFile as File, ComposedBody, CompositionFormat},
};

#[derive(Clone, uniffi::Record)]
pub struct ComposerFile {
    pub name: String,
    pub bytes: Vec<u8>,
}
#[derive(Clone, uniffi::Record)]
pub struct Composition {
    pub html: String,
    pub imported: bool,
    pub images: Vec<Vec<u8>>,
    pub files: Vec<ComposerFile>,
}
impl TryFrom<Composition> for ComposedBody {
    type Error = ClientError;
    fn try_from(v: Composition) -> Result<Self, Self::Error> {
        let images = v
            .images
            .into_iter()
            .map(|b| ImageContent::new(ImageFormat::Png, b))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ClientError::InvalidData)?;
        let files = v
            .files
            .into_iter()
            .map(|f| File::new(f.name, f.bytes))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ClientError::InvalidData)?;
        Self::new(
            if v.imported {
                CompositionFormat::Imported
            } else {
                CompositionFormat::Editable
            },
            v.html,
            images,
            files,
        )
        .map_err(|_| ClientError::InvalidData)
    }
}
impl From<&ComposedBody> for Composition {
    fn from(v: &ComposedBody) -> Self {
        Self {
            html: v.html().into(),
            imported: matches!(v.format(), CompositionFormat::Imported),
            images: v.images().iter().map(|i| i.bytes().to_vec()).collect(),
            files: v
                .files()
                .iter()
                .map(|f| ComposerFile {
                    name: f.name().into(),
                    bytes: f.bytes().to_vec(),
                })
                .collect(),
        }
    }
}
/// # Errors
/// Refuses unsupported image destinations and content beyond the message limits.
#[uniffi::export]
pub fn composition_presentation(content: Composition) -> Result<ReadingEmail, ClientError> {
    let body: ComposedBody = content.try_into()?;
    let mut email: core::ReadingEmail = body
        .presentation()
        .map_err(|_| ClientError::InvalidData)?
        .into();
    email.files = body.files().to_vec();
    Ok(email.into())
}
/// # Errors
/// Invalid or oversized authored content.
#[uniffi::export]
pub fn composition_text(content: Composition) -> Result<String, ClientError> {
    let body: ComposedBody = content.try_into()?;
    Ok(body.text().into())
}
#[uniffi::export]
pub fn composer_image_id(bytes: Vec<u8>) -> String {
    cs_mail_correspondence::email::resource_id(&bytes)
}
#[derive(Clone, uniffi::Record)]
pub struct DesignImage {
    pub source: String,
    pub bytes: Vec<u8>,
}
/// # Errors
/// Invalid or oversized design source.
#[uniffi::export]
pub fn design_image_sources(html: String) -> Result<Vec<String>, ClientError> {
    cs_mail_correspondence::composed::image_sources(&html).map_err(|_| ClientError::InvalidData)
}
/// # Errors
/// Missing or unsupported inline images or oversized content.
#[uniffi::export]
pub fn import_design(
    html: String,
    images: Vec<DesignImage>,
    files: Vec<ComposerFile>,
) -> Result<Composition, ClientError> {
    let resources = images
        .into_iter()
        .map(|i| ImageContent::new(ImageFormat::Png, i.bytes).map(|v| (i.source, v)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ClientError::InvalidData)?;
    let files = files
        .into_iter()
        .map(|f| File::new(f.name, f.bytes))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ClientError::InvalidData)?;
    ComposedBody::import(&html, resources, files)
        .map(|c| (&c).into())
        .map_err(|_| ClientError::InvalidData)
}
