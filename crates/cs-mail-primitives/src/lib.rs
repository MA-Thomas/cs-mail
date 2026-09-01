//! Constrained, dependency-free values shared by the cs-mail protocol layers.

use core::fmt;
use serde::{Deserialize, Serialize};

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
id_type!(BondId);
id_type!(PersistenceReserveId);
id_type!(AttemptId);
id_type!(MessageId);
id_type!(EpisodeId);
id_type!(ContentRef);
id_type!(ContentKeyRef);
id_type!(DeliveryIntentRef);
id_type!(OperationalKeyRef);
id_type!(IdempotencyKey);
id_type!(AuditRef);
id_type!(RetentionClassId);
id_type!(FundingRef);
id_type!(FederationTransactionRef);
id_type!(RecoveryFactorRef);
id_type!(RecoveryAttemptRef);
id_type!(LaneId);

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
    AdmissionTimeout(BondId),
    BondExpiry(BondId),
    PersistenceRelease(PersistenceReserveId),
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
}
