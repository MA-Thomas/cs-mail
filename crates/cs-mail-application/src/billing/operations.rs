//! Billing use cases and atomic persistence contracts; no database or provider implementation.
use crate::accounts::{
    AccountClock,
    operations::{AccountFailure, AccountState, OperationConflict},
};
use cs_mail_billing::{
    BillingAccount, BillingCommand, BillingError, ServiceOffer, SignedBillingCommand,
};
use cs_mail_finance::{
    BankVerification, FinancialProgram, FundingSource, PaymentOperation, SignedPaymentEvidence,
};
use cs_mail_ledger::{LedgerBatch, LedgerError, LedgerView};
use cs_mail_primitives::{
    AllocationId, BillingAccountId, CanonicalTime, PaymentOperationId, PolicyVersion,
    ServiceContractId, SettlementUnit,
};
use std::collections::BTreeMap;

pub struct BillingScope {
    pub account: BillingAccountId,
    pub command: Option<cs_mail_primitives::IdempotencyKey>,
    pub offer: Option<PolicyVersion>,
}
pub struct BillingContext {
    pub account: BillingAccount,
    pub product: AccountState,
    pub processor: [u8; 32],
    pub bank_authority: [u8; 32],
    pub offer: Option<ServiceOffer>,
    pub sources: BTreeMap<[u8; 32], FundingSource>,
    pub ledger: LedgerView,
    pub prior: Option<(SignedBillingCommand, Vec<PaymentOperation>)>,
}
#[derive(Clone)]
pub struct CollectionWork {
    pub account: BillingAccountId,
    pub contract: ServiceContractId,
    pub operation: PaymentOperationId,
    pub at: CanonicalTime,
}
#[derive(Clone)]
pub struct CollectionJournal {
    pub receipt: SignedPaymentEvidence,
    pub postings: LedgerBatch,
    pub at: CanonicalTime,
}
pub struct BillingWrites {
    pub account: BillingAccount,
    pub sources: BTreeMap<[u8; 32], FundingSource>,
    pub ledger: LedgerView,
    pub journal: Option<CollectionJournal>,
    pub work: Option<CollectionWork>,
    pub command: Option<(SignedBillingCommand, Vec<PaymentOperation>, CanonicalTime)>,
}
/// Application-only construction prevents treating arbitrary persistence values as a decision.
pub struct BillingDecision<T> {
    outcome: T,
    writes: Option<BillingWrites>,
}
impl<T> BillingDecision<T> {
    pub fn into_parts(self) -> (T, Option<BillingWrites>) {
        (self.outcome, self.writes)
    }
}
impl BillingContext {
    fn source(&mut self, token: &[u8; 32]) -> Result<&mut FundingSource, BillingError> {
        self.sources.get_mut(token).ok_or(BillingError::Payment(
            cs_mail_finance::PaymentError::FundingRestricted,
        ))
    }
    fn writes(self) -> BillingWrites {
        BillingWrites {
            account: self.account,
            sources: self.sources,
            ledger: self.ledger,
            journal: None,
            work: None,
            command: None,
        }
    }
}
pub struct DistributionContext {
    pub program: FinancialProgram,
    pub account: BillingAccount,
}
pub struct DistributionDecision {
    program: FinancialProgram,
    operation: Option<PaymentOperation>,
    at: CanonicalTime,
    changed: bool,
}
impl DistributionDecision {
    pub fn into_parts(
        self,
    ) -> (
        FinancialProgram,
        Option<PaymentOperation>,
        CanonicalTime,
        bool,
    ) {
        (self.program, self.operation, self.at, self.changed)
    }
}
/// Serialize with earlier ingress, account control/revocation and funding changes. Load
/// all context before invoking a callback once; commit account, funding, ledger, journal,
/// command outcome and queued work together. No external I/O is permitted in callbacks.
pub trait BillingStore {
    type Error: AccountFailure + From<LedgerError> + From<cs_mail_finance::ProgramError>;
    /// # Errors
    /// Rolls back the entire operation on rejection, stale state or persistence failure.
    fn billing<T>(
        &self,
        scope: BillingScope,
        decide: impl FnOnce(BillingContext) -> Result<BillingDecision<T>, Self::Error>,
    ) -> Result<T, Self::Error>;
    /// # Errors
    /// Rolls back program updates and payment work together.
    fn distribution(
        &self,
        unit: SettlementUnit,
        allocation: AllocationId,
        decide: impl FnOnce(DistributionContext) -> Result<DistributionDecision, Self::Error>,
    ) -> Result<(), Self::Error>;
}
pub struct BillingService<'a, R, C: ?Sized> {
    repository: &'a R,
    clock: &'a C,
}
impl<'a, R: BillingStore, C: AccountClock + ?Sized> BillingService<'a, R, C> {
    pub fn new(repository: &'a R, clock: &'a C) -> Self {
        Self { repository, clock }
    }
    /// # Errors
    /// Rejects invalid signatures, authority, revisions, service eligibility or funding.
    pub fn execute_command(
        &self,
        signed: &SignedBillingCommand,
    ) -> Result<Vec<PaymentOperation>, R::Error> {
        let offer = if let BillingCommand::PurchaseService { offer } = signed.command {
            Some(offer)
        } else {
            None
        };
        self.repository.billing(
            BillingScope {
                account: signed.account,
                command: Some(signed.idempotency_key),
                offer,
            },
            |mut context| {
                let at = self.clock.now();
                if !context.product.control.is_manager(signed.operational_key) {
                    return Err(BillingError::Conflict.into());
                }
                let (_, key) = context
                    .product
                    .registry
                    .active_actor(signed.operational_key, at)?;
                signed.verify(&key)?;
                if context.account.scope() != signed.scope {
                    return Err(BillingError::Conflict.into());
                }
                if let Some((prior, outcome)) = context.prior.take() {
                    if prior != *signed {
                        return Err(OperationConflict::Duplicate.into());
                    }
                    return Ok(BillingDecision {
                        outcome,
                        writes: None,
                    });
                }
                if signed.command == BillingCommand::Inspect {
                    return Ok(BillingDecision {
                        outcome: Vec::new(),
                        writes: None,
                    });
                }
                if context.account.revision() != signed.expected_revision {
                    return Err(OperationConflict::Version.into());
                }
                let (contract, operation, collect_at) = match signed.command {
                    BillingCommand::Inspect => return Err(BillingError::Conflict.into()),
                    BillingCommand::PurchaseService { .. } => {
                        if !context.product.control.allows_service() {
                            return Err(BillingError::ServiceNotCovered.into());
                        }
                        let offer = context.offer.as_ref().ok_or(BillingError::MissingRecord)?;
                        if at >= offer.period().end() {
                            return Err(BillingError::InvalidSchedule.into());
                        }
                        let contract = context.account.purchase(offer, context.processor)?;
                        (
                            contract,
                            context.account.contracts()[&contract]
                                .collection()
                                .current()
                                .clone(),
                            offer.collect_at(),
                        )
                    }
                    BillingCommand::RetryCollection { contract } => {
                        let operation = context.account.retry_collection(contract)?;
                        (
                            contract,
                            operation,
                            context.account.contracts()[&contract].offer().collect_at(),
                        )
                    }
                };
                context
                    .source(&operation.destination)?
                    .reserve(&operation)
                    .map_err(BillingError::Payment)?;
                let outcome = vec![operation.clone()];
                let mut writes = context.writes();
                writes.work = Some(CollectionWork {
                    account: signed.account,
                    contract,
                    operation: operation.id,
                    at: at.max(collect_at),
                });
                writes.command = Some((signed.clone(), outcome.clone(), at));
                Ok(BillingDecision {
                    outcome,
                    writes: Some(writes),
                })
            },
        )
    }
    /// # Errors
    /// Rejects early dispatch, missing operations or restricted funding.
    pub fn authorize_dispatch(
        &self,
        account: BillingAccountId,
        contract: ServiceContractId,
        operation: PaymentOperationId,
    ) -> Result<Option<PaymentOperation>, R::Error> {
        self.repository.billing(
            BillingScope {
                account,
                command: None,
                offer: None,
            },
            |mut context| {
                let at = self.clock.now();
                let contract = context
                    .account
                    .contracts()
                    .get(&contract)
                    .ok_or(BillingError::MissingRecord)?;
                if at < contract.offer().collect_at() {
                    return Err(BillingError::TooEarly.into());
                }
                let outcome = contract
                    .collection()
                    .pending()
                    .filter(|op| op.id == operation)
                    .cloned();
                if let Some(op) = &outcome {
                    context
                        .source(&op.destination)?
                        .authorize_dispatch(op.id)
                        .map_err(BillingError::Payment)?;
                }
                Ok(BillingDecision {
                    outcome,
                    writes: Some(context.writes()),
                })
            },
        )
    }
    /// # Errors
    /// Rejects unbound evidence and invalid payment, funding or ledger transitions.
    pub fn confirm_payment(
        &self,
        account: BillingAccountId,
        contract: ServiceContractId,
        receipt: &SignedPaymentEvidence,
    ) -> Result<(), R::Error> {
        self.repository.billing(
            BillingScope {
                account,
                command: None,
                offer: None,
            },
            |mut context| {
                let at = self.clock.now();
                let before = context.account.clone();
                let original = context
                    .account
                    .contracts()
                    .get(&contract)
                    .ok_or(BillingError::MissingRecord)?;
                let operation = original
                    .collection()
                    .operation(receipt.evidence.operation_id)
                    .cloned()
                    .ok_or(BillingError::MissingRecord)?;
                let key = *original.processor_key();
                let postings = context.account.record_collection(contract, receipt, at)?;
                context
                    .source(&operation.destination)?
                    .record(receipt, &key)
                    .map_err(BillingError::Payment)?;
                let changed = before != context.account;
                if changed {
                    context.ledger = context.ledger.apply(&postings)?;
                }
                let mut writes = context.writes();
                if changed {
                    writes.journal = Some(CollectionJournal {
                        receipt: receipt.clone(),
                        postings,
                        at,
                    });
                }
                Ok(BillingDecision {
                    outcome: (),
                    writes: Some(writes),
                })
            },
        )
    }
    /// # Errors
    /// Rejects invalid authority, changed identity/bank association or stale evidence.
    pub fn reverify_funding(&self, evidence: &BankVerification) -> Result<(), R::Error> {
        self.repository.billing(
            BillingScope {
                account: evidence.account,
                command: None,
                offer: None,
            },
            |mut context| {
                let bank = evidence
                    .verify(&context.bank_authority)
                    .map_err(BillingError::Payment)?;
                let old = context.account.bank().evidence();
                if old.person != evidence.person
                    || old.bank_token != evidence.bank_token
                    || old.member != evidence.member
                    || context.account.scope() != evidence.scope
                    || context.account.unit() != evidence.unit
                {
                    return Err(BillingError::Conflict.into());
                }
                context
                    .source(&evidence.bank_token)?
                    .reverify(&bank)
                    .map_err(BillingError::Payment)?;
                Ok(BillingDecision {
                    outcome: (),
                    writes: Some(context.writes()),
                })
            },
        )
    }
    /// # Errors
    /// Rejects unavailable allocations, incompatible associations or early payment.
    pub fn prepare_distribution(
        &self,
        unit: SettlementUnit,
        allocation: AllocationId,
    ) -> Result<(), R::Error> {
        self.repository
            .distribution(unit, allocation, |mut context| {
                let at = self.clock.now();
                let before = context.program.revision();
                let operation = super::prepare_distribution(
                    &context.account,
                    &mut context.program,
                    allocation,
                    at,
                )
                .map_err(|e| match e {
                    super::DistributionError::Billing(e) => R::Error::from(e),
                    super::DistributionError::Program(e) => R::Error::from(e),
                })?;
                let changed = before != context.program.revision();
                Ok(DistributionDecision {
                    program: context.program,
                    operation,
                    at,
                    changed,
                })
            })
    }
}
