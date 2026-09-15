//! Account actions use an operational key with an explicit account grant.
use cs_mail_billing::{BillingCommand, BillingError, SignedBillingCommand};
use cs_mail_finance::FinancialScope;
use cs_mail_primitives::{BillingAccountId, IdempotencyKey, OperationalKeyRef};
pub struct BillingClient {
    account: BillingAccountId,
    scope: FinancialScope,
    secret: [u8; 32],
    key: OperationalKeyRef,
}
impl BillingClient {
    pub fn new(
        account: BillingAccountId,
        scope: FinancialScope,
        key: OperationalKeyRef,
        secret: [u8; 32],
    ) -> Self {
        Self {
            account,
            scope,
            secret,
            key,
        }
    }
    /// # Errors
    /// Rejects unencodable commands; the service enforces owner/administrator privileges.
    pub fn sign(
        &self,
        revision: u64,
        key: IdempotencyKey,
        command: BillingCommand,
    ) -> Result<SignedBillingCommand, BillingError> {
        SignedBillingCommand::sign(
            self.account,
            self.key,
            self.scope,
            revision,
            key,
            command,
            &self.secret,
        )
    }
}
