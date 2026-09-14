//! Transactional in-memory execution for the deterministic protocol kernel.
//!
//! This adapter is deliberately small: it demonstrates atomic state, ledger,
//! schedule, journal, outbox, and idempotency behavior without pretending to be
//! a production database.

use cs_mail_primitives::{MessageId, RelationshipRef, RequestHistoryRef, RequestId};
use cs_mail_protocol::{Message, RequestHistory};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use sha2::Digest;

use cs_mail_ledger::{Account, LedgerError, LedgerState};
use cs_mail_primitives::{
    CanonicalTime, IdempotencyKey, JournalPosition, Money, PrincipalRef, ProtocolIdentity,
    SettlementUnit,
};
use cs_mail_protocol::{
    CommittedTransition, EffectIntent, KernelCommand, PolicySnapshot, ProtocolCommand,
    ProtocolError, ProtocolEvent, ProtocolState, ScheduleChange, ScheduleTask, SettlementSnapshot,
    TransitionContext, transition,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineError {
    Protocol(ProtocolError),
    Ledger(LedgerError),
    DuplicateConflict,
    VersionConflict,
    LockPoisoned,
    ArithmeticOverflow,
    CommandEncoding,
    Finance(cs_mail_finance::ProgramError),
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionOutcome {
    pub transition: CommittedTransition,
    pub replayed: bool,
}

#[derive(Clone)]
struct ExecutionRecord {
    fingerprint: [u8; 32],
    transition: CommittedTransition,
}

#[derive(Clone)]
struct EngineState {
    revision: u64,
    protocol: ProtocolState,
    ledger: LedgerState,
    payments: BTreeMap<RequestId, cs_mail_finance::RequestFinancials>,
    messages: BTreeMap<MessageId, Message>,
    journal: Vec<ProtocolEvent>,
    schedules: BTreeMap<ScheduleTask, CanonicalTime>,
    outbox: Vec<EffectIntent>,
    idempotency: BTreeMap<IdempotencyKey, ExecutionRecord>,
    next_journal_position: u64,
}

#[derive(Default)]
struct StoreState {
    relationships: BTreeMap<RelationshipRef, EngineState>,
    histories: BTreeMap<RequestHistoryRef, RequestHistory>,
    programs: BTreeMap<SettlementUnit, ProgramState>,
}
struct ProgramState {
    program: cs_mail_finance::FinancialProgram,
    financial_commands: BTreeMap<
        IdempotencyKey,
        (
            cs_mail_finance::SignedProgramCommand,
            cs_mail_finance::ProgramOutcome,
        ),
    >,
}
/// All handles from a store share principal-recipient history and the per-unit program.
#[derive(Clone)]
pub struct InMemoryStore {
    scope: cs_mail_finance::FinancialScope,
    inner: Arc<Mutex<StoreState>>,
}
pub struct InMemoryEngine {
    store: InMemoryStore,
    relationship: RelationshipRef,
}
impl InMemoryStore {
    pub fn new(scope: cs_mail_finance::FinancialScope) -> Self {
        Self {
            scope,
            inner: Arc::new(Mutex::new(StoreState::default())),
        }
    }
    /// Registers a relationship without replacing existing history or program data.
    /// # Errors
    /// Rejects duplicate relationships and poisoned locks.
    pub fn register(
        &self,
        protocol: ProtocolState,
        unit: SettlementUnit,
    ) -> Result<InMemoryEngine, EngineError> {
        let mut world = self.inner.lock().map_err(|_| EngineError::LockPoisoned)?;
        let relationship = protocol.relationship.key.reference;
        if world.relationships.contains_key(&relationship)
            || world.relationships.values().any(|r| {
                r.protocol.relationship.key.sender == protocol.relationship.key.sender
                    && r.protocol.relationship.key.recipient == protocol.relationship.key.recipient
            })
        {
            return Err(EngineError::DuplicateConflict);
        }
        let history = world
            .histories
            .entry(protocol.relationship.history)
            .or_insert_with(|| {
                RequestHistory::new(
                    protocol.relationship.history,
                    protocol.relationship.key.recipient,
                    protocol.relationship.changed_at,
                )
            });
        if history.recipient != protocol.relationship.key.recipient {
            return Err(EngineError::DuplicateConflict);
        }
        world.programs.entry(unit).or_insert_with(|| ProgramState {
            program: cs_mail_finance::FinancialProgram::new(self.scope, unit),
            financial_commands: BTreeMap::new(),
        });
        world.relationships.insert(
            relationship,
            EngineState {
                revision: 0,
                protocol,
                ledger: LedgerState::new(unit),
                payments: BTreeMap::new(),
                messages: BTreeMap::new(),
                journal: Vec::new(),
                schedules: BTreeMap::new(),
                outbox: Vec::new(),
                idempotency: BTreeMap::new(),
                next_journal_position: 1,
            },
        );
        Ok(InMemoryEngine {
            store: self.clone(),
            relationship,
        })
    }
}
impl InMemoryEngine {
    /// Creates a relationship in a new store. Use a shared store for multiple aliases.
    /// # Errors
    /// Returns an initialization error.
    pub fn new(
        protocol: ProtocolState,
        unit: SettlementUnit,
        scope: cs_mail_finance::FinancialScope,
    ) -> Result<Self, EngineError> {
        InMemoryStore::new(scope).register(protocol, unit)
    }

    /// Linearizes and atomically commits one trusted host command.
    ///
    /// # Errors
    ///
    /// Returns an error when protocol, ledger, concurrency, idempotency, or
    /// arithmetic preconditions fail, or if the engine lock is poisoned.
    pub fn execute(
        &self,
        authorized: KernelCommand<ProtocolCommand>,
        now: CanonicalTime,
        policy: PolicySnapshot,
    ) -> Result<ExecutionOutcome, EngineError> {
        let mut world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let mut inner = world.relationships[&self.relationship].clone();
        if policy.financial.scope != self.store.scope {
            return Err(EngineError::VersionConflict);
        }
        let key = authorized.idempotency_key();
        let fingerprint: [u8; 32] = sha2::Sha256::digest(
            serde_json::to_vec(&authorized).map_err(|_| EngineError::CommandEncoding)?,
        )
        .into();
        if let Some(record) = inner.idempotency.get(&key) {
            if record.fingerprint == fingerprint {
                return Ok(ExecutionOutcome {
                    transition: record.transition.clone(),
                    replayed: true,
                });
            }
            return Err(EngineError::DuplicateConflict);
        }

        let snapshot = SettlementSnapshot::complete(
            inner.revision,
            inner.protocol.clone(),
            world.histories[&inner.protocol.relationship.history].clone(),
            inner.payments.clone(),
            inner.messages.clone(),
            inner.ledger.view(),
        );
        let context = TransitionContext {
            // This trusted kernel harness does not host ciphertext or provider policy.
            admission: Ok(()),
            now,
            journal_position: JournalPosition(inner.next_journal_position),
            protocol_version: policy.protocol_version,
            policy,
        };
        let manifest = transition(&snapshot, &authorized, &context)?;
        // The replay record keeps the digest; release the original command body now.
        drop(authorized);
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

        let unit = inner.ledger.view().unit;
        let mut program = world.programs[&unit].program.clone();
        for source in &manifest.forfeitures {
            program
                .record_forfeiture(source.clone(), now)
                .map_err(EngineError::Finance)?;
        }
        for id in &manifest.forfeiture_holds {
            program
                .note_reversal(*id, now)
                .map_err(EngineError::Finance)?;
        }
        let outcome = manifest.committed(context.journal_position);
        let record = ExecutionRecord {
            fingerprint,
            transition: outcome.clone(),
        };

        world
            .programs
            .get_mut(&unit)
            .ok_or(EngineError::VersionConflict)?
            .program = program;
        world.histories.insert(
            manifest.next_history.reference,
            manifest.next_history.clone(),
        );
        inner.payments = manifest.next_payments.clone();
        inner.messages = manifest.next_messages.clone();
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
        world.relationships.insert(self.relationship, inner);

        Ok(ExecutionOutcome {
            transition: outcome,
            replayed: false,
        })
    }

    /// Returns a complete point-in-time settlement snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if the engine lock is poisoned.
    pub fn snapshot(&self) -> Result<SettlementSnapshot, EngineError> {
        let world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let inner = &world.relationships[&self.relationship];
        Ok(SettlementSnapshot::complete(
            inner.revision,
            inner.protocol.clone(),
            world.histories[&inner.protocol.relationship.history].clone(),
            inner.payments.clone(),
            inner.messages.clone(),
            inner.ledger.view(),
        ))
    }

    /// Returns one projected ledger balance.
    ///
    /// # Errors
    ///
    /// Returns an error if the engine lock is poisoned.
    pub fn balance(&self, account: Account) -> Result<Money, EngineError> {
        let world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let inner = &world.relationships[&self.relationship];
        Ok(inner.ledger.balance(account))
    }

    /// Returns the conserved value across all ledger accounts.
    ///
    /// # Errors
    ///
    /// Returns an error if the lock is poisoned or balance summation overflows.
    pub fn total_value(&self) -> Result<Money, EngineError> {
        let world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let inner = &world.relationships[&self.relationship];
        Ok(inner.ledger.total_value()?)
    }

    /// Returns the canonical protocol event journal.
    ///
    /// # Errors
    ///
    /// Returns an error if the engine lock is poisoned.
    pub fn journal(&self) -> Result<Vec<ProtocolEvent>, EngineError> {
        let world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let inner = &world.relationships[&self.relationship];
        Ok(inner.journal.clone())
    }

    /// Returns durable post-commit effect intents.
    ///
    /// # Errors
    ///
    /// Returns an error if the engine lock is poisoned.
    pub fn outbox(&self) -> Result<Vec<EffectIntent>, EngineError> {
        let world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let inner = &world.relationships[&self.relationship];
        Ok(inner.outbox.clone())
    }

    /// Executes a signed command using authority supplied by the trusted in-memory host.
    /// Durable network services use the `PostgreSQL` adapter's persisted authority instead.
    /// # Errors
    /// Rejects signatures, stale revisions, conflicting replay, and invalid program transitions.
    pub fn execute_financial_command(
        &self,
        signed: &cs_mail_finance::SignedProgramCommand,
        key: &[u8; 32],
        now: CanonicalTime,
    ) -> Result<cs_mail_finance::ProgramOutcome, EngineError> {
        signed.verify(key).map_err(EngineError::Finance)?;
        let mut world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let unit = world.relationships[&self.relationship].ledger.view().unit;
        let inner = world
            .programs
            .get_mut(&unit)
            .ok_or(EngineError::VersionConflict)?;
        if let Some((previous, outcome)) = inner.financial_commands.get(&signed.idempotency_key) {
            return if previous == signed {
                Ok(outcome.clone())
            } else {
                Err(EngineError::DuplicateConflict)
            };
        }
        if signed.scope != inner.program.scope
            || signed.unit != inner.program.unit
            || signed.expected_revision != inner.program.revision
        {
            return Err(EngineError::VersionConflict);
        }
        let outcome = inner
            .program
            .apply(&signed.command, now)
            .map_err(EngineError::Finance)?;
        inner
            .financial_commands
            .insert(signed.idempotency_key, (signed.clone(), outcome.clone()));
        Ok(outcome)
    }
    /// # Errors
    /// Returns an error if the in-memory state lock is poisoned.
    pub fn financial_program(&self) -> Result<cs_mail_finance::FinancialProgram, EngineError> {
        let world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let unit = world.relationships[&self.relationship].ledger.view().unit;
        Ok(world.programs[&unit].program.clone())
    }

    /// Returns currently scheduled protocol work.
    ///
    /// # Errors
    ///
    /// Returns an error if the engine lock is poisoned.
    pub fn schedules(&self) -> Result<BTreeMap<ScheduleTask, CanonicalTime>, EngineError> {
        let world = self
            .store
            .inner
            .lock()
            .map_err(|_| EngineError::LockPoisoned)?;
        let inner = &world.relationships[&self.relationship];
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
