//! Transactional in-memory execution for the deterministic protocol kernel.
//!
//! This adapter is deliberately small: it demonstrates atomic state, ledger,
//! schedule, journal, outbox, and idempotency behavior without pretending to be
//! a production database.

use std::collections::BTreeMap;
use std::sync::Mutex;

use cs_mail_ledger::{Account, LedgerError, LedgerState};
use cs_mail_primitives::{
    CanonicalTime, IdempotencyKey, JournalPosition, Money, OperationalKeyRef, PrincipalRef,
    ProtocolIdentity, SettlementUnit,
};
use cs_mail_protocol::{
    ActorRef, Authorized, EffectIntent, PolicySnapshot, ProtocolCommand, ProtocolError,
    ProtocolEvent, ProtocolState, ScheduleChange, ScheduleTask, SettlementSnapshot,
    TransitionContext, TransitionManifest, transition,
};
use cs_mail_security::{KeyRegistry, SecurityError, SignedCommandBytes, SigningScope};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineError {
    Protocol(ProtocolError),
    Ledger(LedgerError),
    DuplicateConflict,
    VersionConflict,
    LockPoisoned,
    ArithmeticOverflow,
    Security(SecurityError),
}

impl From<ProtocolError> for EngineError {
    fn from(value: ProtocolError) -> Self {
        Self::Protocol(value)
    }
}

impl From<LedgerError> for EngineError {
    fn from(value: LedgerError) -> Self {
        Self::Ledger(value)
    }
}

