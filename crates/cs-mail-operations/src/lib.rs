//! The CSQD deployment's fixed identity and trust roots, and the authority of its operators.
//!
//! Bootstrap installs the deployment exactly once: repeating it with identical input
//! changes nothing, and different input is refused. Operators act only through commands
//! signed by an operator key installed at bootstrap. Operator keys are a separate
//! namespace from every user, provider and scheduler key.
use cs_mail_finance::FinancialScope;
use cs_mail_primitives::{IdempotencyKey, OperationalKeyRef, ProviderRef, SettlementUnit};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

mod commands;
mod review;
pub use commands::{OperatorCommand, OperatorOutcome};
pub use review::ReviewCommand;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationsError {
    /// Input violates a bootstrap or command invariant.
    Invalid,
    /// The deployment was bootstrapped with different identity or trust roots.
    BootstrapConflict,
    /// The deployment has not been bootstrapped.
    NotBootstrapped,
    /// Unknown operator key, wrong deployment, or a signature that does not verify.
    Unauthorized,
    /// An operator command identity was reused for a different command.
    DuplicateConflict,
}

impl core::fmt::Display for OperationsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid deployment or operator input",
            Self::BootstrapConflict => "the deployment was bootstrapped differently",
            Self::NotBootstrapped => "the deployment has not been bootstrapped",
            Self::Unauthorized => "operator command is not authorized",
            Self::DuplicateConflict => "operator command identity reused for a different command",
        })
    }
}
impl std::error::Error for OperationsError {}

/// Identifies one operator signing key. Distinct from every protocol operational key.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct OperatorKeyRef(pub u128);

/// The operators' verifying keys: at least one, none zero.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    try_from = "BTreeMap<OperatorKeyRef, [u8; 32]>",
    into = "BTreeMap<OperatorKeyRef, [u8; 32]>"
)]
pub struct OperatorKeys(BTreeMap<OperatorKeyRef, [u8; 32]>);
impl TryFrom<BTreeMap<OperatorKeyRef, [u8; 32]>> for OperatorKeys {
    type Error = OperationsError;
    fn try_from(keys: BTreeMap<OperatorKeyRef, [u8; 32]>) -> Result<Self, Self::Error> {
        if keys.is_empty()
            || keys
                .iter()
                .any(|(r, k)| r.0 == 0 || *k == [0; 32] || VerifyingKey::from_bytes(k).is_err())
        {
            return Err(OperationsError::Invalid);
        }
        Ok(Self(keys))
    }
}
impl From<OperatorKeys> for BTreeMap<OperatorKeyRef, [u8; 32]> {
    fn from(keys: OperatorKeys) -> Self {
        keys.0
    }
}
impl OperatorKeys {
    /// # Errors
    /// Rejects an empty set, a zero reference and invalid or zero keys.
    pub fn new(keys: BTreeMap<OperatorKeyRef, [u8; 32]>) -> Result<Self, OperationsError> {
        keys.try_into()
    }
}

/// Everything fixed when the deployment is created. CSQD is the only provider.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "BootstrapRecord", into = "BootstrapRecord")]
pub struct DeploymentBootstrap(BootstrapRecord);
/// The provider key signs quotes and receipts for CSQD.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProviderKey {
    pub reference: OperationalKeyRef,
    pub verifying_key: [u8; 32],
}
/// Trust in the payment processor's evidence and the bank-verification authority.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PaymentTrust {
    pub processor_key: [u8; 32],
    pub bank_authority: [u8; 32],
}
/// Trust in the identity service's enrollment decisions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IdentityTrust {
    pub issuer: String,
    pub product: String,
    pub decision_key: [u8; 32],
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BootstrapRecord {
    pub provider: ProviderRef,
    pub deployment_domain: [u8; 32],
    pub financial_scope: FinancialScope,
    pub unit: SettlementUnit,
    pub provider_key: ProviderKey,
    pub operators: OperatorKeys,
    pub payment: PaymentTrust,
    pub identity: IdentityTrust,
}
impl TryFrom<BootstrapRecord> for DeploymentBootstrap {
    type Error = OperationsError;
    fn try_from(record: BootstrapRecord) -> Result<Self, Self::Error> {
        let valid_key = |k: &[u8; 32]| *k != [0; 32] && VerifyingKey::from_bytes(k).is_ok();
        if record.provider.0 == 0
            || record.deployment_domain == [0; 32]
            || record.financial_scope.deployment_domain != record.deployment_domain
            || record.financial_scope.operator != record.provider
            || record.provider_key.reference.0 == 0
            || !valid_key(&record.provider_key.verifying_key)
            || !valid_key(&record.payment.processor_key)
            || !valid_key(&record.payment.bank_authority)
            || !valid_key(&record.identity.decision_key)
            || !record.identity.issuer.starts_with("https://")
            || record.identity.product.is_empty()
        {
            return Err(OperationsError::Invalid);
        }
        Ok(Self(record))
    }
}
impl From<DeploymentBootstrap> for BootstrapRecord {
    fn from(value: DeploymentBootstrap) -> Self {
        value.0
    }
}
/// The result of applying a bootstrap to the stored deployment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootstrapOutcome {
    /// Nothing existed; the deployment is created.
    Created,
    /// The identical deployment already exists; nothing changes.
    AlreadyBootstrapped,
}
impl DeploymentBootstrap {
    /// # Errors
    /// Rejects inconsistent scope, provider, keys or identity trust.
    pub fn new(record: BootstrapRecord) -> Result<Self, OperationsError> {
        record.try_into()
    }
    pub const fn record(&self) -> &BootstrapRecord {
        &self.0
    }
    /// Decides a bootstrap against what is stored: creation, an exact replay, or refusal.
    /// A deployment's provider key, operators and trust roots are never replaced here.
    /// # Errors
    /// `BootstrapConflict` when a different deployment is already stored.
    pub fn reconcile(&self, stored: Option<&Self>) -> Result<BootstrapOutcome, OperationsError> {
        match stored {
            None => Ok(BootstrapOutcome::Created),
            Some(existing) if existing == self => Ok(BootstrapOutcome::AlreadyBootstrapped),
            Some(_) => Err(OperationsError::BootstrapConflict),
        }
    }
    /// The authority that verifies this deployment's operator commands.
    pub fn operator_authority(&self) -> OperatorAuthority {
        OperatorAuthority {
            deployment_domain: self.0.deployment_domain,
            keys: self.0.operators.clone(),
        }
    }
}

