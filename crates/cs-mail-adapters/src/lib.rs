//! Explicit edge adapters for SMTP downgrade and future federation.

use core::fmt;
use std::collections::BTreeMap;
use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;

use cs_mail_primitives::{
    CanonicalTime, FederationTransactionRef, Money, ProtocolIdentity, SettlementUnit,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The canonical ASCII DNS name used as a legacy sender identity.
///
/// Unicode names must be converted to their IDNA ASCII form by the vetted
/// DMARC verifier before this value is constructed.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct DomainIdentity(String);

impl DomainIdentity {
    /// Normalizes a DNS name to lowercase without its optional root dot.
    ///
    /// # Errors
    ///
    /// Rejects empty, non-ASCII, oversized, or syntactically invalid names.
    pub fn parse_ascii(input: &str) -> Result<Self, DomainIdentityError> {
        let normalized = input.trim_end_matches('.').to_ascii_lowercase();
        if normalized.is_empty() || normalized.len() > 253 || !input.is_ascii() {
            return Err(DomainIdentityError::InvalidDomain);
        }
        if normalized.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        }) {
            return Err(DomainIdentityError::InvalidDomain);
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_self_or_subdomain_of(&self, parent: &Self) -> bool {
        self == parent
            || self
                .0
                .strip_suffix(parent.as_str())
                .is_some_and(|prefix| prefix.ends_with('.'))
    }

    /// Derives the synthetic protocol identity used consistently for legacy
    /// grant, admission, block, and revocation operations.
    pub fn synthetic_protocol_identity(
        &self,
        deployment_domain: &[u8; 32],
        mapping_version: u16,
    ) -> ProtocolIdentity {
        let mut hasher = Sha256::new();
        hasher.update(b"cs-mail/legacy-domain-identity/v1");
        hasher.update(deployment_domain);
        hasher.update(mapping_version.to_be_bytes());
        hasher.update(self.0.as_bytes());
        let digest = hasher.finalize();
        let mut identity = [0; 16];
        identity.copy_from_slice(&digest[..16]);
        ProtocolIdentity(u128::from_be_bytes(identity))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DomainIdentityError {
    InvalidDomain,
}

impl fmt::Display for DomainIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid canonical ASCII domain")
    }
}

impl std::error::Error for DomainIdentityError {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DmarcAlignment {
    Strict,
    Relaxed,
}

/// One DMARC-aligned authentication mechanism.
///
/// The alignment value is the policy mode under which the mechanism passed.
/// DKIM and SPF are retained independently because a DMARC result may contain
/// either pass, or both passes under different alignment modes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DmarcPass {
    pub alignment: DmarcAlignment,
}

/// Evidence produced by a standards-conforming DMARC implementation.
///
/// The author domain is the RFC5322.From domain. The recipient-granted lane
/// domain is deliberately not part of this verifier output: authentication
/// proves authorized use of the author domain, while the grant separately
/// decides which domain scope the recipient trusts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VerifiedDomain {
    pub author_domain: DomainIdentity,
    pub dkim: Option<DmarcPass>,
    pub spf: Option<DmarcPass>,
    pub evaluated_at: CanonicalTime,
}

impl VerifiedDomain {
    /// Constructs trusted evidence after a DMARC pass.
    ///
    /// # Errors
    ///
    /// Rejects evidence without at least one aligned passing mechanism.
    pub fn passed(
        author_domain: DomainIdentity,
        dkim: Option<DmarcPass>,
        spf: Option<DmarcPass>,
        evaluated_at: CanonicalTime,
    ) -> Result<Self, DmarcEvidenceError> {
        if dkim.is_none() && spf.is_none() {
            return Err(DmarcEvidenceError::NoPassingMechanism);
        }
        Ok(Self {
            author_domain,
            dkim,
            spf,
            evaluated_at,
        })
    }
}

/// Versioned binding between verifier evidence and the recipient's lane scope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LegacyDmarcEvidence {
    pub format_version: u16,
    pub lane_domain: DomainIdentity,
    pub verification: VerifiedDomain,
}

impl LegacyDmarcEvidence {
    pub const FORMAT_VERSION: u16 = 1;

    /// Binds a DMARC-authenticated author to a recipient-granted domain scope.
    ///
    /// # Errors
    ///
    /// Rejects an author outside the granted domain scope.
    pub fn new(
        lane_domain: DomainIdentity,
        verification: VerifiedDomain,
    ) -> Result<Self, DmarcEvidenceError> {
        if !verification
            .author_domain
            .is_self_or_subdomain_of(&lane_domain)
        {
            return Err(DmarcEvidenceError::OutsideLaneDomain);
        }
        Ok(Self {
            format_version: Self::FORMAT_VERSION,
            lane_domain,
            verification,
        })
    }

    pub const fn lane_identity(&self) -> &DomainIdentity {
        &self.lane_domain
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DmarcEvidenceError {
    NoPassingMechanism,
    OutsideLaneDomain,
}

impl fmt::Display for DmarcEvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoPassingMechanism => formatter.write_str("DMARC has no passing mechanism"),
            Self::OutsideLaneDomain => {
                formatter.write_str("DMARC author is outside the granted lane domain")
            }
        }
    }
}

