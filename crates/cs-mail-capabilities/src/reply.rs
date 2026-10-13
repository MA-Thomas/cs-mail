//! Operator-defined reply terms and the distinct authority of recorded acceptance.
//! These records are facts from protected persistence, not proof of current authority.
use super::{
    CapabilityError, DeclarationAuthorityConstraint, Lane, LaneGrant, LaneId, LaneMode,
    LaneSubject, RateLimit,
};
use cs_mail_primitives::{
    CanonicalTime, DeclaredPurpose, Duration, OperationalKeyRef, OriginMode, ProtocolIdentity,
    ProtocolVersion, ProviderRef, Version,
};
use cs_mail_primitives::{EventRef, RelationshipRef, RequestId};
use cs_mail_privacy::ScopedHandle;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(try_from = "ReplyTermsRecord", into = "ReplyTermsRecord")]
pub struct ReplyLaneTerms(ReplyTermsRecord);
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplyTermsRecord {
    pub lifetime: Duration,
    pub rate_limit: RateLimit,
    pub purpose: DeclaredPurpose,
}
impl TryFrom<ReplyTermsRecord> for ReplyLaneTerms {
    type Error = CapabilityError;
    fn try_from(record: ReplyTermsRecord) -> Result<Self, Self::Error> {
        if record.lifetime.0 == 0 || !record.rate_limit.valid() {
            return Err(CapabilityError::InvalidGrant);
        }
        record
            .purpose
            .validate()
            .map_err(|_| CapabilityError::InvalidDeclaration)?;
        Ok(Self(record))
    }
}
impl From<ReplyLaneTerms> for ReplyTermsRecord {
    fn from(value: ReplyLaneTerms) -> Self {
        value.0
    }
}
impl ReplyLaneTerms {
    /// # Errors
    /// Rejects empty lifetimes, invalid allowance or malformed purpose.
    pub fn new(record: ReplyTermsRecord) -> Result<Self, CapabilityError> {
        record.try_into()
    }
    pub const fn lifetime(&self) -> Duration {
        self.0.lifetime
    }
    pub const fn rate_limit(&self) -> RateLimit {
        self.0.rate_limit
    }
    pub fn purpose(&self) -> &DeclaredPurpose {
        &self.0.purpose
    }
    /// Derives a valid lane from recorded agreement and acceptance. The application
    /// must establish these facts against protected request/authority state before commit.
    /// No recipient signature is invented and this is not a raw-grant activation API.
    /// # Errors
    /// Rejects foreign/empty identifiers, invalid intervals or overflow.
    pub fn accepted_lane(&self, acceptance: &ReplyAcceptance) -> Result<Lane, CapabilityError> {
        if acceptance.request.0 == 0
            || acceptance.creation_key.0 == 0
            || acceptance.creation_event.0.0 == 0
            || acceptance.acceptance_event.0.0 == 0
        {
            return Err(CapabilityError::InvalidGrant);
        }
        let mut hash = Sha256::new();
        hash.update(b"cs-mail/accepted-reply-lane/v1");
        hash.update(acceptance.deployment);
        hash.update(acceptance.originating_relationship.as_bytes());
        hash.update(acceptance.request.0.to_be_bytes());
        let digest = hash.finalize();
        let mut id = [0; 16];
        id.copy_from_slice(&digest[..16]);
        let grant = LaneGrant {
            id: LaneId(u128::from_be_bytes(id) | 1),
            subject: LaneSubject::Native(ScopedHandle(*acceptance.reverse_relationship.as_bytes())),
            sender: acceptance.recipient,
            recipient: acceptance.sender,
            protocol_version: acceptance.protocol_version,
            deployment_domain: acceptance.deployment,
            intended_provider: acceptance.provider,
            recipient_operational_key: acceptance.creation_key,
            purpose: self.0.purpose.clone(),
            origin: Some(OriginMode::HumanInitiated),
            declaration_authority: DeclarationAuthorityConstraint::NativeSender,
            lifetime: self.0.lifetime,
            rate_limit: self.0.rate_limit,
            mode: LaneMode::Expiring,
            issued_at: acceptance.at,
            not_before: acceptance.at,
            not_after: acceptance
                .at
                .checked_add(self.0.lifetime)
                .ok_or(CapabilityError::ArithmeticOverflow)?,
            version: acceptance.version,
        };
        let mut lane = Lane::from_grant(grant)?;
        lane.authority = LaneAuthority::RequestAcceptance {
            request: acceptance.request,
            relationship: acceptance.originating_relationship,
            creation: acceptance.creation_event,
            acceptance: acceptance.acceptance_event,
        };
        Ok(lane)
    }
}
/// Facts selected by the request/application owners, never a public command DTO.
pub struct ReplyAcceptance {
    pub request: RequestId,
    pub originating_relationship: RelationshipRef,
    pub reverse_relationship: RelationshipRef,
    pub sender: ProtocolIdentity,
    pub recipient: ProtocolIdentity,
    pub creation_key: OperationalKeyRef,
    pub creation_event: EventRef,
    pub acceptance_event: EventRef,
    pub at: CanonicalTime,
    pub version: Version,
    pub deployment: [u8; 32],
    pub protocol_version: ProtocolVersion,
    pub provider: ProviderRef,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LaneAuthority {
    RecipientGrant,
    RequestAcceptance {
        request: RequestId,
        relationship: RelationshipRef,
        creation: EventRef,
        acceptance: EventRef,
    },
}
