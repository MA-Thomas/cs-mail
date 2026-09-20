//! Product management decisions run against a protected, database-independent snapshot.
use super::AccountClock;
use cs_mail_accounts::control::{AccountCommand, AccountControl, SignedAccountCommand};
use cs_mail_accounts::keys::{KeyAuthorityOwner, KeyClaim};
use cs_mail_billing::{BillingAccount, BillingError};
use cs_mail_finance::{BankVerification, FundingSource};
use cs_mail_primitives::{AccountId, CanonicalTime, OperationalKeyRef, ProtocolIdentity};
use cs_mail_protocol::ActorRef;
use cs_mail_security::{KeyRegistry, SecurityError};
use identity_contract::{
    self as contract, DecisionVerifier,
    changes::{IdentityChange, SignedSecurityEvent},
};

/// Persistence must translate these semantic conflicts independently of its driver errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationConflict {
    Version,
    Duplicate,
}

pub trait AccountFailure:
    From<contract::Error> + From<SecurityError> + From<BillingError> + From<OperationConflict>
{
}
impl<T> AccountFailure for T where
    T: From<contract::Error> + From<SecurityError> + From<BillingError> + From<OperationConflict>
{
}

#[derive(Clone)]
pub struct AccountState {
    pub control: AccountControl,
    pub registry: KeyRegistry,
    pub product: String,
}
pub struct CommandContext {
    pub state: AccountState,
    pub prior: Option<(SignedAccountCommand, AccountControl)>,
}
pub struct BankChangeContext {
    pub account: BillingAccount,
    pub authority: [u8; 32],
    pub maximum_unresolved: u32,
}
pub struct SecurityContext {
    pub state: AccountState,
    pub subject: contract::ProductSubjectRef,
    pub version: u64,
    pub trust: DecisionVerifier,
    pub prior: Option<SignedSecurityEvent>,
    pub bank: Option<BankChangeContext>,
}
/// Only application evaluation constructs these effects. Adapters persist them without
/// reinterpreting the originating command or identity event.
pub struct AccountEffects {
    state: AccountState,
    key: Option<(OperationalKeyRef, KeyClaim)>,
    persona: Option<ProtocolIdentity>,
    bank: Option<(BillingAccount, FundingSource)>,
    authorized_at: CanonicalTime,
}
pub struct AccountEffectRecords {
    pub state: AccountState,
    pub key: Option<(OperationalKeyRef, KeyClaim)>,
    pub persona: Option<ProtocolIdentity>,
    pub bank: Option<(BillingAccount, FundingSource)>,
    pub authorized_at: CanonicalTime,
}
impl AccountEffects {
    pub fn into_records(self) -> AccountEffectRecords {
        AccountEffectRecords {
            state: self.state,
            key: self.key,
            persona: self.persona,
            bank: self.bank,
            authorized_at: self.authorized_at,
        }
    }
}
/// A recorded replay requires no new writes; a fresh decision carries its complete effects.
pub struct AccountDecision<T> {
    outcome: T,
    effects: Option<AccountEffects>,
}
impl<T> AccountDecision<T> {
    pub fn into_parts(self) -> (T, Option<AccountEffects>) {
        (self.outcome, self.effects)
    }
}
/// Both operations serialize with receipt authorization, key revocation and trust rotation.
/// The adapter loads the complete context before invoking `decide`, holds protection
/// until commit, and atomically persists effects with their receipt/cursor. Unique key,
/// persona and funding ownership must also be enforced for previously absent records.
/// Callbacks execute once, do local computation only, and never perform provider I/O.
pub trait AccountStore {
    type Error: AccountFailure;
    /// # Errors
    /// Rolls back every effect on rejection or persistence conflict.
    fn command(
        &self,
        signed: &SignedAccountCommand,
        decide: impl FnOnce(CommandContext) -> Result<AccountDecision<AccountControl>, Self::Error>,
    ) -> Result<AccountControl, Self::Error>;
    /// # Errors
    /// Rolls back the event, cursor and all account/financial changes together.
    fn security_change(
        &self,
        signed: &SignedSecurityEvent,
        bank: Option<&BankVerification>,
        decide: impl FnOnce(SecurityContext) -> Result<AccountDecision<()>, Self::Error>,
    ) -> Result<(), Self::Error>;
}

