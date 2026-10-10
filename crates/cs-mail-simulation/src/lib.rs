//! Development support: a durable, nonredeemable payment simulator.
//!
//! It implements the same `PaymentProcessor` port as a real provider, with the rules of
//! `cs_mail_finance::simulation`. Each decision commits together with the directive it
//! used, so a restart can neither repeat an operation nor lose its outcome. Scenario
//! changes are operator-signed commands, audited in the simulator's own tables and never
//! mixed with canonical financial writes. No bank, card network or real money is involved.
use cs_mail_finance::simulation::{self, RefundContext, SimulationDirective, SimulationRecord};
use cs_mail_finance::{
    CaptureCancellation, PaymentError, PaymentKind, PaymentOperation, PaymentOutcome,
    PaymentProcessor, SignedPaymentEvidence,
};
use cs_mail_operations::{
    CommandFamily, CommandReplay, OperationsError, OperatorAuthority, SignedOperatorCommand, replay,
};
use cs_mail_primitives::{CanonicalTime, FinancialEventId, Money, PaymentOperationId};
use ed25519_dalek::SigningKey;
use postgres::types::Json;
use postgres::{Client, NoTls, Transaction};
use serde::{Deserialize, Serialize};

const MIGRATION_1: &str = include_str!("../migrations/0001_simulator.sql");

#[derive(Debug)]
pub enum SimulationError {
    Database(postgres::Error),
    Serialization(serde_json::Error),
    Operations(OperationsError),
    Payment(PaymentError),
}
impl core::fmt::Display for SimulationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Database(e) => write!(f, "simulator database error: {e}"),
            Self::Serialization(e) => write!(f, "simulator record error: {e}"),
            Self::Operations(e) => write!(f, "simulation command refused: {e}"),
            Self::Payment(e) => write!(f, "simulated payment refused: {e}"),
        }
    }
}
impl std::error::Error for SimulationError {}
impl From<postgres::Error> for SimulationError {
    fn from(e: postgres::Error) -> Self {
        Self::Database(e)
    }
}
impl From<serde_json::Error> for SimulationError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e)
    }
}
impl From<OperationsError> for SimulationError {
    fn from(e: OperationsError) -> Self {
        Self::Operations(e)
    }
}
impl From<PaymentError> for SimulationError {
    fn from(e: PaymentError) -> Self {
        Self::Payment(e)
    }
}

/// Operator commands that change the simulated provider's behaviour.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SimulationCommand {
    /// Queue directives for the next new submissions or cancellations, in order.
    QueueDirectives(Vec<SimulationDirective>),
    /// Drop every directive not yet used.
    ClearDirectives,
    /// Resolve a pending operation, emitting a distinct signed event.
    Resolve {
        operation: PaymentOperationId,
        event: FinancialEventId,
        outcome: PaymentOutcome,
    },
}
impl CommandFamily for SimulationCommand {
    const SIGNING_DOMAIN: &'static str = "cs-mail/simulation-command/v1";
    const NAME: &'static str = "simulation";
}
/// The recorded result of a simulation command, returned again on an exact retry.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SimulationOutcome {
    Queued { count: usize },
    Cleared { count: u64 },
    Resolved { operation: PaymentOperationId },
}

/// The durable simulated provider. It signs evidence with its own key, which the
/// deployment trusts as its payment processor.
pub struct PostgresSimulator {
    client: Client,
    key: SigningKey,
}