impl From<SecurityError> for EngineError {
    fn from(value: SecurityError) -> Self {
        Self::Security(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionOutcome {
    pub manifest: TransitionManifest,
    pub replayed: bool,
}

#[derive(Clone)]
struct ExecutionRecord {
    actor: ActorRef,
    operational_key: OperationalKeyRef,
    command: ProtocolCommand,
    manifest: TransitionManifest,
}

struct EngineState {
    revision: u64,
    protocol: ProtocolState,
    ledger: LedgerState,
    journal: Vec<ProtocolEvent>,
    schedules: BTreeMap<ScheduleTask, CanonicalTime>,
    outbox: Vec<EffectIntent>,
    idempotency: BTreeMap<IdempotencyKey, ExecutionRecord>,
    next_journal_position: u64,
}

pub struct InMemoryEngine {
    inner: Mutex<EngineState>,
}

impl InMemoryEngine {
    /// Creates an isolated relationship engine with a funded sender account.
    ///
    /// # Errors
    ///
    /// Returns an error if the bootstrap balance cannot be represented.
    pub fn new(
        protocol: ProtocolState,
        unit: SettlementUnit,
        sender_balance: Money,
    ) -> Result<Self, EngineError> {
        let mut ledger = LedgerState::new(unit);
        ledger.fund_for_test(
            Account::Sender(protocol.attempt.sender_account),
            sender_balance,
        )?;
        Ok(Self {
            inner: Mutex::new(EngineState {
                revision: 0,
                protocol,
                ledger,
                journal: Vec::new(),
                schedules: BTreeMap::new(),
                outbox: Vec::new(),
                idempotency: BTreeMap::new(),
                next_journal_position: 1,
            }),
        })
    }

    /// Linearizes and atomically commits one authorized command.
    ///
    /// # Errors
    ///
    /// Returns an error when protocol, ledger, concurrency, idempotency, or
    /// arithmetic preconditions fail, or if the engine lock is poisoned.
    pub fn execute(
        &self,
        authorized: Authorized<ProtocolCommand>,
        now: CanonicalTime,
        policy: PolicySnapshot,
    ) -> Result<ExecutionOutcome, EngineError> {
        let mut inner = self.inner.lock().map_err(|_| EngineError::LockPoisoned)?;
        let key = authorized.idempotency_key();
        if let Some(record) = inner.idempotency.get(&key) {
            if record.actor == authorized.actor()
                && record.operational_key == authorized.operational_key()
                && &record.command == authorized.command()
            {
                return Ok(ExecutionOutcome {
                    manifest: record.manifest.clone(),
                    replayed: true,
                });
            }
            return Err(EngineError::DuplicateConflict);
        }

        let snapshot = SettlementSnapshot::complete(
            inner.revision,
            inner.protocol.clone(),
            inner.ledger.view(),
        );
        let context = TransitionContext {
            now,
            journal_position: JournalPosition(inner.next_journal_position),
            protocol_version: policy.protocol_version,
            policy,
        };
        let manifest = transition(&snapshot, &authorized, &context)?;
        if manifest.expected_snapshot_revision != inner.revision {
            return Err(EngineError::VersionConflict);
        }
        let next_revision = inner
            .revision
            .checked_add(1)
            .ok_or(EngineError::ArithmeticOverflow)?;
        let next_position = inner
            .next_journal_position
            .checked_add(1)
            .ok_or(EngineError::ArithmeticOverflow)?;

        // The ledger applies to a temporary clone first. No protocol projection
        // changes unless the complete financial batch succeeds.
        let mut next_ledger = inner.ledger.clone();
        next_ledger.apply(manifest.expected_ledger_revision, &manifest.ledger_batch)?;

        let (command, actor, operational_key, _) = authorized.into_parts();
        let record = ExecutionRecord {
            actor,
            operational_key,
            command,
            manifest: manifest.clone(),
        };

        inner.protocol = manifest.next_state.clone();
        inner.ledger = next_ledger;
        apply_schedule_changes(&mut inner.schedules, &manifest.schedule_changes);
        inner
            .journal
            .extend(manifest.protocol_events.iter().copied());
        inner.outbox.extend(manifest.outbox_intents.iter().copied());
        inner.idempotency.insert(key, record);
        inner.revision = next_revision;
        inner.next_journal_position = next_position;

        Ok(ExecutionOutcome {
            manifest,
            replayed: false,
        })
    }

    /// Verifies and executes one signed command at its canonical receipt time.
    ///
    /// # Errors
    ///
    /// Returns an error when signature/key validation or normal execution
    /// preconditions fail.
    pub fn execute_signed(
        &self,
        registry: &KeyRegistry,
        signed: &SignedCommandBytes,
        deployment_domain: [u8; 32],
        now: CanonicalTime,
        policy: PolicySnapshot,
    ) -> Result<ExecutionOutcome, EngineError> {
        let verified = registry.verify(
            signed,
            now,
            policy.protocol_version,
            SigningScope {
                deployment_domain,
                intended_provider: policy.recipient_provider,
                relationship: self.snapshot()?.state.relationship.key.reference,
            },
        )?;
        self.execute(verified.into_authorized(), now, policy)
    }

    /// Returns a complete point-in-time settlement snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if the engine lock is poisoned.
    pub fn snapshot(&self) -> Result<SettlementSnapshot, EngineError> {
        let inner = self.inner.lock().map_err(|_| EngineError::LockPoisoned)?;
        Ok(SettlementSnapshot::complete(
            inner.revision,
            inner.protocol.clone(),
            inner.ledger.view(),
        ))
    }

    /// Returns one projected ledger balance.
    ///
    /// # Errors
    ///
    /// Returns an error if the engine lock is poisoned.
    pub fn balance(&self, account: Account) -> Result<Money, EngineError> {
        let inner = self.inner.lock().map_err(|_| EngineError::LockPoisoned)?;
        Ok(inner.ledger.balance(account))
    }

    /// Returns the conserved value across all ledger accounts.
    ///
    /// # Errors
    ///
    /// Returns an error if the lock is poisoned or balance summation overflows.
    pub fn total_value(&self) -> Result<Money, EngineError> {
        let inner = self.inner.lock().map_err(|_| EngineError::LockPoisoned)?;
        Ok(inner.ledger.total_value()?)
    }

    /// Returns the canonical protocol event journal.
    ///
    /// # Errors
    ///
    /// Returns an error if the engine lock is poisoned.
    pub fn journal(&self) -> Result<Vec<ProtocolEvent>, EngineError> {
        let inner = self.inner.lock().map_err(|_| EngineError::LockPoisoned)?;
        Ok(inner.journal.clone())
    }

    /// Returns durable post-commit effect intents.
    ///
    /// # Errors
    ///
    /// Returns an error if the engine lock is poisoned.
    pub fn outbox(&self) -> Result<Vec<EffectIntent>, EngineError> {
        let inner = self.inner.lock().map_err(|_| EngineError::LockPoisoned)?;
        Ok(inner.outbox.clone())
    }

    /// Returns currently scheduled protocol work.
    ///
    /// # Errors
    ///
    /// Returns an error if the engine lock is poisoned.
    pub fn schedules(&self) -> Result<BTreeMap<ScheduleTask, CanonicalTime>, EngineError> {
        let inner = self.inner.lock().map_err(|_| EngineError::LockPoisoned)?;
        Ok(inner.schedules.clone())
    }
}

fn apply_schedule_changes(
    schedules: &mut BTreeMap<ScheduleTask, CanonicalTime>,
    changes: &[ScheduleChange],
) {
    for change in changes {
        match *change {
            ScheduleChange::Schedule { task, at } => {
                schedules.insert(task, at);
            }
            ScheduleChange::Cancel { task } => {
                schedules.remove(&task);
            }
        }
    }
}

pub fn initial_state(
    principal: PrincipalRef,
    sender: ProtocolIdentity,
    recipient: ProtocolIdentity,
    now: CanonicalTime,
) -> ProtocolState {
    ProtocolState::initial(principal, sender, recipient, now)
}
