//! Distribution preparation resolves the beneficiary's bank association, independently of service.
use cs_mail_billing::{BillingAccount, BillingError};
use cs_mail_finance::{FinancialProgram, PaymentOperation, ProgramError};
use cs_mail_primitives::{AllocationId, CanonicalTime};
#[derive(Debug)]
pub enum DistributionError {
    Billing(BillingError),
    Program(ProgramError),
}
impl From<BillingError> for DistributionError {
    fn from(e: BillingError) -> Self {
        Self::Billing(e)
    }
}
impl From<ProgramError> for DistributionError {
    fn from(e: ProgramError) -> Self {
        Self::Program(e)
    }
}
/// # Errors
/// Rejects missing allocations, wrong account associations, or invalid payment transitions.
pub fn prepare_distribution(
    account: &BillingAccount,
    program: &mut FinancialProgram,
    allocation: AllocationId,
    at: CanonicalTime,
) -> Result<Option<PaymentOperation>, DistributionError> {
    let payable = program
        .records()
        .payables
        .get(&allocation)
        .ok_or(BillingError::MissingRecord)?;
    if payable.member() != account.member()
        || program.scope() != account.scope()
        || program.unit() != account.unit()
    {
        return Err(BillingError::Conflict.into());
    }
    Ok(program.prepare_member_payment(allocation, account.bank(), at)?)
}
impl crate::InMemoryStore {
    /// Trusted fixture/bootstrap boundary; service authentication is exercised by host tests.
    /// # Errors
    /// Rejects invalid bank evidence, duplicate person/account bindings, conflicting aliases, or database failures.
    pub fn register_billing_account(
        &self,
        account: BillingAccount,
    ) -> Result<(), crate::EngineError> {
        if account.scope() != self.scope {
            return Err(crate::EngineError::VersionConflict);
        }
        let mut world = self
            .inner
            .lock()
            .map_err(|_| crate::EngineError::LockPoisoned)?;
        if let Some(old) = world.billing_accounts.get(&account.id()) {
            return if old == &account {
                Ok(())
            } else {
                Err(crate::EngineError::DuplicateConflict)
            };
        }
        if world.billing_accounts.values().any(|a| {
            a.bank().evidence().person == account.bank().evidence().person
                || a.member() == account.member()
        }) {
            return Err(crate::EngineError::DuplicateConflict);
        }
        world.billing_accounts.insert(account.id(), account);
        Ok(())
    }
    /// # Errors
    /// Rejects missing owners, wrong associations, conflicting work, or lock failure.
    pub fn prepare_member_distribution(
        &self,
        account: cs_mail_primitives::BillingAccountId,
        allocation: AllocationId,
        at: CanonicalTime,
    ) -> Result<Option<PaymentOperation>, crate::EngineError> {
        let mut world = self
            .inner
            .lock()
            .map_err(|_| crate::EngineError::LockPoisoned)?;
        let account = world
            .billing_accounts
            .get(&account)
            .cloned()
            .ok_or(crate::EngineError::Billing(BillingError::MissingRecord))?;
        let mut program = world
            .programs
            .get(&account.unit())
            .ok_or(crate::EngineError::Billing(BillingError::MissingRecord))?
            .program
            .clone();
        let operation =
            prepare_distribution(&account, &mut program, allocation, at).map_err(|e| match e {
                DistributionError::Billing(e) => crate::EngineError::Billing(e),
                DistributionError::Program(e) => crate::EngineError::Finance(e),
            })?;
        if let Some(op) = &operation {
            if world.billing_work.get(&op.id).is_some_and(|old| old != op) {
                return Err(crate::EngineError::DuplicateConflict);
            }
            world.billing_work.insert(op.id, op.clone());
        }
        world
            .programs
            .get_mut(&account.unit())
            .ok_or(crate::EngineError::Billing(BillingError::MissingRecord))?
            .program = program;
        Ok(operation)
    }
}