fn operation_lock(tx: &mut Transaction<'_>, id: PaymentOperationId) -> Result<(), SimulationError> {
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtext($1))",
        &[&format!("cs-sim-operation:{}", id.0)],
    )?;
    Ok(())
}
fn record(
    tx: &mut Transaction<'_>,
    id: PaymentOperationId,
) -> Result<Option<SimulationRecord>, SimulationError> {
    Ok(tx
        .query_opt(
            "SELECT record FROM cs_sim_operations WHERE operation_id=$1 FOR UPDATE",
            &[&id.0.to_string()],
        )?
        .map(|row| row.get::<_, Json<SimulationRecord>>(0).0))
}
fn next_directive(
    tx: &mut Transaction<'_>,
) -> Result<Option<(i64, SimulationDirective)>, SimulationError> {
    Ok(tx
        .query_opt(
            "SELECT seq,directive FROM cs_sim_directives WHERE consumed_by IS NULL \
             ORDER BY seq LIMIT 1 FOR UPDATE",
            &[],
        )?
        .map(|row| (row.get(0), row.get::<_, Json<SimulationDirective>>(1).0)))
}
fn store(
    tx: &mut Transaction<'_>,
    decision: &simulation::SimulationDecision,
    directive: Option<i64>,
    by: PaymentOperationId,
) -> Result<(), SimulationError> {
    if let (true, Some(seq)) = (decision.consumed_directive, directive) {
        tx.execute(
            "UPDATE cs_sim_directives SET consumed_by=$2 WHERE seq=$1",
            &[&seq, &by.0.to_string()],
        )?;
    }
    if let Some(record) = &decision.record {
        let refunds = match record.operation.kind {
            PaymentKind::Refund { capture } => Some(capture.0.to_string()),
            _ => None,
        };
        tx.execute(
            "INSERT INTO cs_sim_operations(operation_id,refunds_capture,record) VALUES($1,$2,$3) \
             ON CONFLICT(operation_id) DO UPDATE SET record=EXCLUDED.record",
            &[&record.operation.id.0.to_string(), &refunds, &Json(record)],
        )?;
    }
    Ok(())
}
fn unavailable(error: &SimulationError) -> PaymentError {
    match error {
        SimulationError::Payment(e) => *e,
        _ => PaymentError::Unavailable,
    }
}