impl std::error::Error for DmarcEvidenceError {}

/// SMTP facts required to evaluate SPF, DKIM, and DMARC correctly.
#[derive(Clone, Copy, Debug)]
pub struct SmtpAuthenticationRequest<'a> {
    pub raw_message: &'a [u8],
    pub remote_ip: IpAddr,
    /// RFC5321 EHLO/HELO domain or address literal.
    pub helo_identity: &'a str,
    /// The complete RFC5321.MailFrom mailbox, or `None` for a null reverse path.
    pub mail_from: Option<&'a str>,
    pub receiver_hostname: &'a str,
    pub received_at: CanonicalTime,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DmarcErrorKind {
    Temporary,
    Failed,
    Malformed,
    ResourceLimit,
}

/// Typed DMARC failure suitable for an SMTP retry/permanent-failure decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DmarcError {
    pub kind: DmarcErrorKind,
    pub detail: String,
}

impl DmarcError {
    pub fn new(kind: DmarcErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    pub const fn is_temporary(&self) -> bool {
        matches!(self.kind, DmarcErrorKind::Temporary)
    }
}

impl fmt::Display for DmarcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for DmarcError {}

pub type DmarcVerificationFuture<'a> =
    Pin<Box<dyn Future<Output = Result<VerifiedDomain, DmarcError>> + Send + 'a>>;

/// Async boundary implemented by a vetted RFC 9989 DMARC library or gateway.
pub trait DmarcVerifier: Send + Sync {
    /// Verifies one message at the canonical receiving boundary.
    ///
    /// # Errors
    ///
    /// Returns a typed error when DMARC cannot produce a pass.
    fn verify<'a>(&'a self, request: SmtpAuthenticationRequest<'a>) -> DmarcVerificationFuture<'a>;
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DowngradeConsent {
    pub accepted_at: CanonicalTime,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DeliveryRoute {
    NativeEncrypted,
    SmtpDowngrade {
        recipient_domain: String,
        consent: DowngradeConsent,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SmtpSubmission {
    pub recipient_address: String,
    pub provider_readable_rfc822: Vec<u8>,
    pub route: DeliveryRoute,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmtpError {
    MissingDowngradeConsent,
    NativeContentNotAccepted,
}

impl fmt::Display for SmtpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDowngradeConsent => {
                formatter.write_str("SMTP delivery requires explicit privacy-downgrade consent")
            }
            Self::NativeContentNotAccepted => {
                formatter.write_str("native ciphertext cannot be submitted as SMTP plaintext")
            }
        }
    }
}

impl std::error::Error for SmtpError {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SmtpReceipt {
    AcceptedByGateway,
}

/// Validates the explicit downgrade boundary. It never claims recipient delivery or reading.
///
/// # Errors
///
/// Returns an error unless the route contains explicit SMTP downgrade consent.
pub fn validate_smtp_submission(submission: &SmtpSubmission) -> Result<SmtpReceipt, SmtpError> {
    match submission.route {
        DeliveryRoute::SmtpDowngrade { .. } => Ok(SmtpReceipt::AcceptedByGateway),
        DeliveryRoute::NativeEncrypted => Err(SmtpError::NativeContentNotAccepted),
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FederationDigest(pub [u8; 32]);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReservationCommitment {
    pub transaction: FederationTransactionRef,
    pub sender_provider: [u8; 32],
    pub recipient_provider: [u8; 32],
    pub command_digest: FederationDigest,
    pub amount: Money,
    pub unit: SettlementUnit,
    pub expires_at: CanonicalTime,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum FederationState {
    Prepared,
    Committed { manifest: FederationDigest },
    Aborted,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FederationRecord {
    pub commitment: ReservationCommitment,
    pub state: FederationState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FederationError {
    DuplicateConflict,
    MissingPrepare,
    PrepareExpired,
    CommandMismatch,
    AlreadyFinal,
}

impl fmt::Display for FederationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateConflict => formatter.write_str("federation transaction conflict"),
            Self::MissingPrepare => formatter.write_str("federation prepare is missing"),
            Self::PrepareExpired => formatter.write_str("federation prepare expired"),
            Self::CommandMismatch => formatter.write_str("commit does not match prepared command"),
            Self::AlreadyFinal => formatter.write_str("federation transaction is already final"),
        }
    }
}

impl std::error::Error for FederationError {}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FederationBook {
    records: BTreeMap<FederationTransactionRef, FederationRecord>,
}

impl FederationBook {
    /// Records an opaque value reservation commitment. Exact replay is idempotent.
    ///
    /// # Errors
    ///
    /// Returns an error when a transaction reference is reused with different facts.
    pub fn prepare(&mut self, commitment: ReservationCommitment) -> Result<(), FederationError> {
        let incoming = FederationRecord {
            commitment,
            state: FederationState::Prepared,
        };
        match self.records.get(&commitment.transaction) {
            Some(existing) if *existing == incoming => Ok(()),
            Some(_) => Err(FederationError::DuplicateConflict),
            None => {
                self.records.insert(commitment.transaction, incoming);
                Ok(())
            }
        }
    }

    /// Commits only the exact prepared command before expiry.
    ///
    /// # Errors
    ///
    /// Returns an error for absent, expired, mismatched, or already final prepares.
    pub fn commit(
        &mut self,
        transaction: FederationTransactionRef,
        command: FederationDigest,
        manifest: FederationDigest,
        now: CanonicalTime,
    ) -> Result<FederationRecord, FederationError> {
        let record = self
            .records
            .get_mut(&transaction)
            .ok_or(FederationError::MissingPrepare)?;
        if record.state != FederationState::Prepared {
            return Err(FederationError::AlreadyFinal);
        }
        if now > record.commitment.expires_at {
            return Err(FederationError::PrepareExpired);
        }
        if command != record.commitment.command_digest {
            return Err(FederationError::CommandMismatch);
        }
        record.state = FederationState::Committed { manifest };
        Ok(*record)
    }

    /// Aborts a prepared transaction without changing any protocol manifest.
    ///
    /// # Errors
    ///
    /// Returns an error for absent or already final prepares.
    pub fn abort(
        &mut self,
        transaction: FederationTransactionRef,
    ) -> Result<FederationRecord, FederationError> {
        let record = self
            .records
            .get_mut(&transaction)
            .ok_or(FederationError::MissingPrepare)?;
        if record.state != FederationState::Prepared {
            return Err(FederationError::AlreadyFinal);
        }
        record.state = FederationState::Aborted;
        Ok(*record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_identity_is_canonical_and_subdomain_aware() {
        let organization = DomainIdentity::parse_ascii("Bank.COM.").unwrap();
        let author = DomainIdentity::parse_ascii("alerts.e.bank.com").unwrap();
        assert_eq!(organization.as_str(), "bank.com");
        assert!(author.is_self_or_subdomain_of(&organization));
        assert!(
            !DomainIdentity::parse_ascii("bank.com.evil.example")
                .unwrap()
                .is_self_or_subdomain_of(&organization)
        );
        assert!(DomainIdentity::parse_ascii("bad_domain.example").is_err());
        assert_eq!(
            organization.synthetic_protocol_identity(&[1; 32], 1),
            DomainIdentity::parse_ascii("BANK.COM")
                .unwrap()
                .synthetic_protocol_identity(&[1; 32], 1)
        );
        assert_ne!(
            organization.synthetic_protocol_identity(&[1; 32], 1),
            organization.synthetic_protocol_identity(&[1; 32], 2)
        );
    }

    #[test]
    fn dmarc_evidence_requires_a_pass_and_the_granted_domain_scope() {
        let author = DomainIdentity::parse_ascii("alerts.bank.com").unwrap();
        assert_eq!(
            VerifiedDomain::passed(author.clone(), None, None, CanonicalTime(1)),
            Err(DmarcEvidenceError::NoPassingMechanism)
        );
        let verification = VerifiedDomain::passed(
            author,
            Some(DmarcPass {
                alignment: DmarcAlignment::Relaxed,
            }),
            None,
            CanonicalTime(1),
        )
        .unwrap();
        assert_eq!(
            LegacyDmarcEvidence::new(
                DomainIdentity::parse_ascii("unrelated.example").unwrap(),
                verification.clone(),
            ),
            Err(DmarcEvidenceError::OutsideLaneDomain)
        );
        let evidence = LegacyDmarcEvidence::new(
            DomainIdentity::parse_ascii("bank.com").unwrap(),
            verification,
        )
        .unwrap();
        assert_eq!(evidence.format_version, LegacyDmarcEvidence::FORMAT_VERSION);
    }

    #[test]
    fn smtp_is_always_an_explicit_downgrade() {
        let submission = SmtpSubmission {
            recipient_address: "bob@example.test".into(),
            provider_readable_rfc822: b"Subject: visible".to_vec(),
            route: DeliveryRoute::NativeEncrypted,
        };
        assert_eq!(
            validate_smtp_submission(&submission),
            Err(SmtpError::NativeContentNotAccepted)
        );
    }

    #[test]
    fn federation_commit_must_match_nonexpired_prepare() {
        let mut book = FederationBook::default();
        let commitment = ReservationCommitment {
            transaction: FederationTransactionRef(1),
            sender_provider: [1; 32],
            recipient_provider: [2; 32],
            command_digest: FederationDigest([3; 32]),
            amount: Money::from_minor_units(10),
            unit: SettlementUnit(1),
            expires_at: CanonicalTime(10),
        };
        book.prepare(commitment).unwrap();
        assert_eq!(
            book.commit(
                FederationTransactionRef(1),
                FederationDigest([4; 32]),
                FederationDigest([5; 32]),
                CanonicalTime(5)
            ),
            Err(FederationError::CommandMismatch)
        );
        assert!(
            book.commit(
                FederationTransactionRef(1),
                FederationDigest([3; 32]),
                FederationDigest([5; 32]),
                CanonicalTime(5)
            )
            .is_ok()
        );
    }
}
