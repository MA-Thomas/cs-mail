//! Enrollment orchestration over atomic reservation and activation ports.
use super::{
    Account, AccountId, CanonicalTime, DecisionVerifier, EnrollmentInput, EnrollmentRepository,
    Error, PendingEnrollment, SignedDecision, activate,
};
use cs_mail_accounts::{control::AccountControl, keys::KeyClaim};
use cs_mail_billing::{BillingAccount, BillingError};
use cs_mail_finance::FundingSource;
use cs_mail_security::{KeyRegistry, SecurityError};
pub struct ReservationContext {
    pub prior: Option<PendingEnrollment>,
    pub verifier: DecisionVerifier,
    pub bank_key: [u8; 32],
}
pub struct ActivationContext {
    pub pending: PendingEnrollment,
    pub prior: Option<SignedDecision>,
    pub verifier: DecisionVerifier,
    pub bank_key: [u8; 32],
    pub key_claim: KeyClaim,
}
pub struct EnrollmentRecords {
    pub product: Account,
    pub billing: BillingAccount,
    pub source: FundingSource,
    pub control: AccountControl,
    pub registry: KeyRegistry,
    pub key_claim: KeyClaim,
    pub at: CanonicalTime,
    pub membership_identity: [u8; 32],
    pub ledger: cs_mail_ledger::LedgerView,
}
pub struct EnrollmentDecision {
    account: AccountId,
    records: Option<EnrollmentRecords>,
}
impl EnrollmentDecision {
    pub fn into_parts(self) -> (AccountId, Option<EnrollmentRecords>) {
        (self.account, self.records)
    }
}
pub struct AccountEnrollment<'a, R> {
    repository: &'a R,
}
impl<'a, R: EnrollmentRepository> AccountEnrollment<'a, R> {
    pub fn new(repository: &'a R) -> Self {
        Self { repository }
    }
    /// # Errors
    /// Rejects changed retries, invalid bank evidence and conflicting ownership.
    pub fn begin(
        &self,
        operation: &str,
        input: &EnrollmentInput,
    ) -> Result<PendingEnrollment, R::Error> {
        self.repository.reserve(operation, input, |context, clock| {
            if let Some(prior) = context.prior {
                if prior.input() != input {
                    return Err(Error::Conflict.into());
                }
                return Ok(prior);
            }
            input
                .bank
                .verify(&context.bank_key)
                .map_err(BillingError::Payment)?;
            let now = i64::try_from(clock.now().0 / 1000).map_err(|_| Error::Invalid)?;
            Ok(PendingEnrollment::new(
                context.verifier.product(),
                operation,
                input.clone(),
                now,
            )?)
        })
    }
    /// # Errors
    /// Rejects committed or still-live attempts without reallocating ownership.
    pub fn renew(&self, operation: &str) -> Result<PendingEnrollment, R::Error> {
        self.repository
            .renewal(operation, |pending, committed, clock| {
                if committed {
                    return Err(Error::Conflict.into());
                }
                let now = i64::try_from(clock.now().0 / 1000).map_err(|_| Error::Invalid)?;
                Ok(pending.renew(now)?)
            })
    }
    /// # Errors
    /// Rejects stale authority or inconsistent ownership; activation is all-or-nothing.
    pub fn commit(&self, decision: &SignedDecision) -> Result<AccountId, R::Error> {
        self.repository.activation(decision, |context, clock| {
            let p = context.pending;
            if let Some(prior) = context.prior {
                if prior != *decision {
                    return Err(Error::Conflict.into());
                }
                return Ok(EnrollmentDecision {
                    account: p.account(),
                    records: None,
                });
            }
            let at = clock.now();
            let product = activate(&p, decision, &context.verifier, at)?;
            let expected = KeyClaim::Reserved {
                enrollment: p.intent().operation.clone(),
                intended_owner: p.account(),
                actor: p.input().actor,
                key: p.input().initial_key,
            };
            if context.key_claim != expected {
                return Err(SecurityError::DuplicateKey.into());
            }
            let key_claim = context
                .key_claim
                .activate(&p.intent().operation, p.account())
                .map_err(|_| SecurityError::ActorMismatch)?;
            let bank = p
                .input()
                .bank
                .verify(&context.bank_key)
                .map_err(BillingError::Payment)?;
            let source = FundingSource::verified(&bank, p.input().maximum_unresolved)
                .map_err(BillingError::Payment)?;
            let billing = BillingAccount::new(bank);
            let control = AccountControl::new(p.input().persona, p.input().key_ref)
                .map_err(|_| Error::Invalid)?;
            let mut registry = KeyRegistry::default();
            registry.register(
                p.input().key_ref,
                p.input().actor,
                p.input().initial_key,
                at,
            )?;
            let membership_identity =
                identity_contract::digest("cs-mail/member-identity/v1", &product.principal())?;
            let ledger = cs_mail_ledger::LedgerState::new(billing.unit()).view();
            Ok(EnrollmentDecision {
                account: product.id(),
                records: Some(EnrollmentRecords {
                    product,
                    billing,
                    source,
                    control,
                    registry,
                    key_claim,
                    at,
                    membership_identity,
                    ledger,
                }),
            })
        })
    }
}
