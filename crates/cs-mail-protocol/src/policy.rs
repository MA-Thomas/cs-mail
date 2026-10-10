//! CSQD's protocol timing and member-program policy, published by operators.
//!
//! The deployment fixes who the provider is, its settlement unit, financial scope and
//! payment authority at bootstrap. The operator publishes, as versions, the rules that
//! may change over time: the request windows, cooldowns and retry backoff, and the
//! member program's corporate share and maturity. A [`PolicySnapshot`] combines both.
use crate::{PolicySnapshot, ProtocolError};
use cs_mail_finance::{FinancialScope, FinancialTerms};
use cs_mail_primitives::{
    Duration, PolicyVersion, PrivacyProfileVersion, ProtocolVersion, ProviderRef,
    RetentionPolicyVersion, SettlementUnit,
};
use serde::{Deserialize, Serialize};

/// Largest accepted number of backoff steps.
pub const MAX_BACKOFF_STEPS: usize = 16;

/// One operator-published version of the protocol policy.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(try_from = "ProtocolPolicyRecord", into = "ProtocolPolicyRecord")]
pub struct ProtocolPolicy(ProtocolPolicyRecord);

/// The published fields of a [`ProtocolPolicy`], as they are signed and stored.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolPolicyRecord {
    pub version: PolicyVersion,
    pub privacy_profile_version: PrivacyProfileVersion,
    pub retention_policy_version: RetentionPolicyVersion,
    /// From request creation until the initial message must be submitted.
    pub submission_window: Duration,
    /// From submission until the recipient must decide.
    pub decision_window: Duration,
    /// How long issued request terms remain usable.
    pub quote_lifetime: Duration,
    /// Waiting time before another request to the same recipient, by request level. The
    /// first step is zero and steps never decrease.
    pub backoff: Vec<Duration>,
    /// Waiting time after an expired request.
    pub expiry_cooldown: Duration,
    /// Waiting time after a rejected request.
    pub rejection_cooldown: Duration,
    /// CSQD's share of forfeited collateral, in basis points.
    pub corporate_basis_points: u16,
    /// Time before a forfeiture is eligible for allocation.
    pub maturity_delay: Duration,
}

impl TryFrom<ProtocolPolicyRecord> for ProtocolPolicy {
    type Error = String;
    fn try_from(record: ProtocolPolicyRecord) -> Result<Self, Self::Error> {
        Self::new(record).map_err(|e| format!("{e:?}"))
    }
}
impl ProtocolPolicy {
    /// # Errors
    /// `PolicyInvalid` for version zero, a zero window, an empty, oversized, nonzero-first
    /// or decreasing backoff, or a corporate share above the whole.
    pub fn new(record: ProtocolPolicyRecord) -> Result<Self, ProtocolError> {
        let positive = [
            record.submission_window,
            record.decision_window,
            record.quote_lifetime,
        ];
        if record.version.0 == 0
            || positive.iter().any(|d| d.0 == 0)
            || record.backoff.is_empty()
            || record.backoff.len() > MAX_BACKOFF_STEPS
            || record.backoff[0] != Duration(0)
            || !record.backoff.windows(2).all(|pair| pair[0] <= pair[1])
            || record.corporate_basis_points > 10_000
        {
            return Err(ProtocolError::PolicyInvalid);
        }
        Ok(Self(record))
    }
}
impl From<ProtocolPolicy> for ProtocolPolicyRecord {
    fn from(policy: ProtocolPolicy) -> Self {
        policy.0
    }
}

/// What the deployment fixed at bootstrap and every policy version shares.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeploymentTerms {
    pub protocol_version: ProtocolVersion,
    pub provider: ProviderRef,
    pub unit: SettlementUnit,
    pub financial_scope: FinancialScope,
    pub payment_provider_key: [u8; 32],
}

impl ProtocolPolicy {
    pub const fn version(&self) -> PolicyVersion {
        self.0.version
    }
    pub const fn record(&self) -> &ProtocolPolicyRecord {
        &self.0
    }
    /// The complete policy the kernel applies under this version and deployment.
    /// # Errors
    /// `PolicyInvalid` when the deployment's financial scope names another protocol
    /// version or its payment key is zero.
    pub fn snapshot(&self, deployment: DeploymentTerms) -> Result<PolicySnapshot, ProtocolError> {
        if deployment.financial_scope.protocol_version != deployment.protocol_version
            || deployment.payment_provider_key == [0; 32]
        {
            return Err(ProtocolError::PolicyInvalid);
        }
        let record = &self.0;
        let snapshot = PolicySnapshot {
            protocol_version: deployment.protocol_version,
            policy_version: record.version,
            privacy_profile_version: record.privacy_profile_version,
            retention_policy_version: record.retention_policy_version,
            recipient_provider: deployment.provider,
            unit: deployment.unit,
            submission_window: record.submission_window,
            decision_window: record.decision_window,
            quote_lifetime: record.quote_lifetime,
            backoff: record.backoff.clone(),
            financial: FinancialTerms {
                scope: deployment.financial_scope,
                policy_version: record.version,
                corporate_basis_points: record.corporate_basis_points,
                maturity_delay: record.maturity_delay,
            },
            payment_provider_key: deployment.payment_provider_key,
            expiry_cooldown: record.expiry_cooldown,
            rejection_cooldown: record.rejection_cooldown,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }
}