impl PostgresSimulator {
    /// Connects and applies the simulator's own migrations.
    /// # Errors
    /// Returns connection or migration failures.
    pub fn connect(database_url: &str, secret: [u8; 32]) -> Result<Self, SimulationError> {
        let mut client = Client::connect(database_url, NoTls)?;
        let mut tx = client.transaction()?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtext('cs-sim-migrations'))",
            &[],
        )?;
        tx.batch_execute(
            "CREATE TABLE IF NOT EXISTS cs_sim_migrations (version BIGINT PRIMARY KEY)",
        )?;
        if tx
            .query_opt("SELECT 1 FROM cs_sim_migrations WHERE version=1", &[])?
            .is_none()
        {
            tx.batch_execute(MIGRATION_1)?;
        }
        tx.commit()?;
        Ok(Self {
            client,
            key: SigningKey::from_bytes(&secret),
        })
    }
    /// The key the deployment must trust as its payment processor.
    pub fn verifying_key(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }

    fn submit_durably(
        &mut self,
        operation: &PaymentOperation,
    ) -> Result<Result<SignedPaymentEvidence, PaymentError>, SimulationError> {
        let key = self.key.clone();
        let mut tx = self.client.transaction()?;
        operation_lock(&mut tx, operation.id)?;
        let existing = record(&mut tx, operation.id)?;
        let (capture, refunded) = match operation.kind {
            PaymentKind::Refund { capture } => {
                operation_lock(&mut tx, capture)?;
                let refunds = tx
                    .query(
                        "SELECT record FROM cs_sim_operations WHERE refunds_capture=$1",
                        &[&capture.0.to_string()],
                    )?
                    .into_iter()
                    .map(|row| row.get::<_, Json<SimulationRecord>>(0).0)
                    .filter(simulation::refund_counts)
                    .try_fold(Money::ZERO, |sum, r| sum.checked_add(r.operation.amount))
                    .ok_or(PaymentError::InvalidOperation)?;
                (record(&mut tx, capture)?, refunds)
            }
            _ => (None, Money::ZERO),
        };
        let directive = if existing.is_none() {
            next_directive(&mut tx)?
        } else {
            None
        };
        let decision = simulation::submit(
            &key,
            existing.as_ref(),
            &RefundContext {
                capture: capture.as_ref(),
                refunded,
            },
            operation,
            directive.map(|(_, d)| d).unwrap_or_default(),
        );
        store(
            &mut tx,
            &decision,
            directive.map(|(seq, _)| seq),
            operation.id,
        )?;
        tx.commit()?;
        Ok(decision.response)
    }

    fn cancel_durably(
        &mut self,
        request: &CaptureCancellation,
    ) -> Result<Result<SignedPaymentEvidence, PaymentError>, SimulationError> {
        let key = self.key.clone();
        let id = request.operation().id;
        let mut tx = self.client.transaction()?;
        operation_lock(&mut tx, id)?;
        let existing = record(&mut tx, id)?;
        let directive = next_directive(&mut tx)?;
        let decision = simulation::cancel_capture(
            &key,
            existing.as_ref(),
            request,
            directive.map(|(_, d)| d).unwrap_or_default(),
        );
        store(&mut tx, &decision, directive.map(|(seq, _)| seq), id)?;
        tx.commit()?;
        Ok(decision.response)
    }

    /// Verifies and executes one operator scenario command. An exact retry returns the
    /// recorded outcome without a new effect.
    /// # Errors
    /// `Unauthorized` or `DuplicateConflict` from operator verification, a resolution the
    /// simulation rules refuse, and database failures.
    pub fn execute(
        &mut self,
        authority: &OperatorAuthority,
        signed: &SignedOperatorCommand<SimulationCommand>,
        clock: impl FnOnce() -> CanonicalTime,
    ) -> Result<SimulationOutcome, SimulationError> {
        let key = self.key.clone();
        let mut tx = self.client.transaction()?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtext('cs-sim-commands'))",
            &[],
        )?;
        let verified = authority.verify(signed)?;
        let identity = verified.idempotency_key().0.to_string();
        let prior = tx
            .query_opt(
                "SELECT digest,outcome FROM cs_sim_commands WHERE idempotency_key=$1",
                &[&identity],
            )?
            .map(|row| -> Result<_, SimulationError> {
                Ok((
                    <[u8; 32]>::try_from(row.get::<_, Vec<u8>>(0))
                        .map_err(|_| OperationsError::Invalid)?,
                    row.get::<_, Json<SimulationOutcome>>(1).0,
                ))
            })
            .transpose()?;
        if let CommandReplay::Replay(outcome) =
            replay(&verified, prior.as_ref().map(|(d, o)| (d, o)))?
        {
            return Ok(outcome);
        }
        let at = clock();
        let outcome = match verified.command() {
            SimulationCommand::QueueDirectives(directives) => {
                for directive in directives {
                    tx.execute(
                        "INSERT INTO cs_sim_directives(directive,queued_by) VALUES($1,$2)",
                        &[&Json(directive), &identity],
                    )?;
                }
                SimulationOutcome::Queued {
                    count: directives.len(),
                }
            }
            SimulationCommand::ClearDirectives => SimulationOutcome::Cleared {
                count: tx.execute(
                    "DELETE FROM cs_sim_directives WHERE consumed_by IS NULL",
                    &[],
                )?,
            },
            SimulationCommand::Resolve {
                operation,
                event,
                outcome,
            } => {
                operation_lock(&mut tx, *operation)?;
                let current = record(&mut tx, *operation)?.ok_or(PaymentError::InvalidOperation)?;
                let next = simulation::resolve(&key, &current, *event, *outcome)?;
                tx.execute(
                    "UPDATE cs_sim_operations SET record=$2 WHERE operation_id=$1",
                    &[&operation.0.to_string(), &Json(&next)],
                )?;
                SimulationOutcome::Resolved {
                    operation: *operation,
                }
            }
        };
        tx.execute(
            "INSERT INTO cs_sim_commands(idempotency_key,operator,digest,command,outcome,authorized_at) \
             VALUES($1,$2,$3,$4,$5,$6)",
            &[
                &identity,
                &verified.operator().0.to_string(),
                &verified.digest().as_slice(),
                &Json(verified.command()),
                &Json(outcome),
                &i64::try_from(at.0).map_err(|_| OperationsError::Invalid)?,
            ],
        )?;
        tx.commit()?;
        Ok(outcome)
    }
}

