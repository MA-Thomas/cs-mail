//! Recipient-granted, moneyless communication capabilities.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use cs_mail_adapters::{DomainIdentity, LegacyDmarcEvidence};
use cs_mail_primitives::{
    CanonicalTime, ContentRef, DeclarationAuthority, DeclaredPurpose, DeliveryIntentRef, Duration,
    IdempotencyKey, LaneId, MessageDeclarations, MessageId, MessageValidityUntil,
    OperationalKeyRef, OriginMode, PayloadSchema, ProtocolIdentity, ProtocolVersion, ProviderRef,
    ScheduleChange, ScheduleTask, Version, WireVersion,
};
use cs_mail_privacy::ScopedHandle;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
pub mod reply;

const SIGNING_DOMAIN: &[u8] = b"cs-mail/lane-grant/v2";
const CONTROL_SIGNING_DOMAIN: &[u8] = b"cs-mail/lane-control/v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LaneSubject {
    Native(ScopedHandle),
    LegacyDomain(DomainIdentity),
    /// Exact mailbox identity, checked against the deployment sender-subject registry.
    GuestMailbox(ProtocolIdentity),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LaneMode {
    Expiring,
    Persistent,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DeclarationAuthorityConstraint {
    NativeSender,
    LegacyGateway,
    GuestGateway,
}

impl DeclarationAuthorityConstraint {
    const fn matches(self, authority: DeclarationAuthority) -> bool {
        matches!(
            (self, authority),
            (Self::NativeSender, DeclarationAuthority::NativeSender(_))
                | (Self::LegacyGateway, DeclarationAuthority::LegacyGateway(_))
                | (
                    Self::GuestGateway,
                    DeclarationAuthority::GuestGateway(_) | DeclarationAuthority::GuestSmtp(_)
                )
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct RateLimit {
    pub max_messages: u32,
    pub interval: Duration,
}

impl RateLimit {
    const fn valid(self) -> bool {
        self.max_messages > 0 && self.interval.0 > 0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LaneGrant {
    pub id: LaneId,
    pub subject: LaneSubject,
    /// The protocol identity used by relationship state and block ordering.
    pub sender: ProtocolIdentity,
    pub recipient: ProtocolIdentity,
    pub protocol_version: ProtocolVersion,
    pub deployment_domain: [u8; 32],
    pub intended_provider: ProviderRef,
    pub recipient_operational_key: OperationalKeyRef,
    pub purpose: DeclaredPurpose,
    pub origin: Option<OriginMode>,
    pub declaration_authority: DeclarationAuthorityConstraint,
    pub lifetime: Duration,
    pub rate_limit: RateLimit,
    pub mode: LaneMode,
    pub issued_at: CanonicalTime,
    pub not_before: CanonicalTime,
    pub not_after: CanonicalTime,
    pub version: Version,
}

impl LaneGrant {
    fn validate(&self) -> Result<(), CapabilityError> {
        self.purpose
            .validate()
            .map_err(|_| CapabilityError::InvalidDeclaration)?;
        let authority_matches_subject = matches!(
            (&self.subject, self.declaration_authority),
            (
                LaneSubject::Native(_),
                DeclarationAuthorityConstraint::NativeSender
            ) | (
                LaneSubject::LegacyDomain(_),
                DeclarationAuthorityConstraint::LegacyGateway
            ) | (
                LaneSubject::GuestMailbox(_),
                DeclarationAuthorityConstraint::GuestGateway
            )
        );
        if self.id.0 == 0
            || self.sender.0 == 0
            || self.recipient.0 == 0
            || self.intended_provider.0 == 0
            || self.recipient_operational_key.0 == 0
            || matches!(self.subject, LaneSubject::GuestMailbox(p) if p.0 == 0 || p != self.sender)
            || self.lifetime.0 == 0
            || self.protocol_version.0 == 0
            || !self.rate_limit.valid()
            || self.not_before < self.issued_at
            || self.not_after <= self.not_before
            || !authority_matches_subject
            || (self.declaration_authority == DeclarationAuthorityConstraint::LegacyGateway
                && self
                    .origin
                    .is_some_and(|origin| origin != OriginMode::LegacyOrUnspecified))
        {
            return Err(CapabilityError::InvalidGrant);
        }
        Ok(())
    }

    /// Returns the domain-separated canonical grant representation.
    ///
    /// # Errors
    ///
    /// Returns an error when the grant is structurally invalid.
    pub fn signing_bytes(&self) -> Result<Vec<u8>, CapabilityError> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(256);
        bytes.extend_from_slice(SIGNING_DOMAIN);
        bytes.extend_from_slice(&self.id.0.to_be_bytes());
        match &self.subject {
            LaneSubject::GuestMailbox(identity) => {
                bytes.push(2);
                bytes.extend_from_slice(&identity.0.to_be_bytes());
            }
            LaneSubject::Native(handle) => {
                bytes.push(0);
                bytes.extend_from_slice(&handle.0);
            }
            LaneSubject::LegacyDomain(domain) => {
                bytes.push(1);
                push_bytes(&mut bytes, domain.as_str().as_bytes())?;
            }
        }
        bytes.extend_from_slice(&self.sender.0.to_be_bytes());
        bytes.extend_from_slice(&self.recipient.0.to_be_bytes());
        bytes.extend_from_slice(&self.protocol_version.0.to_be_bytes());
        bytes.extend_from_slice(&self.deployment_domain);
        bytes.extend_from_slice(&self.intended_provider.0.to_be_bytes());
        bytes.extend_from_slice(&self.recipient_operational_key.0.to_be_bytes());
        self.purpose
            .append_canonical(&mut bytes)
            .map_err(|_| CapabilityError::InvalidDeclaration)?;
        match self.origin {
            Some(origin) => {
                bytes.push(1);
                bytes.push(origin.code());
            }
            None => bytes.push(0),
        }
        bytes.push(match self.declaration_authority {
            DeclarationAuthorityConstraint::NativeSender => 0,
            DeclarationAuthorityConstraint::LegacyGateway => 1,
            DeclarationAuthorityConstraint::GuestGateway => 2,
        });
        bytes.extend_from_slice(&self.lifetime.0.to_be_bytes());
        bytes.extend_from_slice(&self.rate_limit.max_messages.to_be_bytes());
        bytes.extend_from_slice(&self.rate_limit.interval.0.to_be_bytes());
        bytes.push(match self.mode {
            LaneMode::Expiring => 0,
            LaneMode::Persistent => 1,
        });
        bytes.extend_from_slice(&self.issued_at.0.to_be_bytes());
        bytes.extend_from_slice(&self.not_before.0.to_be_bytes());
        bytes.extend_from_slice(&self.not_after.0.to_be_bytes());
        bytes.extend_from_slice(&self.version.0.to_be_bytes());
        Ok(bytes)
    }
}

fn push_bytes(target: &mut Vec<u8>, bytes: &[u8]) -> Result<(), CapabilityError> {
    let length = u32::try_from(bytes.len()).map_err(|_| CapabilityError::InvalidGrant)?;
    target.extend_from_slice(&length.to_be_bytes());
    target.extend_from_slice(bytes);
    Ok(())
}

/// Signature-verified grant contents. The caller obtains the recipient key from
/// protected authority; this proof cannot be deserialized or built from a raw grant.
#[derive(Clone, Debug)]
pub struct VerifiedLaneGrant(LaneGrant);
impl VerifiedLaneGrant {
    /// # Errors
    /// Rejects invalid contents or recipient signatures.
    pub fn verify(signed: &SignedLaneGrant, key: &[u8; 32]) -> Result<Self, CapabilityError> {
        signed.verify(key)?;
        Ok(Self(signed.grant.clone()))
    }
    /// # Errors
    /// Rejects overflow when deriving the lane horizon.
    pub fn activate(self) -> Result<Lane, CapabilityError> {
        Lane::from_grant(self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct SignedLaneGrant {
    pub grant: LaneGrant,
    #[serde(with = "cs_mail_primitives::signature_bytes")]
    pub signature: [u8; 64],
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LaneControlAction {
    Reconfirm,
    Revoke,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LaneControl {
    pub lane_id: LaneId,
    pub recipient: ProtocolIdentity,
    pub action: LaneControlAction,
    pub expected_version: Version,
    pub idempotency_key: IdempotencyKey,
    pub protocol_version: ProtocolVersion,
    pub deployment_domain: [u8; 32],
    pub intended_provider: ProviderRef,
    pub recipient_operational_key: OperationalKeyRef,
}

impl LaneControl {
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(160);
        bytes.extend_from_slice(CONTROL_SIGNING_DOMAIN);
        bytes.extend_from_slice(&self.lane_id.0.to_be_bytes());
        bytes.extend_from_slice(&self.recipient.0.to_be_bytes());
        bytes.push(match self.action {
            LaneControlAction::Reconfirm => 0,
            LaneControlAction::Revoke => 1,
        });
        bytes.extend_from_slice(&self.expected_version.0.to_be_bytes());
        bytes.extend_from_slice(&self.idempotency_key.0.to_be_bytes());
        bytes.extend_from_slice(&self.protocol_version.0.to_be_bytes());
        bytes.extend_from_slice(&self.deployment_domain);
        bytes.extend_from_slice(&self.intended_provider.0.to_be_bytes());
        bytes.extend_from_slice(&self.recipient_operational_key.0.to_be_bytes());
        bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedLaneControl {
    pub control: LaneControl,
    pub signature: [u8; 64],
}

impl SignedLaneControl {
    /// Verifies a recipient lane-control signature.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed keys or invalid signatures.
    pub fn verify(&self, recipient_key: &[u8; 32]) -> Result<(), CapabilityError> {
        let key = VerifyingKey::from_bytes(recipient_key)
            .map_err(|_| CapabilityError::InvalidPublicKey)?;
        key.verify(
            &self.control.signing_bytes(),
            &Signature::from_bytes(&self.signature),
        )
        .map_err(|_| CapabilityError::InvalidSignature)
    }
}

impl SignedLaneGrant {
    /// Verifies that the recipient authorized the exact grant.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed keys, invalid grants, or bad signatures.
    pub fn verify(&self, recipient_key: &[u8; 32]) -> Result<(), CapabilityError> {
        let key = VerifyingKey::from_bytes(recipient_key)
            .map_err(|_| CapabilityError::InvalidPublicKey)?;
        key.verify(
            &self.grant.signing_bytes()?,
            &Signature::from_bytes(&self.signature),
        )
        .map_err(|_| CapabilityError::InvalidSignature)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LaneState {
    Active,
    AwaitingReconfirmation,
    Expired,
    Revoked,
}

/// Complete, revision-bound consequence of ending an existing lane.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "LaneRevocationRecord", into = "LaneRevocationRecord")]
pub struct LaneRevocation {
    expected_version: Version,
    lane: Lane,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LaneRevocationRecord {
    expected_version: Version,
    lane: Lane,
}
impl TryFrom<LaneRevocationRecord> for LaneRevocation {
    type Error = CapabilityError;
    fn try_from(record: LaneRevocationRecord) -> Result<Self, Self::Error> {
        if record.lane.state() != LaneState::Revoked
            || record.expected_version.checked_next() != Some(record.lane.version())
        {
            return Err(CapabilityError::InvalidGrant);
        }
        Ok(Self {
            expected_version: record.expected_version,
            lane: record.lane,
        })
    }
}
impl From<LaneRevocation> for LaneRevocationRecord {
    fn from(effect: LaneRevocation) -> Self {
        Self {
            expected_version: effect.expected_version,
            lane: effect.lane,
        }
    }
}
impl LaneRevocation {
    pub const fn expected_version(&self) -> Version {
        self.expected_version
    }
    pub const fn lane(&self) -> &Lane {
        &self.lane
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "LaneRecord", into = "LaneRecord")]
pub struct Lane {
    authority: reply::LaneAuthority,
    grant: LaneGrant,
    state: LaneState,
    last_activity: CanonicalTime,
    horizon_at: CanonicalTime,
    rate_window_started_at: CanonicalTime,
    messages_in_rate_window: u32,
    version: Version,
    admitted_messages: BTreeSet<MessageId>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LaneRecord {
    authority: reply::LaneAuthority,
    grant: LaneGrant,
    state: LaneState,
    last_activity: CanonicalTime,
    horizon_at: CanonicalTime,
    rate_window_started_at: CanonicalTime,
    messages_in_rate_window: u32,
    version: Version,
    admitted_messages: BTreeSet<MessageId>,
}
impl TryFrom<LaneRecord> for Lane {
    type Error = CapabilityError;
    fn try_from(r: LaneRecord) -> Result<Self, Self::Error> {
        r.grant.validate()?;
        if matches!(r.authority, reply::LaneAuthority::RequestAcceptance {request, creation, acceptance, ..}
            if request.0 == 0 || creation.0.0 == 0 || acceptance.0.0 == 0)
        {
            return Err(CapabilityError::InvalidGrant);
        }
        let activity_horizon = r
            .last_activity
            .checked_add(r.grant.lifetime)
            .ok_or(CapabilityError::ArithmeticOverflow)?
            .min(r.grant.not_after);
        let admitted =
            u64::try_from(r.admitted_messages.len()).map_err(|_| CapabilityError::InvalidGrant)?;
        if r.last_activity < r.grant.issued_at
            || r.last_activity >= r.grant.not_after
            || r.rate_window_started_at < r.grant.issued_at
            || r.rate_window_started_at > r.last_activity
            || r.rate_window_started_at
                .checked_add(r.grant.rate_limit.interval)
                .is_none()
            || r.horizon_at < activity_horizon
            || r.horizon_at > r.grant.not_after
            || (r.grant.mode == LaneMode::Expiring && r.horizon_at != activity_horizon)
            || (r.state == LaneState::AwaitingReconfirmation && r.grant.mode != LaneMode::Expiring)
            || (r.state != LaneState::Active && r.version.0 == 0)
            || r.messages_in_rate_window > r.grant.rate_limit.max_messages
            || u64::from(r.messages_in_rate_window) > admitted
            || (admitted > 0 && r.messages_in_rate_window == 0)
            || r.version.0 < admitted
            || r.admitted_messages.iter().any(|m| m.0 == 0)
        {
            return Err(CapabilityError::InvalidGrant);
        }
        Ok(Self {
            authority: r.authority,
            grant: r.grant,
            state: r.state,
            last_activity: r.last_activity,
            horizon_at: r.horizon_at,
            rate_window_started_at: r.rate_window_started_at,
            messages_in_rate_window: r.messages_in_rate_window,
            version: r.version,
            admitted_messages: r.admitted_messages,
        })
    }
}
impl From<Lane> for LaneRecord {
    fn from(l: Lane) -> Self {
        Self {
            authority: l.authority,
            grant: l.grant,
            state: l.state,
            last_activity: l.last_activity,
            horizon_at: l.horizon_at,
            rate_window_started_at: l.rate_window_started_at,
            messages_in_rate_window: l.messages_in_rate_window,
            version: l.version,
            admitted_messages: l.admitted_messages,
        }
    }
}

impl Lane {
    fn from_grant(grant: LaneGrant) -> Result<Self, CapabilityError> {
        grant.validate()?;
        let horizon_at = grant
            .issued_at
            .checked_add(grant.lifetime)
            .ok_or(CapabilityError::ArithmeticOverflow)?
            .min(grant.not_after);
        grant
            .issued_at
            .checked_add(grant.rate_limit.interval)
            .ok_or(CapabilityError::ArithmeticOverflow)?;
        Ok(Self {
            authority: reply::LaneAuthority::RecipientGrant,
            rate_window_started_at: grant.issued_at,
            last_activity: grant.issued_at,
            grant,
            state: LaneState::Active,
            horizon_at,
            messages_in_rate_window: 0,
            version: Version::default(),
            admitted_messages: BTreeSet::new(),
        })
    }

    pub const fn grant(&self) -> &LaneGrant {
        &self.grant
    }
    pub const fn authority(&self) -> &reply::LaneAuthority {
        &self.authority
    }
    pub const fn state(&self) -> LaneState {
        self.state
    }
    pub const fn version(&self) -> Version {
        self.version
    }
    pub const fn horizon_at(&self) -> CanonicalTime {
        self.horizon_at
    }
    pub const fn messages_in_rate_window(&self) -> u32 {
        self.messages_in_rate_window
    }

    pub fn schedule_change(&self) -> ScheduleChange {
        ScheduleChange::Schedule {
            task: ScheduleTask::LaneHorizon(self.grant.id),
            at: self.horizon_at,
        }
    }

    /// Established access, including a scheduled start; admission checks `not_before`.
    pub fn provides_relationship_access(&self, now: CanonicalTime) -> bool {
        self.state == LaneState::Active
            && now >= self.grant.issued_at
            && now < self.grant.not_after
            && (self.grant.mode != LaneMode::Expiring || now < self.horizon_at)
    }

    /// Whether this lane currently provides access, independent of message scope.
    pub fn is_current(&self, now: CanonicalTime) -> bool {
        self.state == LaneState::Active
            && now >= self.grant.not_before
            && now < self.grant.not_after
            && (self.grant.mode != LaneMode::Expiring || now < self.horizon_at)
    }

    /// Checks pair, evidence, state, and validity without consuming allowance.
    ///
    /// # Errors
    ///
    /// Returns `NotAuthorized` when any authorization condition fails.
    pub fn authorizes(
        &self,
        sender: ProtocolIdentity,
        recipient: ProtocolIdentity,
        capability: Option<LaneId>,
        declarations: &MessageDeclarations,
        evidence: &LaneEvidence,
        now: CanonicalTime,
    ) -> Result<(), CapabilityError> {
        declarations
            .validate()
            .map_err(|_| CapabilityError::InvalidDeclaration)?;
        if capability != Some(self.grant.id)
            || declarations.purpose != self.grant.purpose
            || self
                .grant
                .origin
                .is_some_and(|origin| origin != declarations.origin.mode)
            || !self
                .grant
                .declaration_authority
                .matches(declarations.origin.authority)
        {
            return Err(CapabilityError::ScopeMismatch);
        }
        if self.state != LaneState::Active
            || sender != self.grant.sender
            || recipient != self.grant.recipient
            || now < self.grant.not_before
            || now >= self.grant.not_after
            || (self.grant.mode == LaneMode::Expiring && now >= self.horizon_at)
            || !evidence.matches(&self.grant.subject)
        {
            return Err(CapabilityError::NotAuthorized);
        }
        Ok(())
    }

    /// Idempotently consumes one message from the current rate window.
    ///
    /// # Errors
    ///
    /// Returns an authorization, rate-limit, or arithmetic error.
    pub fn consume(
        &mut self,
        message: MessageId,
        capability: Option<LaneId>,
        declarations: &MessageDeclarations,
        evidence: &LaneEvidence,
        now: CanonicalTime,
    ) -> Result<bool, CapabilityError> {
        self.authorizes(
            self.grant.sender,
            self.grant.recipient,
            capability,
            declarations,
            evidence,
            now,
        )?;
        if self.admitted_messages.contains(&message) {
            return Ok(true);
        }
        let rate_window_end = self
            .rate_window_started_at
            .checked_add(self.grant.rate_limit.interval)
            .ok_or(CapabilityError::ArithmeticOverflow)?;
        if message.0 == 0 || now < self.last_activity {
            return Err(CapabilityError::NotAuthorized);
        }
        let (window, count) = if now >= rate_window_end {
            now.checked_add(self.grant.rate_limit.interval)
                .ok_or(CapabilityError::ArithmeticOverflow)?;
            (now, 0)
        } else {
            (self.rate_window_started_at, self.messages_in_rate_window)
        };
        if count >= self.grant.rate_limit.max_messages {
            return Err(CapabilityError::RateLimitExceeded);
        }
        let count = count
            .checked_add(1)
            .ok_or(CapabilityError::ArithmeticOverflow)?;
        let horizon = now
            .checked_add(self.grant.lifetime)
            .ok_or(CapabilityError::ArithmeticOverflow)?
            .min(self.grant.not_after);
        let version = self
            .version
            .checked_next()
            .ok_or(CapabilityError::ArithmeticOverflow)?;
        self.rate_window_started_at = window;
        self.messages_in_rate_window = count;
        self.last_activity = now;
        self.horizon_at = horizon;
        self.version = version;
        self.admitted_messages.insert(message);
        Ok(false)
    }

    /// Applies a due lifetime horizon.
    ///
    /// # Errors
    ///
    /// Returns an arithmetic error when a persistent horizon cannot advance.
    pub fn reach_horizon(
        &mut self,
        now: CanonicalTime,
    ) -> Result<LaneHorizonEffect, CapabilityError> {
        if now < self.horizon_at || self.state != LaneState::Active {
            return Ok(LaneHorizonEffect::None);
        }
        let version = self
            .version
            .checked_next()
            .ok_or(CapabilityError::ArithmeticOverflow)?;
        let horizon = if self.grant.mode == LaneMode::Persistent && now < self.grant.not_after {
            now.checked_add(self.grant.lifetime)
                .ok_or(CapabilityError::ArithmeticOverflow)?
                .min(self.grant.not_after)
        } else {
            self.horizon_at
        };
        self.version = version;
        if now >= self.grant.not_after {
            self.state = LaneState::Expired;
            return Ok(LaneHorizonEffect::ReconfirmationRequired);
        }
        match self.grant.mode {
            LaneMode::Expiring => {
                self.state = LaneState::AwaitingReconfirmation;
                Ok(LaneHorizonEffect::ReconfirmationRequired)
            }
            LaneMode::Persistent => {
                self.horizon_at = horizon;
                Ok(LaneHorizonEffect::ReviewReminder)
            }
        }
    }

    /// Irreversibly revokes this version of the lane.
    ///
    /// # Errors
    ///
    /// Returns an error if the version cannot advance.
    pub fn revoke(&mut self) -> Result<(), CapabilityError> {
        if self.state == LaneState::Revoked {
            return Ok(());
        }
        let version = self
            .version
            .checked_next()
            .ok_or(CapabilityError::ArithmeticOverflow)?;
        self.state = LaneState::Revoked;
        self.version = version;
        Ok(())
    }

    /// Plans revocation without modifying the protected input snapshot.
    /// # Errors
    /// Rejects version overflow before producing any effect.
    pub fn plan_revocation(&self) -> Result<Option<LaneRevocation>, CapabilityError> {
        if self.state == LaneState::Revoked {
            return Ok(None);
        }
        let mut lane = self.clone();
        lane.revoke()?;
        Ok(Some(LaneRevocation {
            expected_version: self.version,
            lane,
        }))
    }

    /// Reactivates an awaiting lane within its signed hard validity interval.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid state, expired authority, or overflow.
    pub fn reconfirm(&mut self, now: CanonicalTime) -> Result<(), CapabilityError> {
        if self.state != LaneState::AwaitingReconfirmation
            || now >= self.grant.not_after
            || now < self.horizon_at
        {
            return Err(CapabilityError::NotAuthorized);
        }
        let horizon = now
            .checked_add(self.grant.lifetime)
            .ok_or(CapabilityError::ArithmeticOverflow)?
            .min(self.grant.not_after);
        let version = self
            .version
            .checked_next()
            .ok_or(CapabilityError::ArithmeticOverflow)?;
        self.state = LaneState::Active;
        self.last_activity = now;
        self.horizon_at = horizon;
        self.version = version;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LaneEvidence {
    Native(ScopedHandle),
    Legacy(LegacyDmarcEvidence),
    GuestMailbox(ProtocolIdentity),
}

impl LaneEvidence {
    pub fn matches(&self, subject: &LaneSubject) -> bool {
        match (self, subject) {
            (Self::GuestMailbox(actual), LaneSubject::GuestMailbox(expected)) => actual == expected,
            (Self::Native(actual), LaneSubject::Native(expected)) => actual == expected,
            (Self::Legacy(actual), LaneSubject::LegacyDomain(expected)) => {
                actual.lane_identity() == expected
            }
            _ => false,
        }
    }

    fn append_signing_bytes(&self, bytes: &mut Vec<u8>) -> Result<(), CapabilityError> {
        match self {
            Self::GuestMailbox(identity) => {
                bytes.push(2);
                bytes.extend_from_slice(&identity.0.to_be_bytes());
            }
            Self::Native(handle) => {
                bytes.push(0);
                bytes.extend_from_slice(&handle.0);
            }
            Self::Legacy(evidence) => {
                bytes.push(1);
                bytes.extend_from_slice(&evidence.format_version.to_be_bytes());
                push_bytes(bytes, evidence.lane_domain.as_str().as_bytes())?;
                push_bytes(
                    bytes,
                    evidence.verification.author_domain.as_str().as_bytes(),
                )?;
                append_dmarc_pass(bytes, evidence.verification.dkim);
                append_dmarc_pass(bytes, evidence.verification.spf);
                bytes.extend_from_slice(&evidence.verification.evaluated_at.0.to_be_bytes());
            }
        }
        Ok(())
    }
}

fn append_dmarc_pass(bytes: &mut Vec<u8>, pass: Option<cs_mail_adapters::DmarcPass>) {
    match pass {
        Some(pass) => {
            bytes.push(1);
            bytes.push(match pass.alignment {
                cs_mail_adapters::DmarcAlignment::Strict => 0,
                cs_mail_adapters::DmarcAlignment::Relaxed => 1,
            });
        }
        None => bytes.push(0),
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BondFreeAdmission {
    pub wire_version: WireVersion,
    pub sender: ProtocolIdentity,
    pub recipient: ProtocolIdentity,
    pub message_id: MessageId,
    pub content_ref: ContentRef,
    pub delivery_intent_ref: DeliveryIntentRef,
    pub declarations: MessageDeclarations,
    pub message_valid_until: MessageValidityUntil,
    pub capability: Option<LaneId>,
    pub evidence: Option<LaneEvidence>,
    pub idempotency_key: IdempotencyKey,
    pub protocol_version: ProtocolVersion,
    pub deployment_domain: [u8; 32],
    pub intended_provider: ProviderRef,
    pub authentication: AdmissionAuthentication,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AdmissionAuthentication {
    NativeKey(OperationalKeyRef),
    LegacyDmarc,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyBondFreeAdmission {
    pub sender: ProtocolIdentity,
    pub recipient: ProtocolIdentity,
    pub message_id: MessageId,
    pub content_ref: ContentRef,
    pub delivery_intent_ref: DeliveryIntentRef,
    pub purpose: DeclaredPurpose,
    pub payload_schema: Option<PayloadSchema>,
    pub message_valid_until: MessageValidityUntil,
    pub capability: Option<LaneId>,
    pub idempotency_key: IdempotencyKey,
    pub protocol_version: ProtocolVersion,
    pub deployment_domain: [u8; 32],
    pub intended_provider: ProviderRef,
    /// Authenticated domain used for the stable legacy sender identity.
    pub sender_domain: DomainIdentity,
}

impl BondFreeAdmission {
    fn validate(&self) -> Result<(), CapabilityError> {
        if self.wire_version != WireVersion(1) {
            return Err(CapabilityError::UnsupportedWireVersion);
        }
        self.declarations
            .validate()
            .map_err(|_| CapabilityError::InvalidDeclaration)?;
        match (self.authentication, self.declarations.origin.authority) {
            (
                AdmissionAuthentication::NativeKey(expected),
                DeclarationAuthority::NativeSender(actual),
            ) if expected == actual => {}
            (AdmissionAuthentication::LegacyDmarc, DeclarationAuthority::LegacyGateway(actual))
                if actual == self.intended_provider => {}
            _ => return Err(CapabilityError::InvalidDeclaration),
        }
        Ok(())
    }

    /// Returns the domain-separated canonical admission representation.
    ///
    /// # Errors
    ///
    /// Returns an error when variable-length evidence cannot be represented.
    pub fn signing_bytes(&self) -> Result<Vec<u8>, CapabilityError> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(448);
        bytes.extend_from_slice(b"cs-mail/bond-free-admission/v2");
        bytes.extend_from_slice(&self.wire_version.0.to_be_bytes());
        bytes.extend_from_slice(&self.sender.0.to_be_bytes());
        bytes.extend_from_slice(&self.recipient.0.to_be_bytes());
        bytes.extend_from_slice(&self.message_id.0.to_be_bytes());
        bytes.extend_from_slice(&self.content_ref.0.to_be_bytes());
        bytes.extend_from_slice(&self.delivery_intent_ref.0.to_be_bytes());
        bytes.extend_from_slice(
            &self
                .declarations
                .canonical_bytes()
                .map_err(|_| CapabilityError::InvalidDeclaration)?,
        );
        bytes.extend_from_slice(&self.message_valid_until.0.0.to_be_bytes());
        match self.capability {
            Some(lane_id) => {
                bytes.push(1);
                bytes.extend_from_slice(&lane_id.0.to_be_bytes());
            }
            None => bytes.push(0),
        }
        match &self.evidence {
            Some(evidence) => {
                bytes.push(1);
                evidence.append_signing_bytes(&mut bytes)?;
            }
            None => bytes.push(0),
        }
        bytes.extend_from_slice(&self.idempotency_key.0.to_be_bytes());
        bytes.extend_from_slice(&self.protocol_version.0.to_be_bytes());
        bytes.extend_from_slice(&self.deployment_domain);
        bytes.extend_from_slice(&self.intended_provider.0.to_be_bytes());
        match self.authentication {
            AdmissionAuthentication::NativeKey(key) => {
                bytes.push(0);
                bytes.extend_from_slice(&key.0.to_be_bytes());
            }
            AdmissionAuthentication::LegacyDmarc => bytes.push(1),
        }
        Ok(bytes)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaneHorizonEffect {
    None,
    ReconfirmationRequired,
    ReviewReminder,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CapabilityError {
    InvalidGrant,
    InvalidPublicKey,
    InvalidSignature,
    DuplicateConflict,
    MissingLane,
    NotAuthorized,
    RateLimitExceeded,
    CapacityExceeded,
    ArithmeticOverflow,
    InvalidDeclaration,
    ScopeMismatch,
    MessageValidityClosed,
    UnsupportedCriticalExtension,
    UnsupportedWireVersion,
}

impl fmt::Display for CapabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for CapabilityError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneBook {
    max_records: usize,
    lanes: BTreeMap<LaneId, Lane>,
    operations: BTreeMap<IdempotencyKey, LaneGrant>,
}

impl LaneBook {
    pub const fn new(max_records: usize) -> Self {
        Self {
            max_records,
            lanes: BTreeMap::new(),
            operations: BTreeMap::new(),
        }
    }

    /// Verifies and idempotently records a grant in the bounded in-memory book.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid authority, conflict, capacity, or arithmetic failure.
    pub fn grant(
        &mut self,
        signed: SignedLaneGrant,
        recipient_key: &[u8; 32],
        idempotency: IdempotencyKey,
    ) -> Result<ScheduleChange, CapabilityError> {
        let verified = VerifiedLaneGrant::verify(&signed, recipient_key)?;
        if let Some(existing) = self.operations.get(&idempotency) {
            return if *existing == signed.grant {
                self.lanes
                    .get(&existing.id)
                    .map(Lane::schedule_change)
                    .ok_or(CapabilityError::MissingLane)
            } else {
                Err(CapabilityError::DuplicateConflict)
            };
        }
        if self.lanes.len() >= self.max_records && !self.lanes.contains_key(&signed.grant.id) {
            return Err(CapabilityError::CapacityExceeded);
        }
        let lane = verified.activate()?;
        let id = lane.grant.id;
        match self.lanes.get(&id) {
            Some(existing) if existing == &lane => {}
            Some(_) => return Err(CapabilityError::DuplicateConflict),
            None => {
                self.lanes.insert(id, lane);
            }
        }
        self.operations.insert(idempotency, signed.grant);
        Ok(self.lanes[&id].schedule_change())
    }

    pub fn lane(&self, id: LaneId) -> Option<&Lane> {
        self.lanes.get(&id)
    }
}

/// Evidence issued only by the gateway verification path, never deserialized from a request.
/// Authentication evidence cannot be fabricated by deserializing caller data.
/// ```compile_fail
/// fn accepts_json<T: for<'de> serde::Deserialize<'de>>() {}
/// accepts_json::<cs_mail_capabilities::VerifiedLegacyAdmission>();
/// ```
pub struct VerifiedLegacyAdmission {
    admission: BondFreeAdmission,
}
impl VerifiedLegacyAdmission {
    pub fn admission(&self) -> &BondFreeAdmission {
        &self.admission
    }
}
/// Authenticates legacy origin and binds its declared sender to the selected domain.
/// # Errors
/// Refuses DMARC failure, inconsistent evidence time or mismatched sender identity.
pub async fn verify_legacy_admission<V: cs_mail_adapters::DmarcVerifier>(
    verifier: &V,
    authentication: cs_mail_adapters::SmtpAuthenticationRequest<'_>,
    request: &LegacyBondFreeAdmission,
    mapping_version: u16,
) -> Result<VerifiedLegacyAdmission, CapabilityError> {
    let at = authentication.received_at;
    let evidence = verifier
        .verify(authentication)
        .await
        .map_err(|_| CapabilityError::NotAuthorized)?;
    if evidence.evaluated_at != at
        || request
            .sender_domain
            .synthetic_protocol_identity(&request.deployment_domain, mapping_version)
            != request.sender
    {
        return Err(CapabilityError::ScopeMismatch);
    }
    let evidence =
        cs_mail_adapters::LegacyDmarcEvidence::new(request.sender_domain.clone(), evidence)
            .map_err(|_| CapabilityError::NotAuthorized)?;
    Ok(VerifiedLegacyAdmission {
        admission: BondFreeAdmission {
            wire_version: WireVersion(1),
            sender: request.sender,
            recipient: request.recipient,
            message_id: request.message_id,
            content_ref: request.content_ref,
            delivery_intent_ref: request.delivery_intent_ref,
            declarations: MessageDeclarations {
                purpose: request.purpose.clone(),
                payload_schema: request.payload_schema.clone(),
                origin: cs_mail_primitives::OriginDeclaration {
                    mode: cs_mail_primitives::OriginMode::LegacyOrUnspecified,
                    authority: DeclarationAuthority::LegacyGateway(request.intended_provider),
                },
            },
            message_valid_until: request.message_valid_until,
            capability: request.capability,
            evidence: Some(LaneEvidence::Legacy(evidence)),
            idempotency_key: request.idempotency_key,
            protocol_version: request.protocol_version,
            deployment_domain: request.deployment_domain,
            intended_provider: request.intended_provider,
            authentication: AdmissionAuthentication::LegacyDmarc,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cs_mail_adapters::{DmarcAlignment, DmarcPass, VerifiedDomain};
    use ed25519_dalek::{Signer, SigningKey};

    fn signed_grant() -> (SignedLaneGrant, [u8; 32]) {
        let signing = SigningKey::from_bytes(&[7; 32]);
        let grant = LaneGrant {
            id: LaneId(1),
            subject: LaneSubject::LegacyDomain(DomainIdentity::parse_ascii("bank.com").unwrap()),
            sender: ProtocolIdentity(2),
            recipient: ProtocolIdentity(3),
            protocol_version: ProtocolVersion(2),
            deployment_domain: [3; 32],
            intended_provider: ProviderRef(4),
            recipient_operational_key: OperationalKeyRef(5),
            purpose: DeclaredPurpose::Known(cs_mail_primitives::KnownPurpose::Transactional),
            origin: Some(OriginMode::LegacyOrUnspecified),
            declaration_authority: DeclarationAuthorityConstraint::LegacyGateway,
            lifetime: Duration(1_000),
            rate_limit: RateLimit {
                max_messages: 1,
                interval: Duration(100),
            },
            mode: LaneMode::Expiring,
            issued_at: CanonicalTime(10),
            not_before: CanonicalTime(10),
            not_after: CanonicalTime(10_000),
            version: Version(1),
        };
        let signature = signing.sign(&grant.signing_bytes().unwrap()).to_bytes();
        (
            SignedLaneGrant { grant, signature },
            signing.verifying_key().to_bytes(),
        )
    }

    fn evidence() -> LaneEvidence {
        LaneEvidence::Legacy(
            LegacyDmarcEvidence::new(
                DomainIdentity::parse_ascii("bank.com").unwrap(),
                VerifiedDomain::passed(
                    DomainIdentity::parse_ascii("alerts.e.bank.com").unwrap(),
                    Some(DmarcPass {
                        alignment: DmarcAlignment::Relaxed,
                    }),
                    None,
                    CanonicalTime(20),
                )
                .unwrap(),
            )
            .unwrap(),
        )
    }

    fn legacy_declarations() -> MessageDeclarations {
        MessageDeclarations {
            purpose: DeclaredPurpose::Known(cs_mail_primitives::KnownPurpose::Transactional),
            origin: cs_mail_primitives::OriginDeclaration {
                mode: OriginMode::LegacyOrUnspecified,
                authority: DeclarationAuthority::LegacyGateway(ProviderRef(4)),
            },
            payload_schema: None,
        }
    }

    // Conjecture: restoration preserves used allowance and rejects inconsistent
    // state. A reset counter or extended horizon would grant unauthorized delivery.
    #[test]
    fn restored_lane_preserves_allowance_and_rejects_inconsistent_records() {
        let (signed, key) = signed_grant();
        let mut lane = VerifiedLaneGrant::verify(&signed, &key)
            .unwrap()
            .activate()
            .unwrap();
        lane.consume(
            MessageId(1),
            Some(LaneId(1)),
            &legacy_declarations(),
            &evidence(),
            CanonicalTime(20),
        )
        .unwrap();
        let record = serde_json::to_value(&lane).unwrap();
        let mut restored: Lane = serde_json::from_value(record.clone()).unwrap();
        assert_eq!(
            restored.consume(
                MessageId(2),
                Some(LaneId(1)),
                &legacy_declarations(),
                &evidence(),
                CanonicalTime(21)
            ),
            Err(CapabilityError::RateLimitExceeded)
        );
        for (field, value) in [
            ("messages_in_rate_window", serde_json::json!(0)),
            ("horizon_at", serde_json::json!(9999)),
            ("version", serde_json::json!(0)),
            ("admitted_messages", serde_json::json!([])),
            ("rate_window_started_at", serde_json::json!(22)),
        ] {
            let mut invalid = record.clone();
            invalid[field] = value;
            assert!(serde_json::from_value::<Lane>(invalid).is_err(), "{field}");
        }
        let mut invalid = record;
        invalid["grant"]["subject"]["LegacyDomain"] = serde_json::json!("not a domain");
        assert!(serde_json::from_value::<Lane>(invalid).is_err());
        restored.revoke().unwrap();
        let revoked: Lane =
            serde_json::from_value(serde_json::to_value(restored).unwrap()).unwrap();
        assert!(!revoked.is_current(CanonicalTime(21)));
        // Valid lifecycle transitions must also survive restoration: rejecting
        // them would strand an otherwise valid lane after a process restart.
        for mode in [LaneMode::Expiring, LaneMode::Persistent] {
            let (mut signed, key) = signed_grant();
            signed.grant.mode = mode;
            signed.signature = SigningKey::from_bytes(&[7; 32])
                .sign(&signed.grant.signing_bytes().unwrap())
                .to_bytes();
            let mut lane = VerifiedLaneGrant::verify(&signed, &key)
                .unwrap()
                .activate()
                .unwrap();
            lane.reach_horizon(CanonicalTime(1010)).unwrap();
            lane = serde_json::from_value(serde_json::to_value(&lane).unwrap()).unwrap();
            if mode == LaneMode::Expiring {
                assert!(!lane.is_current(CanonicalTime(1011)));
                lane.reconfirm(CanonicalTime(1011)).unwrap();
            }
            let restored: Lane =
                serde_json::from_value(serde_json::to_value(&lane).unwrap()).unwrap();
            assert_eq!(restored, lane);
            assert!(restored.is_current(CanonicalTime(1011)));
            lane.reach_horizon(CanonicalTime(10_000)).unwrap();
            let expired: Lane =
                serde_json::from_value(serde_json::to_value(lane).unwrap()).unwrap();
            assert!(!expired.is_current(CanonicalTime(10_000)));
        }
    }

    // Conjecture: a failed transition changes nothing. Overflow after a counter or
    // state mutation would leave callers with an uncommitted partial consequence.
    #[test]
    fn overflow_leaves_each_lane_transition_unchanged() {
        let (signed, key) = signed_grant();
        let mut lane = VerifiedLaneGrant::verify(&signed, &key)
            .unwrap()
            .activate()
            .unwrap();
        lane.version = Version(u64::MAX);
        let before = lane.clone();
        assert_eq!(
            lane.consume(
                MessageId(1),
                Some(LaneId(1)),
                &legacy_declarations(),
                &evidence(),
                CanonicalTime(110)
            ),
            Err(CapabilityError::ArithmeticOverflow)
        );
        assert_eq!(lane, before);
        assert_eq!(lane.revoke(), Err(CapabilityError::ArithmeticOverflow));
        assert_eq!(lane, before);
        assert_eq!(
            lane.reach_horizon(CanonicalTime(1010)),
            Err(CapabilityError::ArithmeticOverflow)
        );
        assert_eq!(lane, before);
        lane.state = LaneState::AwaitingReconfirmation;
        let before = lane.clone();
        assert_eq!(
            lane.reconfirm(CanonicalTime(1010)),
            Err(CapabilityError::ArithmeticOverflow)
        );
        assert_eq!(lane, before);
    }

    #[test]
    fn recipient_signature_domain_evidence_and_rate_are_enforced() {
        let (signed, key) = signed_grant();
        let mut lane = Lane::from_grant(signed.grant.clone()).unwrap();
        signed.verify(&key).unwrap();
        assert!(
            !lane
                .consume(
                    MessageId(1),
                    Some(LaneId(1)),
                    &legacy_declarations(),
                    &evidence(),
                    CanonicalTime(20),
                )
                .unwrap()
        );
        assert!(
            lane.consume(
                MessageId(1),
                Some(LaneId(1)),
                &legacy_declarations(),
                &evidence(),
                CanonicalTime(21),
            )
            .unwrap()
        );
        assert_eq!(
            lane.consume(
                MessageId(2),
                Some(LaneId(1)),
                &legacy_declarations(),
                &evidence(),
                CanonicalTime(22),
            ),
            Err(CapabilityError::RateLimitExceeded)
        );
    }

    #[test]
    fn expiring_lane_requires_reconfirmation_at_horizon() {
        let (signed, _) = signed_grant();
        let mut lane = Lane::from_grant(signed.grant).unwrap();
        assert_eq!(
            lane.reach_horizon(CanonicalTime(1_010)).unwrap(),
            LaneHorizonEffect::ReconfirmationRequired
        );
        assert_eq!(
            lane.authorizes(
                ProtocolIdentity(2),
                ProtocolIdentity(3),
                Some(LaneId(1)),
                &legacy_declarations(),
                &evidence(),
                CanonicalTime(1_010)
            ),
            Err(CapabilityError::NotAuthorized)
        );
        lane.reconfirm(CanonicalTime(1_011)).unwrap();
        lane.authorizes(
            ProtocolIdentity(2),
            ProtocolIdentity(3),
            Some(LaneId(1)),
            &legacy_declarations(),
            &evidence(),
            CanonicalTime(1_011),
        )
        .unwrap();
    }

    #[test]
    fn declaration_scope_mismatch_does_not_consume_lane_allowance() {
        let (signed, _) = signed_grant();
        let mut lane = Lane::from_grant(signed.grant).unwrap();
        let mut wrong_purpose = legacy_declarations();
        wrong_purpose.purpose = DeclaredPurpose::Known(cs_mail_primitives::KnownPurpose::Personal);

        assert_eq!(
            lane.consume(
                MessageId(1),
                Some(LaneId(1)),
                &wrong_purpose,
                &evidence(),
                CanonicalTime(20),
            ),
            Err(CapabilityError::ScopeMismatch)
        );
        assert_eq!(lane.messages_in_rate_window, 0);
        assert!(
            !lane
                .consume(
                    MessageId(1),
                    Some(LaneId(1)),
                    &legacy_declarations(),
                    &evidence(),
                    CanonicalTime(20),
                )
                .unwrap()
        );
        assert_eq!(lane.messages_in_rate_window, 1);
    }

    #[test]
    fn lane_control_signature_covers_action_and_version() {
        let signing = SigningKey::from_bytes(&[6; 32]);
        let control = LaneControl {
            lane_id: LaneId(1),
            recipient: ProtocolIdentity(3),
            action: LaneControlAction::Revoke,
            expected_version: Version(2),
            idempotency_key: IdempotencyKey(13),
            protocol_version: ProtocolVersion(2),
            deployment_domain: [3; 32],
            intended_provider: ProviderRef(4),
            recipient_operational_key: OperationalKeyRef(5),
        };
        let mut signed = SignedLaneControl {
            signature: signing.sign(&control.signing_bytes()).to_bytes(),
            control,
        };
        signed.verify(&signing.verifying_key().to_bytes()).unwrap();
        signed.control.action = LaneControlAction::Reconfirm;
        assert_eq!(
            signed.verify(&signing.verifying_key().to_bytes()),
            Err(CapabilityError::InvalidSignature)
        );
    }
}