/// A family of operator commands with its own signing domain, so a signature for one
/// family can never be replayed as another.
pub trait CommandFamily: Serialize + DeserializeOwned + Clone {
    const SIGNING_DOMAIN: &'static str;
    const NAME: &'static str;
}

/// An operator command as submitted: signed over the deployment, key, identity and command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(bound = "C: CommandFamily")]
pub struct SignedOperatorCommand<C> {
    pub deployment_domain: [u8; 32],
    pub operator: OperatorKeyRef,
    pub idempotency_key: IdempotencyKey,
    pub command: C,
    pub signature: Vec<u8>,
}
impl<C: CommandFamily> SignedOperatorCommand<C> {
    fn bytes(&self) -> Result<Vec<u8>, OperationsError> {
        serde_json::to_vec(&(
            C::SIGNING_DOMAIN,
            self.deployment_domain,
            self.operator,
            self.idempotency_key,
            &self.command,
        ))
        .map_err(|_| OperationsError::Invalid)
    }
    /// Signs a command with an operator's secret key.
    /// # Errors
    /// Rejects an unencodable command.
    pub fn sign(
        deployment_domain: [u8; 32],
        operator: OperatorKeyRef,
        idempotency_key: IdempotencyKey,
        command: C,
        secret: &[u8; 32],
    ) -> Result<Self, OperationsError> {
        let mut signed = Self {
            deployment_domain,
            operator,
            idempotency_key,
            command,
            signature: Vec::new(),
        };
        signed.signature = SigningKey::from_bytes(secret)
            .sign(&signed.bytes()?)
            .to_bytes()
            .to_vec();
        Ok(signed)
    }
}

/// A command whose operator signature has been verified. Only [`OperatorAuthority`]
/// constructs it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedOperatorCommand<C> {
    operator: OperatorKeyRef,
    idempotency_key: IdempotencyKey,
    digest: [u8; 32],
    command: C,
}
impl<C> VerifiedOperatorCommand<C> {
    pub const fn operator(&self) -> OperatorKeyRef {
        self.operator
    }
    pub const fn idempotency_key(&self) -> IdempotencyKey {
        self.idempotency_key
    }
    /// Digest of the exact signed bytes; a retry must carry the same digest.
    pub const fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub const fn command(&self) -> &C {
        &self.command
    }
}

/// Verifies operator commands for one deployment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorAuthority {
    deployment_domain: [u8; 32],
    keys: OperatorKeys,
}
impl OperatorAuthority {
    /// # Errors
    /// `Unauthorized` for another deployment, an unknown operator key or a bad signature.
    pub fn verify<C: CommandFamily>(
        &self,
        signed: &SignedOperatorCommand<C>,
    ) -> Result<VerifiedOperatorCommand<C>, OperationsError> {
        if signed.deployment_domain != self.deployment_domain {
            return Err(OperationsError::Unauthorized);
        }
        let key = self
            .keys
            .0
            .get(&signed.operator)
            .ok_or(OperationsError::Unauthorized)?;
        let bytes = signed.bytes()?;
        let signature =
            Signature::from_slice(&signed.signature).map_err(|_| OperationsError::Unauthorized)?;
        VerifyingKey::from_bytes(key)
            .map_err(|_| OperationsError::Unauthorized)?
            .verify_strict(&bytes, &signature)
            .map_err(|_| OperationsError::Unauthorized)?;
        Ok(VerifiedOperatorCommand {
            operator: signed.operator,
            idempotency_key: signed.idempotency_key,
            digest: Sha256::digest([bytes.as_slice(), &signed.signature].concat()).into(),
            command: signed.command.clone(),
        })
    }
}

/// How a verified command relates to the stored record for its identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandReplay<O> {
    /// No command with this identity exists; decide and record it.
    New,
    /// The identical command was already executed; return its recorded outcome.
    Replay(O),
}
/// Checks a verified command against the record stored under its identity.
/// # Errors
/// `DuplicateConflict` when the identity was used for a different command.
pub fn replay<C, O: Clone>(
    command: &VerifiedOperatorCommand<C>,
    stored: Option<(&[u8; 32], &O)>,
) -> Result<CommandReplay<O>, OperationsError> {
    match stored {
        None => Ok(CommandReplay::New),
        Some((digest, outcome)) if *digest == command.digest => {
            Ok(CommandReplay::Replay(outcome.clone()))
        }
        Some(_) => Err(OperationsError::DuplicateConflict),
    }
}
