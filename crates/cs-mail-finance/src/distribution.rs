use crate::ProgramError;
use cs_mail_primitives::{
    AnnualDistributionId, CanonicalTime, Duration, MemberId, Money, PaymentOperationId,
    PolicyVersion,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EligibilityPolicy {
    pub version: PolicyVersion,
    pub minimum_tenure: Duration,
    pub minimum_active_days: u32,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AnnualDistributionSchedule {
    pub id: AnnualDistributionId,
    pub start: CanonicalTime,
    pub cutoff: CanonicalTime,
    pub eligibility: EligibilityPolicy,
    pub payment: DistributionTerms,
}
impl AnnualDistributionSchedule {
    /// Constructs an exact Gregorian calendar year in UTC milliseconds.
    /// # Errors
    /// Rejects invalid calendar values or eligibility thresholds.
    pub fn utc(
        year: u16,
        eligibility: EligibilityPolicy,
        payment: DistributionTerms,
    ) -> Result<Self, ProgramError> {
        if eligibility.minimum_active_days == 0 {
            return Err(ProgramError::InvalidPolicy);
        }
        let start =
            cs_mail_primitives::calendar_date(year, 1, 1).ok_or(ProgramError::InvalidPolicy)?;
        let cutoff = cs_mail_primitives::calendar_date(
            year.checked_add(1).ok_or(ProgramError::InvalidPolicy)?,
            1,
            1,
        )
        .ok_or(ProgramError::InvalidPolicy)?;
        if payment.due_at < cutoff {
            return Err(ProgramError::InvalidPolicy);
        }
        Ok(Self {
            id: AnnualDistributionId(u128::from(year)),
            start,
            cutoff,
            eligibility,
            payment,
        })
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AnnualAllocation {
    pub schedule: AnnualDistributionSchedule,
    pub finalized_at: CanonicalTime,
    pub members: Vec<MemberId>,
    pub funding: Vec<PaymentOperationId>,
    pub newly_eligible: Money,
    pub corporate_share: Money,
    pub member_contribution: Money,
    pub each: Money,
    pub remainder: Money,
}

impl AnnualDistributionSchedule {
    pub(crate) fn is_calendar_year(&self) -> bool {
        let Ok(year) = u16::try_from(self.id.0) else {
            return false;
        };
        Self::utc(year, self.eligibility.clone(), self.payment.clone())
            .is_ok_and(|expected| expected == *self)
    }
}

/// Published reporting and payment terms, independent of subscription renewal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "StoredDistributionTerms", into = "StoredDistributionTerms")]
pub struct DistributionTerms {
    version: PolicyVersion,
    utility_charge: Money,
    due_at: CanonicalTime,
}
#[derive(Deserialize, Serialize)]
struct StoredDistributionTerms {
    version: PolicyVersion,
    utility_charge: Money,
    due_at: CanonicalTime,
}
impl DistributionTerms {
    /// # Errors
    /// Rejects invalid or inconsistent domain inputs.
    pub fn new(
        version: PolicyVersion,
        utility_charge: Money,
        due_at: CanonicalTime,
    ) -> Result<Self, ProgramError> {
        if version.0 == 0 || utility_charge.is_zero() {
            return Err(ProgramError::InvalidPolicy);
        }
        Ok(Self {
            version,
            utility_charge,
            due_at,
        })
    }
    pub const fn version(&self) -> PolicyVersion {
        self.version
    }
    pub const fn utility_charge(&self) -> Money {
        self.utility_charge
    }
    pub const fn due_at(&self) -> CanonicalTime {
        self.due_at
    }
}
impl TryFrom<StoredDistributionTerms> for DistributionTerms {
    type Error = ProgramError;
    fn try_from(v: StoredDistributionTerms) -> Result<Self, Self::Error> {
        Self::new(v.version, v.utility_charge, v.due_at)
    }
}
impl From<DistributionTerms> for StoredDistributionTerms {
    fn from(v: DistributionTerms) -> Self {
        Self {
            version: v.version,
            utility_charge: v.utility_charge,
            due_at: v.due_at,
        }
    }
}
