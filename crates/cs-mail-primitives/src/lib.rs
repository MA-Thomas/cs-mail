//! Constrained, dependency-free values shared by the cs-mail protocol layers.

use core::fmt;
use serde::{Deserialize, Serialize};

/// An exact Gregorian UTC date at midnight; supported years are 1970 through 9999.
pub fn calendar_date(year: u16, month: u8, day: u8) -> Option<CanonicalTime> {
    if !(1970..=9999).contains(&year) || !(1..=12).contains(&month) {
        return None;
    }
    let leap = |y: u16| y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let months = [
        31_u64,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if day == 0 || u64::from(day) > months[usize::from(month - 1)] {
        return None;
    }
    let years: u64 = (1970..year).map(|y| if leap(y) { 366 } else { 365 }).sum();
    Some(CanonicalTime(
        (years + months[..usize::from(month - 1)].iter().sum::<u64>() + u64::from(day - 1))
            * 86_400_000,
    ))
}

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        pub struct $name(pub u128);

        impl From<u128> for $name {
            fn from(value: u128) -> Self {
                Self(value)
            }
        }
    };
}

id_type!(PrincipalRef);
id_type!(ProtocolIdentity);
id_type!(ProviderRef);
id_type!(QuoteId);
id_type!(RequestId);
id_type!(MessageId);
id_type!(ContentRef);
id_type!(ContentKeyRef);
id_type!(DeliveryIntentRef);
id_type!(OperationalKeyRef);
id_type!(IdempotencyKey);
id_type!(AuditRef);
id_type!(RetentionClassId);
id_type!(FundingRef);
id_type!(PaymentOperationId);
id_type!(MemberId);
id_type!(BillingAccountId);
id_type!(ServiceContractId);
id_type!(ProgramRef);
id_type!(AllocationId);
id_type!(AnnualDistributionId);
id_type!(FinancialEventId);
id_type!(FederationTransactionRef);
id_type!(RecoveryFactorRef);
id_type!(RecoveryAttemptRef);
id_type!(LaneId);
id_type!(ReceiptRef);
id_type!(RetentionRecordRef);

macro_rules! scoped_ref_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        pub struct $name {
            derivation_version: u16,
            bytes: [u8; 32],
        }

        impl $name {
            pub const fn new(derivation_version: u16, bytes: [u8; 32]) -> Self {
                Self {
                    derivation_version,
                    bytes,
                }
            }

            pub const fn derivation_version(self) -> u16 {
                self.derivation_version
            }

            pub const fn as_bytes(&self) -> &[u8; 32] {
                &self.bytes
            }

            pub const fn from_u128_for_test(value: u128) -> Self {
                let mut bytes = [0_u8; 32];
                let value = value.to_be_bytes();
                let mut index = 0;
                while index < 16 {
                    bytes[index + 16] = value[index];
                    index += 1;
                }
                Self::new(0, bytes)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_struct(stringify!($name))
                    .field("derivation_version", &self.derivation_version)
                    .field("value", &"[redacted]")
                    .finish()
            }
        }
    };
}

scoped_ref_type!(RelationshipRef);
scoped_ref_type!(RequestHistoryRef);
scoped_ref_type!(ContentScopeRef);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Money(u64);

impl Money {
    pub const ZERO: Self = Self(0);

    pub const fn from_minor_units(units: u64) -> Self {
        Self(units)
    }

