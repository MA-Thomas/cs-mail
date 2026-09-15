use crate::{FinancialTerms, ProgramError};
use cs_mail_primitives::{
    AnnualDistributionId, CanonicalTime, FinancialEventId, Money, PaymentOperationId,
    SettlementUnit,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Forfeiture {
    pub id: PaymentOperationId,
    pub amount: Money,
    pub unit: SettlementUnit,
    pub forfeited_at: CanonicalTime,
    pub requires_review: bool,
    pub terms: FinancialTerms,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MaturityClearance {
    pub at: CanonicalTime,
    pub evidence: FinancialEventId,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LotLifecycle {
    Pending {
        held: bool,
    },
    Cleared(MaturityClearance),
    Assessed {
        clearance: MaturityClearance,
        distribution: AnnualDistributionId,
    },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ForfeitureLot {
    pub source: Forfeiture,
    pub lifecycle: LotLifecycle,
}
impl ForfeitureLot {
    /// # Errors
    /// Refuses changes to a lot already assessed in a distribution.
    pub fn set_hold(&mut self, held: bool) -> Result<(), ProgramError> {
        if matches!(self.lifecycle, LotLifecycle::Assessed { .. }) {
            return Err(ProgramError::ClosedPeriod);
        }
        self.lifecycle = LotLifecycle::Pending { held };
        Ok(())
    }
    /// # Errors
    /// Refuses held, immature, assessed, or unidentified clearance evidence.
    pub fn clear(
        &mut self,
        evidence: FinancialEventId,
        at: CanonicalTime,
    ) -> Result<(), ProgramError> {
        if matches!(
            self.lifecycle,
            LotLifecycle::Pending { held: true } | LotLifecycle::Assessed { .. }
        ) || evidence.0 == 0
        {
            return Err(ProgramError::InsufficientEvidence);
        }
        let mature = self
            .source
            .forfeited_at
            .checked_add(self.source.terms.maturity_delay)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        if at < mature {
            return Err(ProgramError::TooEarly);
        }
        self.lifecycle = LotLifecycle::Cleared(MaturityClearance { at, evidence });
        Ok(())
    }
    pub fn eligible_at(&self, cutoff: CanonicalTime) -> bool {
        matches!(self.lifecycle, LotLifecycle::Cleared(c) if c.at < cutoff)
    }
    /// # Errors
    /// Requires a cleared lot that has not already been assessed.
    pub fn assess(&mut self, distribution: AnnualDistributionId) -> Result<(), ProgramError> {
        let LotLifecycle::Cleared(clearance) = self.lifecycle else {
            return Err(ProgramError::InsufficientEvidence);
        };
        self.lifecycle = LotLifecycle::Assessed {
            clearance,
            distribution,
        };
        Ok(())
    }
}