impl PaymentProcessor for PostgresSimulator {
    fn lookup(
        &mut self,
        id: PaymentOperationId,
    ) -> Result<Option<SignedPaymentEvidence>, PaymentError> {
        let mut tx = self
            .client
            .transaction()
            .map_err(|_| PaymentError::Unavailable)?;
        let found = record(&mut tx, id).map_err(|e| unavailable(&e))?;
        tx.commit().map_err(|_| PaymentError::Unavailable)?;
        Ok(found.map(|r| r.receipt))
    }
    fn submit(
        &mut self,
        operation: &PaymentOperation,
    ) -> Result<SignedPaymentEvidence, PaymentError> {
        self.submit_durably(operation)
            .map_err(|e| unavailable(&e))?
    }
    fn cancel_capture(
        &mut self,
        request: &CaptureCancellation,
    ) -> Result<SignedPaymentEvidence, PaymentError> {
        self.cancel_durably(request).map_err(|e| unavailable(&e))?
    }
}

/// Development support: a simulated bank that attests account ownership for enrollment.
/// A bank account is named by a label: the same label is the same bank account, whoever
/// enrolls with it. The bank reports the owner the request describes.
pub struct SimulatedBank {
    /// Signs the financial `BankVerification` (the deployment's bank authority).
    authority: [u8; 32],
    /// Signs the identity service's `SignedBankOwnership`.
    ownership: [u8; 32],
}
/// Names a simulated bank account and the owner the bank holds on record for it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SimulatedBankAccount {
    pub account: String,
    pub owner: SimulatedOwner,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SimulatedOwner {
    pub name: String,
    pub address: String,
    pub phone: String,
}
fn label_digest(domain: &str, parts: &[&str]) -> [u8; 32] {
    use sha2::Digest as _;
    let mut hash = sha2::Sha256::new();
    hash.update(domain.as_bytes());
    for part in parts {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    hash.finalize().into()
}
impl SimulatedBank {
    pub const fn new(authority: [u8; 32], ownership: [u8; 32]) -> Self {
        Self {
            authority,
            ownership,
        }
    }
    /// Financial bank verification for backend-allocated billing and member identifiers.
    /// # Errors
    /// Rejects empty labels or evidence the finance domain refuses.
    pub fn verification(
        &self,
        scope: cs_mail_finance::FinancialScope,
        unit: cs_mail_primitives::SettlementUnit,
        billing: cs_mail_primitives::BillingAccountId,
        member: cs_mail_primitives::MemberId,
        account: &SimulatedBankAccount,
    ) -> Result<cs_mail_finance::BankVerification, PaymentError> {
        if account.account.trim().is_empty() {
            return Err(PaymentError::InvalidOperation);
        }
        cs_mail_finance::BankVerification {
            scope,
            account: billing,
            member,
            bank_token: token(account),
            unit,
            version: 1,
            signature: Vec::new(),
        }
        .sign(&self.authority)
    }
    /// Ownership attestation bound to one enrollment attempt and the login that claims it,
    /// reporting the account's owner. The identity service rejects it unless that login is
    /// the one it verified.
    /// # Errors
    /// `Context` when `account` is not the bank account `verification` attests;
    /// otherwise evidence the identity contract cannot sign.
    pub fn ownership(
        &self,
        intent: &identity_contract::EnrollmentIntent,
        account: &SimulatedBankAccount,
        verification: &cs_mail_finance::BankVerification,
        login: (&str, &str),
        now: i64,
    ) -> Result<identity_contract::SignedBankOwnership, identity_contract::Error> {
        if token(account) != verification.bank_token {
            return Err(identity_contract::Error::Context);
        }
        identity_contract::SignedBankOwnership::sign(
            identity_contract::BankOwnershipClaims {
                product: intent.product.clone(),
                operation: intent.operation.clone(),
                challenge: intent.challenge.clone(),
                login_issuer: login.0.into(),
                login_subject: login.1.into(),
                bank_digest: intent.bank_digest,
                account_token: verification.bank_token,
                owners: vec![identity_contract::evidence::ReportedOwner {
                    names: vec![account.owner.name.clone()],
                    addresses: vec![account.owner.address.clone()],
                    phones: vec![account.owner.phone.clone()],
                }],
                evidence_ref: format!("simulated-bank:{}", intent.operation),
                ownership: identity_contract::Ownership::Confirmed,
                issued_at: now,
                expires_at: intent.expires_at,
            },
            &self.ownership,
        )
    }
}
fn token(account: &SimulatedBankAccount) -> [u8; 32] {
    label_digest("cs-mail/simulated-bank-account/v1", &[&account.account])
}