pub struct AccountService<'a, R, C: ?Sized> {
    repository: &'a R,
    clock: &'a C,
}
impl<'a, R: AccountStore, C: AccountClock + ?Sized> AccountService<'a, R, C> {
    pub fn new(repository: &'a R, clock: &'a C) -> Self {
        Self { repository, clock }
    }
    /// # Errors
    /// Rejects invalid authority, revisions, ownership or account transitions.
    pub fn execute_command(
        &self,
        signed: &SignedAccountCommand,
    ) -> Result<AccountControl, R::Error> {
        self.repository.command(signed, |context| {
            let mut state = context.state;
            let at = self.clock.now();
            let (_, key) = state.registry.active_actor(signed.operational_key, at)?;
            signed
                .verify(&key)
                .map_err(|_| contract::Error::Unauthorized)?;
            if state.product != signed.product || !state.control.is_manager(signed.operational_key)
            {
                return Err(contract::Error::Unauthorized.into());
            }
            if let Some((prior, outcome)) = context.prior {
                if prior != *signed {
                    return Err(OperationConflict::Duplicate.into());
                }
                return Ok(AccountDecision {
                    outcome,
                    effects: None,
                });
            }
            if state.control.revision() != signed.expected_revision {
                return Err(OperationConflict::Version.into());
            }
            if let AccountCommand::GrantManager(reference) = signed.command {
                state.registry.active_actor(reference, at)?;
            }
            let key = if let AccountCommand::RegisterKey(key) = &signed.command {
                key.verify(signed.account, &signed.product)
                    .map_err(|_| contract::Error::Unauthorized)?;
                state
                    .registry
                    .register(key.reference, key.actor, key.key, at)?;
                Some((
                    key.reference,
                    KeyClaim::Assigned {
                        owner: KeyAuthorityOwner::Account(signed.account),
                        actor: key.actor,
                        key: key.key,
                    },
                ))
            } else {
                None
            };
            state
                .control
                .apply(&signed.command)
                .map_err(|_| contract::Error::Conflict)?;
            let persona = if let AccountCommand::AddPersona(persona) = signed.command {
                Some(persona)
            } else {
                None
            };
            Ok(AccountDecision {
                outcome: state.control.clone(),
                effects: Some(AccountEffects {
                    state,
                    key,
                    persona,
                    bank: None,
                    authorized_at: at,
                }),
            })
        })
    }
    /// # Errors
    /// Rejects substituted, stale or noncontiguous events and invalid financial evidence.
    pub fn apply_identity_change(
        &self,
        signed: &SignedSecurityEvent,
        bank: Option<&BankVerification>,
    ) -> Result<(), R::Error> {
        self.repository.security_change(signed, bank, |context| {
            let mut state = context.state;
            let at = self.clock.now();
            let seconds = i64::try_from(at.0 / 1000).map_err(|_| contract::Error::Invalid)?;
            let verified = context.trust.verify_security_event(signed, seconds)?;
            let event = verified.event();
            if context.subject != event.subject_ref || state.product != event.product {
                return Err(contract::Error::Context.into());
            }
            if let Some(prior) = context.prior {
                if prior != *signed {
                    return Err(OperationConflict::Duplicate.into());
                }
                return Ok(AccountDecision {
                    outcome: (),
                    effects: None,
                });
            }
            if context.version.checked_add(1) != Some(event.security_version) {
                return Err(OperationConflict::Version.into());
            }
            let owner = AccountId(
                event
                    .account
                    .parse()
                    .map_err(|_| contract::Error::Invalid)?,
            );
            let mut key = None;
            let mut financial = None;
            let replacement = match &event.change {
                IdentityChange::RecoverDevice {
                    key_reference,
                    persona,
                } => {
                    let persona =
                        ProtocolIdentity(persona.parse().map_err(|_| contract::Error::Invalid)?);
                    if !state.control.personas().contains(&persona) {
                        return Err(contract::Error::Unauthorized.into());
                    }
                    let reference = OperationalKeyRef(
                        key_reference
                            .parse()
                            .map_err(|_| contract::Error::Invalid)?,
                    );
                    key = Some((
                        reference,
                        KeyClaim::Assigned {
                            owner: KeyAuthorityOwner::Account(owner),
                            actor: ActorRef::Sender(persona),
                            key: event.initial_key,
                        },
                    ));
                    replace_device_key(
                        &mut state.registry,
                        reference,
                        persona,
                        event.initial_key,
                        at,
                    )?;
                    Some(reference)
                }
                IdentityChange::RebindBank => {
                    let bank = bank.ok_or(contract::Error::Invalid)?;
                    let mut context = context.bank.ok_or(contract::Error::Invalid)?;
                    if contract::digest("cs-mail/bank-evidence/v1", bank)? != event.bank_digest
                        || bank.account != context.account.id()
                    {
                        return Err(contract::Error::Context.into());
                    }
                    let verified = bank
                        .verify(&context.authority)
                        .map_err(BillingError::Payment)?;
                    let source = FundingSource::verified(&verified, context.maximum_unresolved)
                        .map_err(BillingError::Payment)?;
                    context.account.rebind_bank(verified)?;
                    financial = Some((context.account, source));
                    None
                }
                IdentityChange::LinkLogin => None,
            };
            state
                .control
                .apply_security_change(replacement)
                .map_err(|_| contract::Error::Invalid)?;
            Ok(AccountDecision {
                outcome: (),
                effects: Some(AccountEffects {
                    state,
                    key,
                    persona: None,
                    bank: financial,
                    authorized_at: at,
                }),
            })
        })
    }
}

fn replace_device_key(
    registry: &mut KeyRegistry,
    reference: OperationalKeyRef,
    persona: ProtocolIdentity,
    key: [u8; 32],
    at: CanonicalTime,
) -> Result<(), SecurityError> {
    let keys: Vec<_> = registry
        .records()
        .map(|key| (key.reference, key.version))
        .collect();
    for (reference, version) in keys {
        registry.revoke(reference, version, at)?;
    }
    registry.register(reference, ActorRef::Sender(persona), key, at)?;
    Ok(())
}
