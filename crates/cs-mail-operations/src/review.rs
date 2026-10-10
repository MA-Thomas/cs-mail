//! Operator commands over enrollment review: enrollments the identity service holds
//! because the applicant's evidence overlaps another enrollment's.
use crate::CommandFamily;
use identity_contract::evidence::{ReviewCaseId, ReviewResolution};
use serde::{Deserialize, Serialize};

/// Operator commands over enrollment review, signed like other operator commands.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReviewCommand {
    /// The open cases, oldest first.
    Open,
    /// Approve or deny one case. Repeating the same resolution returns the recorded case.
    Resolve {
        case: ReviewCaseId,
        resolution: ReviewResolution,
    },
}
impl CommandFamily for ReviewCommand {
    const SIGNING_DOMAIN: &'static str = "cs-mail/review-command/v1";
    const NAME: &'static str = "review";
}
