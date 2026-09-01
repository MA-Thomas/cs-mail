//! A small double-entry ledger whose batches conserve value by construction.

use std::collections::BTreeMap;

use cs_mail_primitives::{
    BondId, LedgerAccountRef, Money, PersistenceReserveId, ProtocolIdentity, ProviderRef,
    SettlementUnit,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Account {
    Sender(LedgerAccountRef),
    Recipient(ProtocolIdentity),
    RecipientProvider(ProviderRef),
    Bond(BondId),
    PersistenceReserve(PersistenceReserveId),
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
        self.transfers.iter().all(|entry| entry.from != entry.to)
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
    balances: BTreeMap<Account, Money>,
}

impl LedgerView {
    pub fn from_balances(
        revision: u64,
        unit: SettlementUnit,
        balances: BTreeMap<Account, Money>,
    ) -> Self {
        Self {
            revision,
            unit,
            balances,
        }
    }

    pub fn balances(&self) -> &BTreeMap<Account, Money> {
        &self.balances
    }

    pub fn balance(&self, account: Account) -> Money {
        self.balances.get(&account).copied().unwrap_or(Money::ZERO)
    }

    /// Applies a complete batch to a copy of this view.
    ///
    /// # Errors
    ///
    /// Returns an error when the batch is invalid, an account is underfunded,
    /// or checked arithmetic overflows.
    pub fn apply(&self, batch: &LedgerBatch) -> Result<Self, LedgerError> {
        if !batch.is_balanced() {
            return Err(LedgerError::InvalidBatch);
        }
        let mut next = self.clone();
        for transfer in batch.transfers() {
            let available = next.balance(transfer.from);
            let Some(remaining) = available.checked_sub(transfer.amount) else {
                return Err(LedgerError::InsufficientFunds {
                    account: transfer.from,
                    available,
                    required: transfer.amount,
                });
            };
            let destination = next.balance(transfer.to);
            let Some(destination) = destination.checked_add(transfer.amount) else {
                return Err(LedgerError::ArithmeticOverflow);
            };
            next.balances.insert(transfer.from, remaining);
            next.balances.insert(transfer.to, destination);
        }
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or(LedgerError::ArithmeticOverflow)?;
        Ok(next)
    }

    /// Returns the value held by every account.
    ///
    /// # Errors
    ///
    /// Returns an error if summing all balances overflows.
    pub fn total_value(&self) -> Result<Money, LedgerError> {
        self.balances
            .values()
            .try_fold(Money::ZERO, |total, amount| {
                total
                    .checked_add(*amount)
                    .ok_or(LedgerError::ArithmeticOverflow)
            })
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
            view: LedgerView {
                revision: 0,
                unit,
                balances: BTreeMap::new(),
            },
        }
    }

    /// Credits bootstrap value before protocol execution begins.
    ///
    /// This exists for the in-memory reference adapter and its tests; a
    /// production ledger would use an authenticated funding-rail journal entry.
    ///
    /// # Errors
    ///
    /// Returns an error if the resulting balance overflows.
    pub fn fund_for_test(&mut self, account: Account, amount: Money) -> Result<(), LedgerError> {
        let balance = self.view.balance(account);
        let updated = balance
            .checked_add(amount)
            .ok_or(LedgerError::ArithmeticOverflow)?;
        self.view.balances.insert(account, updated);
        Ok(())
    }

    pub fn view(&self) -> LedgerView {
        self.view.clone()
    }

    /// Atomically applies a batch at the expected ledger revision.
    ///
    /// # Errors
    ///
    /// Returns an error for a stale revision, invalid or underfunded batch, or
    /// arithmetic overflow. The ledger remains unchanged on error.
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

    /// Returns the value held by every account.
    ///
    /// # Errors
    ///
    /// Returns an error if summing all balances overflows.
    pub fn total_value(&self) -> Result<Money, LedgerError> {
        self.view.total_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_batch_has_no_partial_effect() {
        let sender = Account::Sender(LedgerAccountRef::from_u128_for_test(1));
        let first = Account::Bond(BondId(1));
        let second = Account::Bond(BondId(2));
        let mut ledger = LedgerState::new(SettlementUnit(1));
        ledger
            .fund_for_test(sender, Money::from_minor_units(5))
            .unwrap();
        let before = ledger.clone();
        let mut batch = LedgerBatch::new();
        batch.transfer(sender, first, Money::from_minor_units(4));
        batch.transfer(sender, second, Money::from_minor_units(4));
        assert!(matches!(
            ledger.apply(0, &batch),
            Err(LedgerError::InsufficientFunds { .. })
        ));
        assert_eq!(ledger, before);
    }

    #[test]
    fn transfers_conserve_total_value() {
        let sender = Account::Sender(LedgerAccountRef::from_u128_for_test(1));
        let bond = Account::Bond(BondId(1));
        let mut ledger = LedgerState::new(SettlementUnit(1));
        ledger
            .fund_for_test(sender, Money::from_minor_units(10))
            .unwrap();
        let total = ledger.total_value().unwrap();
        let mut batch = LedgerBatch::new();
        batch.transfer(sender, bond, Money::from_minor_units(7));
        ledger.apply(0, &batch).unwrap();
        assert_eq!(ledger.total_value().unwrap(), total);
    }
}
