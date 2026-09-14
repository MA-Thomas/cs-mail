use cs_mail_primitives::{ProgramRef, ProtocolVersion, ProviderRef};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct FinancialScope {
    pub deployment_domain: [u8; 32],
    pub operator: ProviderRef,
    pub program: ProgramRef,
    pub payment_account: [u8; 32],
    pub protocol_version: ProtocolVersion,
}
impl FinancialScope {
    pub const fn new(
        deployment_domain: [u8; 32],
        operator: ProviderRef,
        program: ProgramRef,
        payment_account: [u8; 32],
        protocol_version: ProtocolVersion,
    ) -> Self {
        Self {
            deployment_domain,
            operator,
            program,
            payment_account,
            protocol_version,
        }
    }
    pub fn canonical_bytes(self) -> Vec<u8> {
        let mut bytes = b"cs-mail/financial-scope/v1".to_vec();
        bytes.extend_from_slice(&self.deployment_domain);
        bytes.extend_from_slice(&self.operator.0.to_be_bytes());
        bytes.extend_from_slice(&self.program.0.to_be_bytes());
        bytes.extend_from_slice(&self.payment_account);
        bytes.extend_from_slice(&self.protocol_version.0.to_be_bytes());
        bytes
    }
}
