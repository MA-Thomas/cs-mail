//! In-memory persistence for the same enrollment application contract as `PostgreSQL`.
use super::{
    AccountId, CanonicalTime, ConfirmationFailure, DecisionVerifier, EnrollmentInput,
    EnrollmentRepository, Error, PendingEnrollment, SignedDecision,
};
use crate::{EngineError, InMemoryStore};
use std::collections::BTreeMap;

#[derive(Default, Clone)]
pub(crate) struct EnrollmentState {
    pub(crate) configuration: Option<(DecisionVerifier, [u8; 32])>,
    operations: BTreeMap<String, Operation>,
    pub(super) keys:
        BTreeMap<cs_mail_primitives::OperationalKeyRef, cs_mail_accounts::keys::KeyClaim>,
    pub(crate) managed: BTreeMap<AccountId, ManagedAccount>,
}
#[derive(Clone)]
struct Operation {
    pending: PendingEnrollment,
    state: State,
    generation: u64,
}
#[derive(Clone)]
enum State {
    AwaitingEvidence,
    Active {
        decision: Box<SignedDecision>,
        confirmation: Confirmation,
    },
}
#[derive(Clone)]
enum Confirmation {
    Pending(CanonicalTime),
    Leased(CanonicalTime),
    Intervention,
    Confirmed,
}
#[derive(Clone)]
pub struct MemoryAccountRepository {
    store: InMemoryStore,
    clock: std::sync::Arc<dyn super::AccountClock>,
}
impl MemoryAccountRepository {
    /// Pins deployment trust once for this store; cloned handles share it.
    /// # Errors
    /// Rejects reconfiguration or poisoned storage.
    pub fn new(
        store: InMemoryStore,
        verifier: DecisionVerifier,
        bank_key: [u8; 32],
        clock: impl super::AccountClock + 'static,
    ) -> Result<Self, EngineError> {
        {
            let mut world = store.inner.lock().map_err(|_| EngineError::LockPoisoned)?;
            if world.enrollment.configuration.is_some() {
                return Err(Error::Conflict.into());
            }
            world.enrollment.configuration = Some((verifier, bank_key));
        }
        Ok(Self {
            store,
            clock: std::sync::Arc::new(clock),
        })
    }
}
impl EnrollmentRepository for MemoryAccountRepository {
    type Error = EngineError;
    fn reserve(
        &self,
        operation: &str,
        input: &EnrollmentInput,
        decide: impl FnOnce(
            super::enrollment::ReservationContext,
            &dyn super::AccountClock,
        ) -> Result<PendingEnrollment, EngineError>,
    ) -> Result<PendingEnrollment, EngineError> {
        let mut world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let (verifier, bank_key) = world
            .enrollment
            .configuration
            .as_ref()
            .ok_or(Error::Unavailable)?;
        if input.bank.scope != self.store.scope {
            return Err(Error::Context.into());
        }
        let prior = world
            .enrollment
            .operations
            .get(operation)
            .map(|op| op.pending.clone());
        let existed = prior.is_some();
        let pending = decide(
            super::enrollment::ReservationContext {
                prior,
                verifier: verifier.clone(),
                bank_key: *bank_key,
            },
            self.clock.as_ref(),
        )?;
        if !existed {
            if world
                .enrollment
                .operations
                .values()
                .any(|old| input.conflicts_with(old.pending.input()))
                || world.enrollment.keys.contains_key(&input.key_ref)
                || world
                    .enrollment
                    .managed
                    .values()
                    .any(|account| account.state.control.personas().contains(&input.persona))
            {
                return Err(Error::Conflict.into());
            }
            world.enrollment.keys.insert(
                input.key_ref,
                cs_mail_accounts::keys::KeyClaim::Reserved {
                    enrollment: operation.into(),
                    intended_owner: pending.account(),
                    actor: input.actor,
                    key: input.initial_key,
                },
            );
            world.enrollment.operations.insert(
                operation.into(),
                Operation {
                    pending: pending.clone(),
                    state: State::AwaitingEvidence,
                    generation: 0,
                },
            );
        }
        Ok(pending)
    }
    fn renewal(
        &self,
        operation: &str,
        decide: impl FnOnce(
            PendingEnrollment,
            bool,
            &dyn super::AccountClock,
        ) -> Result<PendingEnrollment, EngineError>,
    ) -> Result<PendingEnrollment, EngineError> {
        let mut world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let op = world
            .enrollment
            .operations
            .get_mut(operation)
            .ok_or(Error::Invalid)?;
        let renewed = decide(
            op.pending.clone(),
            !matches!(op.state, State::AwaitingEvidence),
            self.clock.as_ref(),
        )?;
        op.pending = renewed.clone();
        Ok(renewed)
    }
    fn pending(&self, operation: &str) -> Result<PendingEnrollment, EngineError> {
        let world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        Ok(world
            .enrollment
            .operations
            .get(operation)
            .ok_or(Error::Invalid)?
            .pending
            .clone())
    }
    fn activation(
        &self,
        decision: &SignedDecision,
        decide: impl FnOnce(
            super::enrollment::ActivationContext,
            &dyn super::AccountClock,
        ) -> Result<super::enrollment::EnrollmentDecision, EngineError>,
    ) -> Result<AccountId, EngineError> {
        let mut world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let operation = &decision.claims.intent.operation;
        let op = world
            .enrollment
            .operations
            .get(operation)
            .ok_or(Error::Invalid)?;
        let pending = op.pending.clone();
        let prior = match &op.state {
            State::AwaitingEvidence => None,
            State::Active { decision, .. } => Some((**decision).clone()),
        };
        let (verifier, bank_key) = world
            .enrollment
            .configuration
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let key_claim = world
            .enrollment
            .keys
            .get(&pending.input().key_ref)
            .ok_or(Error::Invalid)?
            .clone();
        let (id, records) = decide(
            super::enrollment::ActivationContext {
                pending: pending.clone(),
                prior,
                verifier: verifier.clone(),
                bank_key: *bank_key,
                key_claim,
            },
            self.clock.as_ref(),
        )?
        .into_parts();
        if let Some(records) = records {
            if world
                .product_accounts
                .values()
                .any(|(old, _)| old.binding() == records.product.binding())
                || world.billing_accounts.contains_key(&records.billing.id())
            {
                return Err(Error::Conflict.into());
            }
            let input = pending.input().clone();
            world
                .billing_ledgers
                .insert(records.billing.id(), records.ledger);
            world.funding_sources.insert(
                *records.source.bank_token(),
                (records.billing.id(), records.source),
            );
            world
                .billing_accounts
                .insert(records.billing.id(), records.billing);
            world
                .product_accounts
                .insert(id, (records.product, input.clone()));
            world
                .enrollment
                .keys
                .insert(input.key_ref, records.key_claim);
            world.enrollment.managed.insert(
                id,
                ManagedAccount {
                    state: super::operations::AccountState {
                        control: records.control,
                        registry: records.registry,
                        product: pending.intent().product.clone(),
                    },
                    version: 1,
                    events: BTreeMap::new(),
                    commands: BTreeMap::new(),
                },
            );
            world
                .enrollment
                .operations
                .get_mut(operation)
                .ok_or(Error::Invalid)?
                .state = State::Active {
                decision: Box::new(decision.clone()),
                confirmation: Confirmation::Pending(records.at),
            };
        }
        Ok(id)
    }
    fn claim_confirmations(
        &self,
        limit: u32,
        at: CanonicalTime,
    ) -> Result<Vec<super::ConfirmationClaim>, EngineError> {
        let mut world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let until = CanonicalTime(
            at.0.checked_add(super::CONFIRMATION_LEASE_MILLIS)
                .ok_or(Error::Invalid)?,
        );
        let mut claims = Vec::new();
        for op in world.enrollment.operations.values_mut() {
            if claims.len() >= limit as usize {
                break;
            }
            if let State::Active {
                decision,
                confirmation,
            } = &mut op.state
                && matches!(confirmation, Confirmation::Pending(next) | Confirmation::Leased(next) if *next <= at)
            {
                op.generation = op.generation.checked_add(1).ok_or(Error::Invalid)?;
                *confirmation = Confirmation::Leased(until);
                claims.push(super::ConfirmationClaim {
                    decision: (**decision).clone(),
                    generation: op.generation,
                });
            }
        }
        Ok(claims)
    }
    fn mark_confirmed(&self, claim: &super::ConfirmationClaim) -> Result<(), EngineError> {
        let mut world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let op = world
            .enrollment
            .operations
            .get_mut(claim.operation())
            .ok_or(Error::Invalid)?;
        let State::Active {
            decision,
            confirmation,
        } = &mut op.state
        else {
            return Err(Error::Conflict.into());
        };
        if op.generation != claim.generation
            || **decision != claim.decision
            || !matches!(confirmation, Confirmation::Leased(until) if *until > self.clock.now())
        {
            return Err(Error::Conflict.into());
        }
        *confirmation = Confirmation::Confirmed;
        Ok(())
    }
    fn retry_confirmation(&self, operation: &str, at: CanonicalTime) -> Result<(), EngineError> {
        let mut world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let op = world
            .enrollment
            .operations
            .get_mut(operation)
            .ok_or(Error::Invalid)?;
        let State::Active { confirmation, .. } = &mut op.state else {
            return Err(Error::Conflict.into());
        };
        if matches!(confirmation, Confirmation::Confirmed) {
            return Err(Error::Invalid.into());
        }
        op.generation = op.generation.checked_add(1).ok_or(Error::Invalid)?;
        *confirmation = Confirmation::Pending(at);
        Ok(())
    }
    fn confirmation_failed(
        &self,
        claim: &super::ConfirmationClaim,
        failure: ConfirmationFailure,
    ) -> Result<(), EngineError> {
        let mut world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let op = world
            .enrollment
            .operations
            .get_mut(claim.operation())
            .ok_or(Error::Invalid)?;
        let at = self.clock.now();
        let State::Active { confirmation, .. } = &mut op.state else {
            return Err(Error::Conflict.into());
        };
        if op.generation != claim.generation
            || !matches!(confirmation, Confirmation::Leased(until) if *until > at)
        {
            return Err(Error::Conflict.into());
        }
        {
            *confirmation = match failure {
                ConfirmationFailure::Retryable => Confirmation::Pending(failure.next_attempt(at)?),
                ConfirmationFailure::Intervention(_) => Confirmation::Intervention,
            };
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct ManagedAccount {
    pub state: super::operations::AccountState,
    pub version: u64,
    pub events: BTreeMap<u64, identity_contract::changes::SignedSecurityEvent>,
    pub commands: BTreeMap<
        cs_mail_primitives::IdempotencyKey,
        (
            cs_mail_accounts::control::SignedAccountCommand,
            cs_mail_accounts::control::AccountControl,
        ),
    >,
}
impl super::operations::AccountStore for MemoryAccountRepository {
    type Error = EngineError;
    fn command(
        &self,
        signed: &cs_mail_accounts::control::SignedAccountCommand,
        decide: impl FnOnce(
            super::operations::CommandContext,
        ) -> Result<
            super::operations::AccountDecision<cs_mail_accounts::control::AccountControl>,
            EngineError,
        >,
    ) -> Result<cs_mail_accounts::control::AccountControl, EngineError> {
        let mut world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let mut staged = world.clone();
        let account = staged
            .enrollment
            .managed
            .get(&signed.account)
            .ok_or(Error::Invalid)?;
        let (outcome, effects) = decide(super::operations::CommandContext {
            state: account.state.clone(),
            prior: account.commands.get(&signed.idempotency_key).cloned(),
        })?
        .into_parts();
        if let Some(effects) = effects {
            apply_effects(&mut staged, signed.account, effects)?;
            staged
                .enrollment
                .managed
                .get_mut(&signed.account)
                .ok_or(Error::Invalid)?
                .commands
                .insert(signed.idempotency_key, (signed.clone(), outcome.clone()));
        }
        *world = staged;
        Ok(outcome)
    }
    fn security_change(
        &self,
        signed: &identity_contract::changes::SignedSecurityEvent,
        bank: Option<&cs_mail_finance::BankVerification>,
        decide: impl FnOnce(
            super::operations::SecurityContext,
        ) -> Result<super::operations::AccountDecision<()>, EngineError>,
    ) -> Result<(), EngineError> {
        let mut world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let mut staged = world.clone();
        let id = AccountId(signed.event.account.parse().map_err(|_| Error::Invalid)?);
        let managed = staged.enrollment.managed.get(&id).ok_or(Error::Invalid)?;
        let (product, input) = staged.product_accounts.get(&id).ok_or(Error::Invalid)?;
        let (verifier, authority) = staged
            .enrollment
            .configuration
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let bank = if bank.is_some() {
            Some(super::operations::BankChangeContext {
                account: staged
                    .billing_accounts
                    .get(&product.billing())
                    .ok_or(Error::Invalid)?
                    .clone(),
                authority: *authority,
                maximum_unresolved: input.maximum_unresolved,
            })
        } else {
            None
        };
        let ((), effects) = decide(super::operations::SecurityContext {
            state: managed.state.clone(),
            subject: product.binding().subject_ref().to_owned().try_into()?,
            version: managed.version,
            trust: verifier.clone(),
            prior: managed.events.get(&signed.event.security_version).cloned(),
            bank,
        })?
        .into_parts();
        if let Some(effects) = effects {
            apply_effects(&mut staged, id, effects)?;
            let account = staged
                .enrollment
                .managed
                .get_mut(&id)
                .ok_or(Error::Invalid)?;
            account.version = signed.event.security_version;
            account
                .events
                .insert(signed.event.security_version, signed.clone());
        }
        *world = staged;
        Ok(())
    }
}
fn apply_effects(
    world: &mut crate::StoreState,
    id: AccountId,
    effects: super::operations::AccountEffects,
) -> Result<(), EngineError> {
    let super::operations::AccountEffectRecords {
        state,
        key,
        persona,
        bank,
        authorized_at: _,
    } = effects.into_records();
    if let Some((reference, claim)) = key {
        if world.enrollment.keys.contains_key(&reference) {
            return Err(cs_mail_security::SecurityError::DuplicateKey.into());
        }
        world.enrollment.keys.insert(reference, claim);
    }
    if let Some(persona) = persona
        && (world.enrollment.managed.iter().any(|(owner, account)| {
            *owner != id && account.state.control.personas().contains(&persona)
        }) || world
            .enrollment
            .operations
            .values()
            .any(|op| op.pending.account() != id && op.pending.input().persona == persona))
    {
        return Err(Error::Conflict.into());
    }
    if let Some((financial, source)) = bank {
        let token = *source.bank_token();
        if world
            .enrollment
            .operations
            .values()
            .any(|op| op.pending.account() != id && op.pending.input().bank.bank_token == token)
        {
            return Err(Error::Conflict.into());
        }
        if world
            .funding_sources
            .get(&token)
            .is_some_and(|(owner, _)| *owner != financial.id())
        {
            return Err(Error::Conflict.into());
        }
        world
            .funding_sources
            .entry(token)
            .or_insert((financial.id(), source));
        world.billing_accounts.insert(financial.id(), financial);
    }
    world
        .enrollment
        .managed
        .get_mut(&id)
        .ok_or(Error::Invalid)?
        .state = state;
    Ok(())
}
impl super::IdentitySecurityRepository for MemoryAccountRepository {
    type Error = EngineError;
    fn security_cursor(
        &self,
        account: AccountId,
    ) -> Result<super::IdentitySecurityCursor, EngineError> {
        let world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let (product, _) = world.product_accounts.get(&account).ok_or(Error::Invalid)?;
        let managed = world
            .enrollment
            .managed
            .get(&account)
            .ok_or(Error::Invalid)?;
        Ok(super::IdentitySecurityCursor {
            product: product.binding().product().into(),
            subject: product.binding().subject_ref().to_owned().try_into()?,
            version: managed.version,
        })
    }
}
