//! Accept the complete supported email content through its single checked owner.
use super::Refusal;
use cs_mail_correspondence::email::EmailContent;
use mail_parser::Message;

pub(super) fn content(message: &Message<'_>) -> Result<EmailContent, Refusal> {
    EmailContent::from_mime(message.raw_message.to_vec()).map_err(|_| Refusal::Unsupported)
}
