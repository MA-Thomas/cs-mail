use crate::{DAY_MILLIS, ProgramError};
use cs_mail_primitives::{
    CanonicalTime, Duration, MemberId, Money, PaymentOperationId, PolicyVersion, QuarterId,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EligibilityPolicy {
    pub version: PolicyVersion,
    pub minimum_tenure: Duration,
    pub minimum_active_days: u32,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuarterSchedule {
    pub id: QuarterId,
    pub start: CanonicalTime,
    pub cutoff: CanonicalTime,
    pub eligibility: EligibilityPolicy,
}
impl QuarterSchedule {
    /// Constructs an exact Gregorian calendar quarter in UTC milliseconds.
    /// # Errors
    /// Rejects invalid calendar values or eligibility thresholds.
    pub fn utc(
        year: u16,
        quarter: u8,
        eligibility: EligibilityPolicy,
    ) -> Result<Self, ProgramError> {
        if !(1970..=9998).contains(&year)
            || !(1..=4).contains(&quarter)
            || eligibility.minimum_active_days == 0
        {
            return Err(ProgramError::InvalidPolicy);
        }
        let leap =
            |y: u16| y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
        let days_before = |y: u16, m: usize| -> u64 {
            let years: u64 = (1970..y).map(|v| if leap(v) { 366 } else { 365 }).sum();
            let months = [
                31,
                if leap(y) { 29 } else { 28 },
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
            years + months[..m].iter().sum::<u64>()
        };
        let month = usize::from(quarter - 1) * 3;
        let start = days_before(year, month) * DAY_MILLIS;
        let end = if quarter == 4 {
            days_before(year + 1, 0)
        } else {
            days_before(year, month + 3)
        } * DAY_MILLIS;
        Ok(Self {
            id: QuarterId(u128::from(year) * 4 + u128::from(quarter)),
            start: CanonicalTime(start),
            cutoff: CanonicalTime(end),
            eligibility,
        })
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuarterAllocation {
    pub schedule: QuarterSchedule,
    pub finalized_at: CanonicalTime,
    pub members: Vec<MemberId>,
    pub funding: Vec<PaymentOperationId>,
    pub newly_eligible: Money,
    pub corporate_share: Money,
    pub member_contribution: Money,
    pub each: Money,
    pub remainder: Money,
}

impl QuarterSchedule {
    pub(crate) fn is_calendar_quarter(&self) -> bool {
        let Some(index) = self.id.0.checked_sub(1) else {
            return false;
        };
        let Ok(year) = u16::try_from(index / 4) else {
            return false;
        };
        let quarter = u8::try_from(index % 4 + 1).unwrap_or(0);
        Self::utc(year, quarter, self.eligibility.clone()).is_ok_and(|expected| expected == *self)
    }
}
