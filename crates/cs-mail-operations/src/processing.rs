//! Authenticated inspection and resumption of received commands; never a business override.
use crate::CommandFamily;
use cs_mail_primitives::JournalPosition;
use cs_mail_security::CommandDigest;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProcessingFailure {
    AwaitingQuoteSignature,
    Persistence,
    RequiresIntervention,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProcessingStatus {
    Ready,
    WaitingForQuote { attempts: u64 },
    Retryable { attempts: u64 },
    Intervention { attempts: u64 },
    Completed,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProcessingCommand {
    Inspect {
        limit: u16,
    },
    Resume {
        position: JournalPosition,
        digest: CommandDigest,
    },
}
impl CommandFamily for ProcessingCommand {
    const SIGNING_DOMAIN: &'static str = "cs-mail/processing-command/v1";
    const NAME: &'static str = "processing";
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcessingInspection {
    pub position: JournalPosition,
    pub digest: CommandDigest,
    pub status: ProcessingStatus,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProcessingResponse {
    Pending(Vec<ProcessingInspection>),
    Resumed {
        position: JournalPosition,
        changed: bool,
    },
}
