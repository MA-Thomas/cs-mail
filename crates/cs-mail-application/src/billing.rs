//! Distribution preparation resolves the beneficiary's bank association, independently of service.
mod memory;
pub mod operations;
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
