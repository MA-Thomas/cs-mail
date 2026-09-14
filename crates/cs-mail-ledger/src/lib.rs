//! Conserved journal transfers for request obligations and restricted member funds.
//!
//! Clearing accounts represent the other side of an external or inter-ledger
//! movement and may be negative. Every other account is nonnegative. There are
//! no spendable sender or recipient accounts.
use cs_mail_primitives::{AllocationId, Money, PaymentOperationId, SettlementUnit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Account {
    ProcessorClearing,
    ProgramClearing(PaymentOperationId),
    RequestEscrow(PaymentOperationId),
    RefundPayable(PaymentOperationId),
    ProcessingRevenue,
    PendingForfeiture(PaymentOperationId),
    CorporatePoolRevenue,
    RestrictedMemberFunds,
    MemberPayable(AllocationId),
    CorporateLossClearing,
}
impl Account {
    const fn permits_negative(self) -> bool {
        matches!(
            self,
            Self::ProcessorClearing | Self::ProgramClearing(_) | Self::CorporateLossClearing
        )
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Transfer {
    pub from: Account,
    pub to: Account,
    pub amount: Money,
}
impl Transfer {
    pub const fn new(from: Account, to: Account, amount: Money) -> Self {
        Self { from, to, amount }
    }
}
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct LedgerBatch {
    transfers: Vec<Transfer>,
}
impl LedgerBatch {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn transfer(&mut self, from: Account, to: Account, amount: Money) {
        if !amount.is_zero() {
            self.transfers.push(Transfer::new(from, to, amount));
        }
    }
    pub fn transfers(&self) -> &[Transfer] {
        &self.transfers
    }
    pub fn is_balanced(&self) -> bool {
        self.transfers.iter().all(|t| t.from != t.to)
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LedgerError {
    InsufficientFunds {
        account: Account,
        available: Money,
        required: Money,
    },
    ArithmeticOverflow,
    InvalidBatch,
    VersionConflict,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LedgerView {
    pub revision: u64,
    pub unit: SettlementUnit,
    // A sequence serializes complex account keys without lossy JSON object keys.
    balances: Vec<(Account, i128)>,
}
impl LedgerView {
    pub fn from_balances(
        revision: u64,
        unit: SettlementUnit,
        balances: BTreeMap<Account, i128>,
    ) -> Self {
        Self {
            revision,
            unit,
            balances: balances.into_iter().collect(),
        }
    }
    pub fn balances(&self) -> &[(Account, i128)] {
        &self.balances
    }
    pub fn signed_balance(&self, account: Account) -> i128 {
        self.balances
            .iter()
            .find(|(a, _)| *a == account)
            .map_or(0, |(_, v)| *v)
    }
    /// Nonnegative obligation balance. Use `signed_balance` for clearing accounts.
    pub fn balance(&self, account: Account) -> Money {
        Money::from_minor_units(
            u64::try_from(self.signed_balance(account).max(0)).unwrap_or(u64::MAX),
        )
    }
    /// Applies a complete balanced batch to a copy, with no partial mutation.
    /// # Errors
    /// Rejects invalid transfers, insufficient obligations, and arithmetic overflow.
    pub fn apply(&self, batch: &LedgerBatch) -> Result<Self, LedgerError> {
        self.total_value()?;
        if !batch.is_balanced() {
            return Err(LedgerError::InvalidBatch);
        }
        let mut balances: BTreeMap<_, _> = self.balances.iter().copied().collect();
        for t in batch.transfers() {
            let available = *balances.get(&t.from).unwrap_or(&0);
            let remaining = available
                .checked_sub(i128::from(t.amount.minor_units()))
                .ok_or(LedgerError::ArithmeticOverflow)?;
            if remaining < 0 && !t.from.permits_negative() {
                return Err(LedgerError::InsufficientFunds {
                    account: t.from,
                    available: Money::from_minor_units(
                        u64::try_from(available).map_err(|_| LedgerError::InvalidBatch)?,
                    ),
                    required: t.amount,
                });
            }
            let destination = balances
                .get(&t.to)
                .copied()
                .unwrap_or(0)
                .checked_add(i128::from(t.amount.minor_units()))
                .ok_or(LedgerError::ArithmeticOverflow)?;
            if (!t.to.permits_negative() && destination > i128::from(u64::MAX))
                || remaining < -i128::from(u64::MAX)
            {
                return Err(LedgerError::ArithmeticOverflow);
            }
            balances.insert(t.from, remaining);
            balances.insert(t.to, destination);
        }
        let next = Self::from_balances(
            self.revision
                .checked_add(1)
                .ok_or(LedgerError::ArithmeticOverflow)?,
            self.unit,
            balances,
        );
        next.total_value()?;
        Ok(next)
    }
    /// Checks that all signed journal balances sum to zero.
    /// # Errors
    /// Rejects a nonconserved or overflowing ledger.
    pub fn total_value(&self) -> Result<Money, LedgerError> {
        let mut accounts = std::collections::BTreeSet::new();
        for (account, value) in &self.balances {
            if !accounts.insert(*account) || (!account.permits_negative() && *value < 0) {
                return Err(LedgerError::InvalidBatch);
            }
            if *value > i128::from(u64::MAX) || *value < -i128::from(u64::MAX) {
                return Err(LedgerError::ArithmeticOverflow);
            }
        }
        let sum = self.balances.iter().try_fold(0_i128, |sum, (_, v)| {
            sum.checked_add(*v).ok_or(LedgerError::ArithmeticOverflow)
        })?;
        if sum != 0 {
            return Err(LedgerError::InvalidBatch);
        }
        Ok(Money::ZERO)
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LedgerState {
    view: LedgerView,
}
impl LedgerState {
    pub const fn from_view(view: LedgerView) -> Self {
        Self { view }
    }
    pub fn new(unit: SettlementUnit) -> Self {
        Self {
            view: LedgerView::from_balances(0, unit, BTreeMap::new()),
        }
    }
    pub fn view(&self) -> LedgerView {
        self.view.clone()
    }
    /// # Errors
    /// Rejects stale revisions or invalid journal batches.
    pub fn apply(
        &mut self,
        expected_revision: u64,
        batch: &LedgerBatch,
    ) -> Result<(), LedgerError> {
        if self.view.revision != expected_revision {
            return Err(LedgerError::VersionConflict);
        }
        self.view = self.view.apply(batch)?;
        Ok(())
    }
    pub fn balance(&self, account: Account) -> Money {
        self.view.balance(account)
    }
    /// # Errors
    /// Returns an error if the ledger is not conserved.
    pub fn total_value(&self) -> Result<Money, LedgerError> {
        self.view.total_value()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_batch_is_atomic_and_cannot_spend_refunds_twice() {
        let id = PaymentOperationId(1);
        let account = Account::RefundPayable(id);
        let mut ledger = LedgerState::new(SettlementUnit(1));
        let mut capture = LedgerBatch::new();
        capture.transfer(
            Account::ProcessorClearing,
            account,
            Money::from_minor_units(5),
        );
        ledger.apply(0, &capture).unwrap();
        let before = ledger.clone();
        let mut batch = LedgerBatch::new();
        batch.transfer(
            account,
            Account::ProcessorClearing,
            Money::from_minor_units(4),
        );
        batch.transfer(
            account,
            Account::ProcessorClearing,
            Money::from_minor_units(4),
        );
        assert!(ledger.apply(1, &batch).is_err());
        assert_eq!(ledger, before);
        assert_eq!(ledger.total_value(), Ok(Money::ZERO));
    }
    #[test]
    fn overflow_and_stale_revision_leave_ledger_unchanged() {
        let mut ledger = LedgerState::new(SettlementUnit(1));
        let mut batch = LedgerBatch::new();
        batch.transfer(
            Account::ProcessorClearing,
            Account::ProcessingRevenue,
            Money::from_minor_units(u64::MAX),
        );
        ledger.apply(0, &batch).unwrap();
        let before = ledger.clone();
        assert!(ledger.apply(0, &batch).is_err());
        assert!(ledger.apply(1, &batch).is_err());
        assert_eq!(before, ledger);
    }
}
