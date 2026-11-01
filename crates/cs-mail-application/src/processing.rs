//! Operational processing progress is separate from a committed business outcome.
use cs_mail_operations::{
    CommandReplay, OperationsError, OperatorAuthority, ProcessingCommand, ProcessingInspection,
    ProcessingResponse, SignedOperatorCommand, VerifiedOperatorCommand, replay,
};
pub use cs_mail_operations::{ProcessingFailure, ProcessingStatus};
use cs_mail_primitives::CanonicalTime;
use cs_mail_security::CommandDigest;

/// Protected receipt facts supplied by the persistence adapter.
pub struct ProcessingTarget {
    pub digest: CommandDigest,
    pub status: ProcessingStatus,
}
impl ProcessingTarget {
    /// Resume preserves the original command, authority, time and policy.
    /// # Errors
    /// Refuses a substituted command or an already completed operation.
    pub fn resume(
        &self,
        expected: CommandDigest,
    ) -> Result<bool, cs_mail_operations::OperationsError> {
        if self.digest != expected || self.status == ProcessingStatus::Completed {
            return Err(cs_mail_operations::OperationsError::Invalid);
        }
        Ok(self.status != ProcessingStatus::Ready)
    }
}

pub enum ProcessingInput {
    Pending(Vec<ProcessingInspection>),
    Command(ProcessingTarget),
}
pub struct ProcessingContext {
    pub authority: OperatorAuthority,
    pub prior: Option<([u8; 32], ProcessingResponse)>,
    pub input: ProcessingInput,
}
pub enum ProcessingDecision {
    Observed(ProcessingResponse),
    Resumed {
        command: VerifiedOperatorCommand<ProcessingCommand>,
        at: CanonicalTime,
        response: ProcessingResponse,
    },
}
pub trait ProcessingStore {
    type Error: From<OperationsError>;
    /// Protect the original receipt and operator authority through audit commit.
    /// # Errors
    /// Roll back every effect on authentication, concurrency or persistence failure.
    fn transact_processing(
        &self,
        signed: &SignedOperatorCommand<ProcessingCommand>,
        decide: impl FnOnce(ProcessingContext) -> Result<ProcessingDecision, Self::Error>,
    ) -> Result<ProcessingResponse, Self::Error>;
}
/// # Errors
/// Rejects foreign operators, changed retries, digest substitution and completed commands.
pub fn manage_processing<R: ProcessingStore>(
    store: &R,
    signed: &SignedOperatorCommand<ProcessingCommand>,
    clock: impl FnOnce() -> CanonicalTime,
) -> Result<ProcessingResponse, R::Error> {
    store.transact_processing(signed, |context| {
        let command = context.authority.verify(signed)?;
        if let CommandReplay::Replay(outcome) =
            replay(&command, context.prior.as_ref().map(|(d, o)| (d, o)))?
        {
            return Ok(ProcessingDecision::Observed(outcome));
        }
        match (command.command(), context.input) {
            (ProcessingCommand::Inspect { limit }, ProcessingInput::Pending(items))
                if (1..=100).contains(limit) =>
            {
                Ok(ProcessingDecision::Observed(ProcessingResponse::Pending(
                    items,
                )))
            }
            (ProcessingCommand::Resume { position, digest }, ProcessingInput::Command(target)) => {
                let response = ProcessingResponse::Resumed {
                    position: *position,
                    changed: target.resume(*digest)?,
                };
                Ok(ProcessingDecision::Resumed {
                    command,
                    at: clock(),
                    response,
                })
            }
            _ => Err(OperationsError::Invalid.into()),
        }
    })
}

/// Conservative consistency owners. Persona and account scope are distinct meanings.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProcessingOwner {
    Persona(cs_mail_primitives::ProtocolIdentity),
    Account(cs_mail_primitives::AccountId),
}
pub struct ParticipantOwner {
    pub persona: cs_mail_primitives::ProtocolIdentity,
    pub account: Option<cs_mail_primitives::AccountId>,
}
/// Both directions share participant owners; all aliases of a product account share its fence.
/// # Errors
/// Missing participant or account identity cannot establish an independent ordering scope.
pub fn ordering_owners(
    participants: &[ParticipantOwner],
) -> Result<std::collections::BTreeSet<ProcessingOwner>, OperationsError> {
    if participants.is_empty()
        || participants
            .iter()
            .any(|p| p.persona.0 == 0 || p.account.is_some_and(|id| id.0 == 0))
    {
        return Err(OperationsError::Invalid);
    }
    Ok(participants
        .iter()
        .flat_map(|p| {
            std::iter::once(ProcessingOwner::Persona(p.persona))
                .chain(p.account.map(ProcessingOwner::Account))
        })
        .collect())
}
