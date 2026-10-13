//! Recipient-owned relationship controls. Portable orchestration signs and journals;
//! the server's relationship owner decides permission and settlement atomically.
use super::{
    Error, Inbox, MemberClient, Pending, PendingCommand, lane_grant, parse_sender, storage,
};
use cs_mail_primitives::IdempotencyKey;
use cs_mail_protocol::{ActorRef, FollowupPolicy, ProtocolCommand};
use cs_mail_security::{CommandSigner, SigningScope};

#[derive(Clone, Copy, Debug)]
pub enum RelationshipControl {
    GrantStanding,
    GrantLane {
        lifetime_ms: u64,
    },
    Revoke,
    Block,
    Unblock,
    Followups {
        max_messages: u32,
        max_per_interval: u32,
        interval_ms: u64,
    },
}
impl MemberClient {
    /// # Errors
    /// Refuses replacing unresolved work, absent contacts, unavailable custody or
    /// a canonical refusal. Exact retries use the common permission journal.
    pub fn control_relationship(
        &self,
        sender: &str,
        control: RelationshipControl,
    ) -> Result<Inbox, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        if state.sending.is_some() {
            return Err(Error::SendPending);
        }
        if state.pending.is_some() {
            return Err(Error::DecisionPending);
        }
        if state.setup.is_some() {
            return Err(Error::SetupPending);
        }
        super::require_idle(&state)?;
        self.synchronize(&mut state, &keys)?;
        let sender = parse_sender(sender)?;
        let view = state
            .inbox
            .iter()
            .find(|view| view.handle.sender == sender)
            .ok_or(Error::InvalidData)?;
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(|_| Error::Unavailable)?;
        let operation = IdempotencyKey(u128::from_be_bytes(random) | 1);
        let expected_version = view.version.into();
        let (route, command) = if let RelationshipControl::GrantLane { lifetime_ms } = control {
            (
                "v1/lanes".into(),
                lane_grant(&state, &keys, view, lifetime_ms, (self.clock)(), operation)?,
            )
        } else {
            let command = match control {
                RelationshipControl::GrantStanding => {
                    ProtocolCommand::AcceptRelationship { expected_version }
                }
                RelationshipControl::Revoke => {
                    ProtocolCommand::RevokeRelationship { expected_version }
                }
                RelationshipControl::Block => {
                    ProtocolCommand::BlockRelationship { expected_version }
                }
                RelationshipControl::Unblock => {
                    ProtocolCommand::UnblockRelationship { expected_version }
                }
                RelationshipControl::Followups {
                    max_messages,
                    max_per_interval,
                    interval_ms,
                } => ProtocolCommand::SetFollowupPolicy {
                    expected_version: view.followup_policy.version,
                    policy: FollowupPolicy {
                        version: view.followup_policy.version,
                        max_messages,
                        max_per_interval,
                        interval: cs_mail_primitives::Duration(interval_ms),
                    },
                },
                RelationshipControl::GrantLane { .. } => unreachable!(),
            };
            let signed = CommandSigner::from_secret_bytes(
                ActorRef::Recipient(state.profile.persona),
                state.profile.recipient_key,
                &keys.recipient,
            )
            .sign(
                SigningScope {
                    deployment_domain: state.profile.deployment.scope.deployment_domain,
                    intended_provider: state.profile.deployment.scope.provider,
                    relationship: view.handle.relationship,
                },
                state.profile.deployment.policy.protocol_version,
                operation,
                command,
            )
            .map_err(|_| Error::InvalidData)?;
            (
                format!(
                    "v1/relationships/{}/{}/commands",
                    sender.0, state.profile.persona.0
                ),
                PendingCommand::Relationship(signed),
            )
        };
        state.pending = Some(Pending {
            sender,
            route,
            command,
            relationship: view.handle.relationship,
        });
        state.last_decision = None;
        storage::save(self.store.as_ref(), &keys, &state)?;
        self.send_pending(&mut state, &keys)?;
        super::require_idle(&state)?;
        self.synchronize(&mut state, &keys)?;
        super::views::inbox(&state)
    }
}
