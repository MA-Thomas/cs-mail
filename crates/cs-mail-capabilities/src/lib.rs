//! Recipient-granted, moneyless communication capabilities.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use cs_mail_adapters::{DomainIdentity, LegacyDmarcEvidence};
use cs_mail_primitives::{
    CanonicalTime, ContentRef, DeliveryIntentRef, Duration, IdempotencyKey, LaneId, MessageId,
    OperationalKeyRef, ProtocolIdentity, ProtocolVersion, ProviderRef, ScheduleChange,
    ScheduleTask, Version,
};
use cs_mail_privacy::ScopedHandle;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

const SIGNING_DOMAIN: &[u8] = b"cs-mail/lane-grant/v1";
const ADMISSION_SIGNING_DOMAIN: &[u8] = b"cs-mail/bond-free-admission/v1";
const CONTROL_SIGNING_DOMAIN: &[u8] = b"cs-mail/lane-control/v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LaneSubject {
    Native(ScopedHandle),
    LegacyDomain(DomainIdentity),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LaneMode {
    Expiring,
    Persistent,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
    pub purpose_label: String,
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
        if self.purpose_label.is_empty()
            || self.purpose_label.len() > 128
            || self.lifetime.0 == 0
            || self.protocol_version.0 == 0
            || !self.rate_limit.valid()
            || self.not_before < self.issued_at
            || self.not_after <= self.not_before
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
        push_bytes(&mut bytes, self.purpose_label.as_bytes())?;
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedLaneGrant {
    pub grant: LaneGrant,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Lane {
    pub grant: LaneGrant,
    pub state: LaneState,
    pub last_activity: CanonicalTime,
    pub horizon_at: CanonicalTime,
    pub rate_window_started_at: CanonicalTime,
    pub messages_in_rate_window: u32,
    pub version: Version,
    admitted_messages: BTreeSet<MessageId>,
}

impl Lane {
    fn from_grant(grant: LaneGrant) -> Result<Self, CapabilityError> {
        grant.validate()?;
        let horizon_at = grant
            .issued_at
            .checked_add(grant.lifetime)
            .ok_or(CapabilityError::ArithmeticOverflow)?
            .min(grant.not_after);
        Ok(Self {
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

    /// Verifies the recipient signature and activates the resulting lane.
    ///
    /// # Errors
    ///
    /// Returns an error when the signature or grant is invalid.
    pub fn from_verified_grant(
        signed: &SignedLaneGrant,
        recipient_key: &[u8; 32],
    ) -> Result<Self, CapabilityError> {
        signed.verify(recipient_key)?;
        Self::from_grant(signed.grant.clone())
    }

    pub fn schedule_change(&self) -> ScheduleChange {
        ScheduleChange::Schedule {
            task: ScheduleTask::LaneHorizon(self.grant.id),
            at: self.horizon_at,
        }
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
        evidence: &LaneEvidence,
        now: CanonicalTime,
    ) -> Result<(), CapabilityError> {
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
        evidence: &LaneEvidence,
        now: CanonicalTime,
    ) -> Result<bool, CapabilityError> {
        self.authorizes(self.grant.sender, self.grant.recipient, evidence, now)?;
        if self.admitted_messages.contains(&message) {
            return Ok(true);
        }
        let rate_window_end = self
            .rate_window_started_at
            .checked_add(self.grant.rate_limit.interval)
            .ok_or(CapabilityError::ArithmeticOverflow)?;
        if now >= rate_window_end {
            self.rate_window_started_at = now;
            self.messages_in_rate_window = 0;
        }
        if self.messages_in_rate_window >= self.grant.rate_limit.max_messages {
            return Err(CapabilityError::RateLimitExceeded);
        }
        self.messages_in_rate_window = self
            .messages_in_rate_window
            .checked_add(1)
            .ok_or(CapabilityError::ArithmeticOverflow)?;
        self.last_activity = self.last_activity.max(now);
        self.horizon_at = self
            .last_activity
            .checked_add(self.grant.lifetime)
            .ok_or(CapabilityError::ArithmeticOverflow)?
            .min(self.grant.not_after);
        self.version = self
            .version
            .checked_next()
            .ok_or(CapabilityError::ArithmeticOverflow)?;
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
        self.version = self
            .version
            .checked_next()
            .ok_or(CapabilityError::ArithmeticOverflow)?;
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
                self.horizon_at = now
                    .checked_add(self.grant.lifetime)
                    .ok_or(CapabilityError::ArithmeticOverflow)?
                    .min(self.grant.not_after);
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
        self.state = LaneState::Revoked;
        self.version = self
            .version
            .checked_next()
            .ok_or(CapabilityError::ArithmeticOverflow)?;
        Ok(())
    }

    /// Reactivates an awaiting lane within its signed hard validity interval.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid state, expired authority, or overflow.
    pub fn reconfirm(&mut self, now: CanonicalTime) -> Result<(), CapabilityError> {
        if self.state != LaneState::AwaitingReconfirmation || now >= self.grant.not_after {
            return Err(CapabilityError::NotAuthorized);
        }
        self.state = LaneState::Active;
        self.last_activity = self.last_activity.max(now);
        self.horizon_at = now
            .checked_add(self.grant.lifetime)
            .ok_or(CapabilityError::ArithmeticOverflow)?
            .min(self.grant.not_after);
        self.version = self
            .version
            .checked_next()
            .ok_or(CapabilityError::ArithmeticOverflow)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LaneEvidence {
    Native(ScopedHandle),
    Legacy(LegacyDmarcEvidence),
}

impl LaneEvidence {
    pub fn matches(&self, subject: &LaneSubject) -> bool {
        match (self, subject) {
            (Self::Native(actual), LaneSubject::Native(expected)) => actual == expected,
            (Self::Legacy(actual), LaneSubject::LegacyDomain(expected)) => {
                actual.lane_identity() == expected
            }
            _ => false,
        }
    }

    fn append_signing_bytes(&self, bytes: &mut Vec<u8>) -> Result<(), CapabilityError> {
        match self {
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
    pub sender: ProtocolIdentity,
    pub recipient: ProtocolIdentity,
    pub message_id: MessageId,
    pub content_ref: ContentRef,
    pub delivery_intent_ref: DeliveryIntentRef,
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
    pub idempotency_key: IdempotencyKey,
    pub protocol_version: ProtocolVersion,
    pub deployment_domain: [u8; 32],
    pub intended_provider: ProviderRef,
    /// Recipient-granted scope to which authenticated author domains are bound.
    pub lane_domain: DomainIdentity,
}

impl BondFreeAdmission {
    /// Returns the domain-separated canonical admission representation.
    ///
    /// # Errors
    ///
    /// Returns an error when variable-length evidence cannot be represented.
    pub fn signing_bytes(&self) -> Result<Vec<u8>, CapabilityError> {
        let mut bytes = Vec::with_capacity(320);
        bytes.extend_from_slice(ADMISSION_SIGNING_DOMAIN);
        bytes.extend_from_slice(&self.sender.0.to_be_bytes());
        bytes.extend_from_slice(&self.recipient.0.to_be_bytes());
        bytes.extend_from_slice(&self.message_id.0.to_be_bytes());
        bytes.extend_from_slice(&self.content_ref.0.to_be_bytes());
        bytes.extend_from_slice(&self.delivery_intent_ref.0.to_be_bytes());
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignedBondFreeAdmission {
    pub admission: BondFreeAdmission,
    pub signature: [u8; 64],
}

impl SignedBondFreeAdmission {
    /// Verifies the sender's signature over the complete delivery request and
    /// its provider/deployment scope.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed keys or invalid signatures.
    pub fn verify(&self, sender_key: &[u8; 32]) -> Result<(), CapabilityError> {
        let key =
            VerifyingKey::from_bytes(sender_key).map_err(|_| CapabilityError::InvalidPublicKey)?;
        key.verify(
            &self.admission.signing_bytes()?,
            &Signature::from_bytes(&self.signature),
        )
        .map_err(|_| CapabilityError::InvalidSignature)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaneHorizonEffect {
    None,
    ReconfirmationRequired,
    ReviewReminder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    operations: BTreeMap<IdempotencyKey, LaneId>,
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
        signed.verify(recipient_key)?;
        if let Some(existing) = self.operations.get(&idempotency) {
            return if *existing == signed.grant.id {
                self.lanes
                    .get(existing)
                    .map(Lane::schedule_change)
                    .ok_or(CapabilityError::MissingLane)
            } else {
                Err(CapabilityError::DuplicateConflict)
            };
        }
        if self.lanes.len() >= self.max_records && !self.lanes.contains_key(&signed.grant.id) {
            return Err(CapabilityError::CapacityExceeded);
        }
        let lane = Lane::from_grant(signed.grant)?;
        let id = lane.grant.id;
        match self.lanes.get(&id) {
            Some(existing) if existing == &lane => {}
            Some(_) => return Err(CapabilityError::DuplicateConflict),
            None => {
                self.lanes.insert(id, lane);
            }
        }
        self.operations.insert(idempotency, id);
        Ok(self.lanes[&id].schedule_change())
    }

    pub fn lane(&self, id: LaneId) -> Option<&Lane> {
        self.lanes.get(&id)
    }
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
            protocol_version: ProtocolVersion(1),
            deployment_domain: [3; 32],
            intended_provider: ProviderRef(4),
            recipient_operational_key: OperationalKeyRef(5),
            purpose_label: "account notices".into(),
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

    #[test]
    fn recipient_signature_domain_evidence_and_rate_are_enforced() {
        let (signed, key) = signed_grant();
        let mut lane = Lane::from_grant(signed.grant.clone()).unwrap();
        signed.verify(&key).unwrap();
        assert!(
            !lane
                .consume(MessageId(1), &evidence(), CanonicalTime(20))
                .unwrap()
        );
        assert!(
            lane.consume(MessageId(1), &evidence(), CanonicalTime(21))
                .unwrap()
        );
        assert_eq!(
            lane.consume(MessageId(2), &evidence(), CanonicalTime(22)),
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
                &evidence(),
                CanonicalTime(1_010)
            ),
            Err(CapabilityError::NotAuthorized)
        );
        lane.reconfirm(CanonicalTime(1_011)).unwrap();
        lane.authorizes(
            ProtocolIdentity(2),
            ProtocolIdentity(3),
            &evidence(),
            CanonicalTime(1_011),
        )
        .unwrap();
    }

    #[test]
    fn bond_free_admission_signature_covers_scope_and_evidence() {
        let signing = SigningKey::from_bytes(&[8; 32]);
        let admission = BondFreeAdmission {
            sender: ProtocolIdentity(2),
            recipient: ProtocolIdentity(3),
            message_id: MessageId(4),
            content_ref: ContentRef(5),
            delivery_intent_ref: DeliveryIntentRef(6),
            evidence: Some(evidence()),
            idempotency_key: IdempotencyKey(7),
            protocol_version: ProtocolVersion(1),
            deployment_domain: [9; 32],
            intended_provider: ProviderRef(10),
            authentication: AdmissionAuthentication::NativeKey(OperationalKeyRef(11)),
        };
        let signed = SignedBondFreeAdmission {
            signature: signing.sign(&admission.signing_bytes().unwrap()).to_bytes(),
            admission: admission.clone(),
        };
        signed.verify(&signing.verifying_key().to_bytes()).unwrap();
        let mut altered = signed;
        altered.admission.intended_provider = ProviderRef(12);
        assert_eq!(
            altered.verify(&signing.verifying_key().to_bytes()),
            Err(CapabilityError::InvalidSignature)
        );
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
            protocol_version: ProtocolVersion(1),
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
