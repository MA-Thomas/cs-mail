//! A shared subject is immutable encrypted content. Only its salted commitment is public.
use crate::CorrespondenceError;
use cs_mail_primitives::CanonicalTime;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_SUBJECT_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "[u8; 32]", into = "[u8; 32]")]
pub struct SubjectCommitment([u8; 32]);
impl TryFrom<[u8; 32]> for SubjectCommitment {
    type Error = CorrespondenceError;
    fn try_from(value: [u8; 32]) -> Result<Self, Self::Error> {
        if value == [0; 32] {
            return Err(CorrespondenceError::Invalid);
        }
        Ok(Self(value))
    }
}
impl From<SubjectCommitment> for [u8; 32] {
    fn from(value: SubjectCommitment) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "SubjectRecord", into = "SubjectRecord")]
pub struct ConversationSubject(SubjectRecord);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubjectRecord {
    text: String,
    secret: [u8; 32],
}
impl TryFrom<SubjectRecord> for ConversationSubject {
    type Error = CorrespondenceError;
    fn try_from(value: SubjectRecord) -> Result<Self, Self::Error> {
        if value.secret == [0; 32]
            || value.text.trim().is_empty()
            || value.text.len() > MAX_SUBJECT_BYTES
            || value.text.chars().any(char::is_control)
        {
            return Err(CorrespondenceError::Invalid);
        }
        Ok(Self(value))
    }
}
impl From<ConversationSubject> for SubjectRecord {
    fn from(value: ConversationSubject) -> Self {
        value.0
    }
}
impl ConversationSubject {
    /// # Errors
    /// Rejects invalid text, entropy, date or explicit local UTC offset.
    pub fn new(
        text: &str,
        at: CanonicalTime,
        offset_seconds: i32,
        secret: [u8; 32],
    ) -> Result<Self, CorrespondenceError> {
        let text = if text.trim().is_empty() {
            let offset = time::UtcOffset::from_whole_seconds(offset_seconds)
                .map_err(|_| CorrespondenceError::Invalid)?;
            let date =
                time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(at.0) * 1_000_000)
                    .map_err(|_| CorrespondenceError::Invalid)?
                    .to_offset(offset);
            let hour = match date.hour() % 12 {
                0 => 12,
                n => n,
            };
            format!(
                "Default subject: {}-{}-{}, {}:{:02} {}",
                date.month(),
                date.day(),
                date.year(),
                hour,
                date.minute(),
                if date.hour() < 12 { "AM" } else { "PM" }
            )
        } else {
            text.trim().to_owned()
        };
        SubjectRecord { text, secret }.try_into()
    }
    pub fn text(&self) -> &str {
        &self.0.text
    }
    pub fn commitment(&self) -> SubjectCommitment {
        let mut hash = Sha256::new();
        hash.update(b"cs-mail/conversation-subject/v1");
        hash.update(self.0.secret);
        hash.update(self.0.text.as_bytes());
        SubjectCommitment(hash.finalize().into())
    }
}
