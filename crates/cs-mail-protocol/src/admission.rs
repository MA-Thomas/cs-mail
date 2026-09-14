//! Admission policy and content checks shared by every delivery route.
use crate::{Message, RelationshipKey};
use cs_mail_capabilities::CapabilityError;
use cs_mail_content::{EncryptedContentRecord, message_declaration_digest};
use cs_mail_primitives::{
    CanonicalTime, ExtensionCriticality, NamespacedIdentifier, ProtocolVersion, Version,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AdmissionPolicy {
    pub version: Version,
    pub supported_critical_schemas: BTreeSet<NamespacedIdentifier>,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AdmissionFailure {
    UnsupportedCriticalExtension,
    InvalidDeclaration,
    ContentExpired,
    MessageValidityClosed,
    ContentScopeMismatch,
    ContentMissing,
    AuthenticationMismatch,
}
impl AdmissionPolicy {
    /// # Errors
    /// Refuses invalid declarations or unsupported critical schemas.
    pub fn evaluate(
        &self,
        declarations: &cs_mail_primitives::MessageDeclarations,
    ) -> Result<(), AdmissionFailure> {
        declarations
            .validate()
            .map_err(|_| AdmissionFailure::InvalidDeclaration)?;
        if declarations.payload_schema.as_ref().is_some_and(|s| {
            s.criticality == ExtensionCriticality::Critical
                && !self.supported_critical_schemas.contains(&s.id)
        }) {
            return Err(AdmissionFailure::UnsupportedCriticalExtension);
        }
        Ok(())
    }
}
/// Authority has already been established at durable receipt. Metadata must agree with it.
#[derive(Clone, Copy, Debug)]
pub struct AdmissionContext {
    pub relationship: RelationshipKey,
    pub protocol: ProtocolVersion,
    pub now: CanonicalTime,
    pub authority: cs_mail_primitives::DeclarationAuthority,
    pub capability: Option<cs_mail_primitives::LaneId>,
}
/// Validates the proposed Stage 1 message against durable ciphertext and current policy.
/// # Errors
/// Rejects mismatched bindings, closed validity, expired content and policy refusal.
pub fn validate_message(
    record: &EncryptedContentRecord,
    message: &Message,
    context: AdmissionContext,
    policy: &AdmissionPolicy,
) -> Result<(), AdmissionFailure> {
    let AdmissionContext {
        relationship,
        protocol,
        now,
        authority,
        capability,
    } = context;
    let binding = &record.binding;
    if binding.declarations.origin.authority != authority || binding.capability != capability {
        return Err(AdmissionFailure::AuthenticationMismatch);
    }
    if record.created_at > now {
        return Err(AdmissionFailure::ContentMissing);
    }
    if record.expires_at <= now {
        return Err(AdmissionFailure::ContentExpired);
    }
    if message.valid_until.0 < now {
        return Err(AdmissionFailure::MessageValidityClosed);
    }
    if binding.message_id != message.id
        || binding.content_ref != message.content
        || binding.relationship != relationship.reference
        || binding.sender != relationship.sender
        || binding.recipient != relationship.recipient
        || binding.protocol_version != protocol
        || binding.message_valid_until != message.valid_until
        || message_declaration_digest(&binding.declarations)
            .map_err(|_| AdmissionFailure::InvalidDeclaration)?
            != message.declaration
    {
        return Err(AdmissionFailure::ContentScopeMismatch);
    }
    policy.evaluate(&binding.declarations)
}

/// The complete consequence of free permission selection, without persistence or I/O.
#[derive(Clone, Debug)]
pub struct FreeAdmissionPlan {
    pub message: Message,
    pub next_lane: Option<cs_mail_capabilities::Lane>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdmissionPlanError {
    Protocol(crate::ProtocolError),
    Policy(AdmissionFailure),
    Capability(cs_mail_capabilities::CapabilityError),
}
/// Plans accepted-relationship or lane delivery. Accepted permission takes precedence
/// over an attached lane, and refusal never invents a charged request.
/// # Errors
/// Refuses block, scope/content/policy mismatches, or missing/exhausted permission.
pub fn plan_free_admission(
    state: &crate::ProtocolState,
    request: &cs_mail_capabilities::BondFreeAdmission,
    record: Option<&EncryptedContentRecord>,
    lane: Option<&cs_mail_capabilities::Lane>,
    now: CanonicalTime,
    policy: &AdmissionPolicy,
) -> Result<FreeAdmissionPlan, AdmissionPlanError> {
    use crate::{AdmissionBasis, ProtocolError, RelationshipState};
    use cs_mail_capabilities::AdmissionAuthentication;
    if state.relationship.state == RelationshipState::Blocked {
        return Err(AdmissionPlanError::Protocol(ProtocolError::ContactBlocked));
    }
    if request.sender != state.relationship.key.sender
        || request.recipient != state.relationship.key.recipient
    {
        return Err(AdmissionPlanError::Capability(
            CapabilityError::NotAuthorized,
        ));
    }
    let record = record.ok_or(AdmissionPlanError::Policy(AdmissionFailure::ContentMissing))?;
    let (basis, candidate_lane) = if state.relationship.state == RelationshipState::Accepted {
        (
            AdmissionBasis::AcceptedRelationship {
                version: state.relationship.version,
            },
            None,
        )
    } else {
        let lane = lane.ok_or(AdmissionPlanError::Capability(
            CapabilityError::NotAuthorized,
        ))?;
        (
            AdmissionBasis::ExpressLane {
                lane: lane.grant.id,
                version: lane.version,
            },
            Some(lane),
        )
    };
    let mut message = Message {
        id: request.message_id,
        relationship: state.relationship.key.reference,
        content: request.content_ref,
        delivery: request.delivery_intent_ref,
        declaration: message_declaration_digest(&request.declarations)
            .map_err(|_| AdmissionPlanError::Policy(AdmissionFailure::InvalidDeclaration))?,
        valid_until: request.message_valid_until,
        admitted_at: now,
        basis,
    };
    let authority = match request.authentication {
        AdmissionAuthentication::NativeKey(key) => {
            cs_mail_primitives::DeclarationAuthority::NativeSender(key)
        }
        AdmissionAuthentication::LegacyDmarc => {
            cs_mail_primitives::DeclarationAuthority::LegacyGateway(request.intended_provider)
        }
    };
    validate_message(
        record,
        &message,
        AdmissionContext {
            relationship: state.relationship.key,
            protocol: request.protocol_version,
            now,
            authority,
            capability: request.capability,
        },
        policy,
    )
    .map_err(AdmissionPlanError::Policy)?;
    let next_lane = if let Some(lane) = candidate_lane {
        let lane = consume_lane(lane, request, now)?;
        message.basis = AdmissionBasis::ExpressLane {
            lane: lane.grant.id,
            version: lane.version,
        };
        Some(lane)
    } else {
        None
    };
    Ok(FreeAdmissionPlan { message, next_lane })
}

fn consume_lane(
    lane: &cs_mail_capabilities::Lane,
    request: &cs_mail_capabilities::BondFreeAdmission,
    now: CanonicalTime,
) -> Result<cs_mail_capabilities::Lane, AdmissionPlanError> {
    let evidence = request
        .evidence
        .as_ref()
        .ok_or(AdmissionPlanError::Capability(
            CapabilityError::NotAuthorized,
        ))?;
    let mut lane = lane.clone();
    lane.authorizes(
        request.sender,
        request.recipient,
        request.capability,
        &request.declarations,
        evidence,
        now,
    )
    .map_err(AdmissionPlanError::Capability)?;
    if lane
        .consume(
            request.message_id,
            request.capability,
            &request.declarations,
            evidence,
            now,
        )
        .map_err(AdmissionPlanError::Capability)?
    {
        return Err(AdmissionPlanError::Protocol(
            crate::ProtocolError::DuplicateConflict,
        ));
    }
    Ok(lane)
}

/// Builds the proposed delivery metadata without changing request/history/financial owners.
/// Missing initial requests are left to the kernel's normal missing-record refusal.
pub fn proposed_message(
    state: &crate::ProtocolState,
    command: &crate::ProtocolCommand,
    now: CanonicalTime,
) -> Option<Message> {
    use crate::{AdmissionBasis, ProtocolCommand};
    let (id, content, delivery, declaration, valid_until, basis) = match *command {
        ProtocolCommand::AdmitRequest {
            request_id,
            content_ref,
            delivery_intent_ref,
            declaration_digest,
            message_valid_until,
            ..
        } => (
            state.requests.get(&request_id)?.initial_message,
            content_ref,
            delivery_intent_ref,
            declaration_digest,
            message_valid_until,
            AdmissionBasis::InitialRequest {
                request: request_id,
            },
        ),
        ProtocolCommand::AdmitFollowup {
            request_id,
            message_id,
            content_ref,
            delivery_intent_ref,
            declaration_digest,
            message_valid_until,
            expected_policy_version,
        } => (
            message_id,
            content_ref,
            delivery_intent_ref,
            declaration_digest,
            message_valid_until,
            AdmissionBasis::RequestFollowup {
                request: request_id,
                policy_version: expected_policy_version,
            },
        ),
        _ => return None,
    };
    Some(Message {
        id,
        relationship: state.relationship.key.reference,
        content,
        delivery,
        declaration,
        valid_until,
        admitted_at: now,
        basis,
    })
}