    pub const fn minor_units(self) -> u64 {
        self.0
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        self.0.checked_add(other.0).map(Self)
    }

    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.0.checked_sub(other.0).map(Self)
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SettlementUnit(pub u32);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CanonicalTime(pub u64);

impl CanonicalTime {
    pub fn checked_add(self, duration: Duration) -> Option<Self> {
        self.0.checked_add(duration.0).map(Self)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Duration(pub u64);

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct Version(pub u64);

impl Version {
    pub fn checked_next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ProtocolVersion(pub u16);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PolicyVersion(pub u64);

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct PrivacyProfileVersion(pub u16);

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct RetentionPolicyVersion(pub u16);

macro_rules! version_type {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            Deserialize,
            Eq,
            Hash,
            Ord,
            PartialEq,
            PartialOrd,
            Serialize,
        )]
        pub struct $name(pub u64);

        impl $name {
            pub fn checked_next(self) -> Option<Self> {
                self.0.checked_add(1).map(Self)
            }
        }

        impl From<Version> for $name {
            fn from(value: Version) -> Self {
                Self(value.0)
            }
        }

        impl From<$name> for Version {
            fn from(value: $name) -> Self {
                Self(value.0)
            }
        }
    };
}

version_type!(RelationshipVersion);
version_type!(RequestHistoryVersion);
version_type!(RequestVersion);
version_type!(LaneVersion);
version_type!(AggregateRevision);
version_type!(ContentKeyVersion);

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct WireVersion(pub u16);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum KnownPurpose {
    Personal,
    Transactional,
    Request,
    Bulk,
}

impl KnownPurpose {
    pub const fn code(self) -> u8 {
        match self {
            Self::Personal => 0,
            Self::Transactional => 1,
            Self::Request => 2,
            Self::Bulk => 3,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct NamespacedIdentifier {
    namespace: String,
    name: String,
    version: u16,
}

impl NamespacedIdentifier {
    /// Constructs a bounded, portable extension identifier.
    ///
    /// # Errors
    ///
    /// Returns an error for empty, oversized, non-ASCII, or unversioned values.
    pub fn new(
        namespace: impl Into<String>,
        name: impl Into<String>,
        version: u16,
    ) -> Result<Self, DeclarationError> {
        let value = Self {
            namespace: namespace.into(),
            name: name.into(),
            version,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Validates the bounded identifier grammar.
    ///
    /// # Errors
    ///
    /// Returns [`DeclarationError::InvalidIdentifier`] for invalid components or version zero.
    pub fn validate(&self) -> Result<(), DeclarationError> {
        if self.version == 0
            || !valid_identifier_component(&self.namespace)
            || !valid_identifier_component(&self.name)
        {
            return Err(DeclarationError::InvalidIdentifier);
        }
        Ok(())
    }

    /// Appends the self-delimiting canonical identifier representation.
    ///
    /// # Errors
    ///
    /// Returns an error when the identifier is invalid.
    pub fn append_canonical(&self, bytes: &mut Vec<u8>) -> Result<(), DeclarationError> {
        self.validate()?;
        push_declaration_bytes(bytes, self.namespace.as_bytes())?;
        push_declaration_bytes(bytes, self.name.as_bytes())?;
        bytes.extend_from_slice(&self.version.to_be_bytes());
        Ok(())
    }
}

fn valid_identifier_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
}

fn push_declaration_bytes(target: &mut Vec<u8>, value: &[u8]) -> Result<(), DeclarationError> {
    let length = u16::try_from(value.len()).map_err(|_| DeclarationError::InvalidIdentifier)?;
    target.extend_from_slice(&length.to_be_bytes());
    target.extend_from_slice(value);
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum DeclaredPurpose {
    Unspecified,
    Known(KnownPurpose),
    Extension(NamespacedIdentifier),
}

impl DeclaredPurpose {
    /// Validates an extension purpose identifier when present.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid extension identifier.
    pub fn validate(&self) -> Result<(), DeclarationError> {
        if let Self::Extension(identifier) = self {
            identifier.validate()?;
        }
        Ok(())
    }

    /// Appends the canonical purpose representation.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid extension purpose.
    pub fn append_canonical(&self, bytes: &mut Vec<u8>) -> Result<(), DeclarationError> {
        self.validate()?;
        match self {
            Self::Unspecified => bytes.push(0),
            Self::Known(purpose) => {
                bytes.push(1);
                bytes.push(purpose.code());
            }
            Self::Extension(identifier) => {
                bytes.push(2);
                identifier.append_canonical(bytes)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum OriginMode {
    HumanInitiated,
    AgentDraftHumanApproved,
    AutonomousAgent,
    AutomatedSystem,
    LegacyOrUnspecified,
}

impl OriginMode {
    pub const fn code(self) -> u8 {
        match self {
            Self::HumanInitiated => 0,
            Self::AgentDraftHumanApproved => 1,
            Self::AutonomousAgent => 2,
            Self::AutomatedSystem => 3,
            Self::LegacyOrUnspecified => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum DeclarationAuthority {
    NativeSender(OperationalKeyRef),
    LegacyGateway(ProviderRef),
}

impl DeclarationAuthority {
    pub fn append_canonical(self, bytes: &mut Vec<u8>) {
        match self {
            Self::NativeSender(key) => {
                bytes.push(0);
                bytes.extend_from_slice(&key.0.to_be_bytes());
            }
            Self::LegacyGateway(provider) => {
                bytes.push(1);
                bytes.extend_from_slice(&provider.0.to_be_bytes());
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct OriginDeclaration {
    pub mode: OriginMode,
    pub authority: DeclarationAuthority,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ExtensionCriticality {
    NonCritical,
    Critical,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PayloadSchema {
    pub id: NamespacedIdentifier,
    pub criticality: ExtensionCriticality,
}

impl PayloadSchema {
    /// Validates the schema identifier.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid schema identifier.
    pub fn validate(&self) -> Result<(), DeclarationError> {
        self.id.validate()
    }

    /// Appends the canonical schema representation.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid schema identifier.
    pub fn append_canonical(&self, bytes: &mut Vec<u8>) -> Result<(), DeclarationError> {
        self.validate()?;
        self.id.append_canonical(bytes)?;
        bytes.push(match self.criticality {
            ExtensionCriticality::NonCritical => 0,
            ExtensionCriticality::Critical => 1,
        });
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct MessageDeclarations {
    pub purpose: DeclaredPurpose,
    pub origin: OriginDeclaration,
    pub payload_schema: Option<PayloadSchema>,
}

impl MessageDeclarations {
    /// Validates every declaration and the authority-specific provenance rules.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid identifiers or unsupported legacy-origin claims.
    pub fn validate(&self) -> Result<(), DeclarationError> {
        self.purpose.validate()?;
        if let Some(schema) = &self.payload_schema {
            schema.validate()?;
        }
        if matches!(
            self.origin.authority,
            DeclarationAuthority::LegacyGateway(_)
        ) && self.origin.mode != OriginMode::LegacyOrUnspecified
        {
            return Err(DeclarationError::UnsupportedLegacyOrigin);
        }
        Ok(())
    }

    /// Returns the domain-separated canonical declaration representation.
    ///
    /// # Errors
    ///
    /// Returns an error when any declaration is invalid.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, DeclarationError> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(96);
        bytes.extend_from_slice(b"cs-mail/message-declarations/v1");
        self.purpose.append_canonical(&mut bytes)?;
        bytes.push(self.origin.mode.code());
        self.origin.authority.append_canonical(&mut bytes);
        match &self.payload_schema {
            Some(schema) => {
                bytes.push(1);
                schema.append_canonical(&mut bytes)?;
            }
            None => bytes.push(0),
        }
        Ok(bytes)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct MessageValidityUntil(pub CanonicalTime);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct MessageDeclarationDigest(pub [u8; 32]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeclarationError {
    InvalidIdentifier,
    UnsupportedLegacyOrigin,
}

impl fmt::Display for DeclarationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for DeclarationError {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct JournalPosition(pub u64);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct EventRef(pub JournalPosition);

/// A durable unit of scheduled work shared by protocol subsystems.
///
/// The scheduler owns leasing and retry. Each subsystem owns the meaning of
/// its task variant and the command or state transition materialized from it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ScheduleTask {
    SubmissionTimeout(RequestId),
    RequestExpiry(RequestId),
    LaneHorizon(LaneId),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum ScheduleChange {
    Schedule {
        task: ScheduleTask,
        at: CanonicalTime,
    },
    Cancel {
        task: ScheduleTask,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_and_time_use_checked_arithmetic() {
        assert_eq!(
            Money::from_minor_units(2).checked_add(Money::from_minor_units(3)),
            Some(Money::from_minor_units(5))
        );
        assert_eq!(
            Money::from_minor_units(u64::MAX).checked_add(Money::from_minor_units(1)),
            None
        );
        assert_eq!(CanonicalTime(u64::MAX).checked_add(Duration(1)), None);
    }

    #[test]
    fn declarations_are_canonical_and_legacy_origin_is_honest() {
        let declarations = MessageDeclarations {
            purpose: DeclaredPurpose::Known(KnownPurpose::Transactional),
            origin: OriginDeclaration {
                mode: OriginMode::AutomatedSystem,
                authority: DeclarationAuthority::NativeSender(OperationalKeyRef(7)),
            },
            payload_schema: Some(PayloadSchema {
                id: NamespacedIdentifier::new("org.cs-mail", "invoice", 1).unwrap(),
                criticality: ExtensionCriticality::NonCritical,
            }),
        };
        assert_eq!(
            declarations.canonical_bytes(),
            declarations.canonical_bytes()
        );

        let legacy = MessageDeclarations {
            origin: OriginDeclaration {
                mode: OriginMode::HumanInitiated,
                authority: DeclarationAuthority::LegacyGateway(ProviderRef(8)),
            },
            ..declarations
        };
        assert_eq!(
            legacy.validate(),
            Err(DeclarationError::UnsupportedLegacyOrigin)
        );
    }
}
