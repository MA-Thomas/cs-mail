use std::sync::Arc;
use std::thread;

use cs_mail_application::{EngineError, InMemoryEngine, initial_state};
use cs_mail_ledger::Account;
use cs_mail_primitives::{
    AttemptId, BondId, CanonicalTime, ContentRef, DeliveryIntentRef, Duration, IdempotencyKey,
    MessageId, Money, OperationalKeyRef, PersistenceReserveId, PolicyVersion, PrincipalRef,
    ProtocolIdentity, ProtocolVersion, ProviderRef, QuoteId, SettlementUnit, Version,
};
use cs_mail_protocol::{
    ActorRef, Authorized, BondState, CancellationReason, EffectIntent, PolicySnapshot,
    ProtocolCommand, ProtocolError, RelationshipState, ReserveState, SolicitationStatus,
    TermsOutcome,
};

const PRINCIPAL: PrincipalRef = PrincipalRef(1);
const SENDER: ProtocolIdentity = ProtocolIdentity(10);
const RECIPIENT: ProtocolIdentity = ProtocolIdentity(20);
const PROVIDER: ProviderRef = ProviderRef(30);
const UNIT: SettlementUnit = SettlementUnit(1);

fn policy() -> PolicySnapshot {
    PolicySnapshot {
        protocol_version: ProtocolVersion(1),
        policy_version: PolicyVersion(1),
        recipient_provider: PROVIDER,
        unit: UNIT,
        processing_charge: Money::from_minor_units(2),
        collateral: Money::from_minor_units(8),
        admission_window: Duration(10),
        decision_window: Duration(50),
        quote_lifetime: Duration(20),
        persistence_duration: Duration(200),
        backoff: vec![Duration(0), Duration(5), Duration(10)],
        persistence: vec![
            Money::ZERO,
            Money::from_minor_units(5),
            Money::from_minor_units(9),
        ],
    }
}

fn engine_with_level(level: u32) -> InMemoryEngine {
    let mut state = initial_state(PRINCIPAL, SENDER, RECIPIENT, CanonicalTime(0));
    state.attempt.level = level;
    InMemoryEngine::new(state, UNIT, Money::from_minor_units(1_000)).unwrap()
}

fn sender(command: ProtocolCommand, key: u128) -> Authorized<ProtocolCommand> {
    Authorized::assume_verified(
        command,
        ActorRef::Sender(SENDER),
        OperationalKeyRef(1),
        IdempotencyKey(key),
    )
}

fn recipient(command: ProtocolCommand, key: u128) -> Authorized<ProtocolCommand> {
    Authorized::assume_verified(
        command,
        ActorRef::Recipient(RECIPIENT),
        OperationalKeyRef(2),
        IdempotencyKey(key),
    )
}

fn scheduler(command: ProtocolCommand, key: u128) -> Authorized<ProtocolCommand> {
    Authorized::assume_verified(
        command,
        ActorRef::Scheduler(PROVIDER),
        OperationalKeyRef(3),
        IdempotencyKey(key),
    )
}

fn issue_terms(engine: &InMemoryEngine, at: u64, key: u128) -> cs_mail_protocol::ContactTerms {
    let outcome = engine
        .execute(
            sender(
                ProtocolCommand::IssueContactTerms {
                    quote_id: QuoteId(key),
                },
                key,
            ),
            CanonicalTime(at),
            policy(),
        )
        .unwrap();
    match outcome.manifest.terms_outcome.unwrap() {
        TermsOutcome::BondRequired(terms) => terms,
        TermsOutcome::NoBondRequired => panic!("expected bonded terms"),
    }
}

#[derive(Clone, Copy)]
struct AttemptSpec {
    bond: BondId,
    reserve: PersistenceReserveId,
    attempt: AttemptId,
    message: MessageId,
    key_base: u128,
}

fn reserve(
    engine: &InMemoryEngine,
    spec: &AttemptSpec,
    terms: cs_mail_protocol::ContactTerms,
    at: u64,
) {
    engine
        .execute(
            sender(
                ProtocolCommand::ReserveAttempt {
                    bond_id: spec.bond,
                    reserve_id: spec.reserve,
                    attempt_id: spec.attempt,
                    message_id: spec.message,
                    terms,
                },
                spec.key_base + 1,
            ),
            CanonicalTime(at),
            policy(),
        )
        .unwrap();
}

fn admit(engine: &InMemoryEngine, spec: &AttemptSpec, at: u64) {
    engine
        .execute(
            sender(
                ProtocolCommand::AdmitAttempt {
                    bond_id: spec.bond,
                    expected_bond_version: Version(0),
                    content_ref: ContentRef(spec.key_base),
                    delivery_intent_ref: DeliveryIntentRef(spec.key_base),
                },
                spec.key_base + 2,
            ),
            CanonicalTime(at),
            policy(),
        )
        .unwrap();
}

#[test]
fn unadmitted_cancellation_returns_everything_without_advancing() {
    let engine = engine_with_level(1);
    let terms = issue_terms(&engine, 1, 100);
    let spec = AttemptSpec {
        bond: BondId(1),
        reserve: PersistenceReserveId(1),
        attempt: AttemptId(1),
        message: MessageId(1),
        key_base: 200,
    };
    reserve(&engine, &spec, terms, 2);
    assert_eq!(
        engine.balance(Account::Sender(PRINCIPAL)).unwrap(),
        Money::from_minor_units(985)
    );

    engine
        .execute(
            sender(
                ProtocolCommand::CancelReservedAttempt {
                    bond_id: spec.bond,
                    expected_bond_version: Version(0),
                    reason: CancellationReason::SenderRequested,
                },
                300,
            ),
            CanonicalTime(3),
            policy(),
        )
        .unwrap();

    let snapshot = engine.snapshot().unwrap();
    assert!(matches!(
        snapshot.state.bonds[&spec.bond].state,
        BondState::CancelledUnadmitted { .. }
    ));
    assert!(matches!(
        snapshot.state.reserves[&spec.reserve].state,
        ReserveState::Released { .. }
    ));
    assert_eq!(snapshot.state.attempt.level, 1);
    assert!(snapshot.state.solicitation.is_none());
    assert!(engine.outbox().unwrap().is_empty());
    assert_eq!(
        engine.balance(Account::Sender(PRINCIPAL)).unwrap(),
        Money::from_minor_units(1_000)
    );
}

#[test]
fn admissions_coalesce_and_acceptance_is_relationship_wide() {
    let engine = engine_with_level(0);
    let first = AttemptSpec {
        bond: BondId(1),
        reserve: PersistenceReserveId(1),
        attempt: AttemptId(1),
        message: MessageId(1),
        key_base: 1_000,
    };
    let first_terms = issue_terms(&engine, 1, 900);
    reserve(&engine, &first, first_terms, 2);
    admit(&engine, &first, 3);

    let second = AttemptSpec {
        bond: BondId(2),
        reserve: PersistenceReserveId(2),
        attempt: AttemptId(2),
        message: MessageId(2),
        key_base: 2_000,
    };
    let second_terms = issue_terms(&engine, 4, 1_900);
    reserve(&engine, &second, second_terms, 5);
    admit(&engine, &second, 8);

    let establishment_count = engine
        .outbox()
        .unwrap()
        .iter()
        .filter(|intent| {
            matches!(
                intent,
                EffectIntent::EstablishRelationshipSolicitation { .. }
            )
        })
        .count();
    assert_eq!(establishment_count, 1);
    assert_eq!(
        engine
            .outbox()
            .unwrap()
            .iter()
            .filter(|intent| matches!(intent, EffectIntent::DeliverMessage { .. }))
            .count(),
        2
    );

    let version = engine.snapshot().unwrap().state.relationship.version;
    engine
        .execute(
            recipient(
                ProtocolCommand::AcceptRelationship {
                    expected_version: version,
                },
                3_000,
            ),
            CanonicalTime(9),
            policy(),
        )
        .unwrap();

    let snapshot = engine.snapshot().unwrap();
    assert_eq!(
        snapshot.state.relationship.state,
        RelationshipState::Accepted
    );
    assert_eq!(snapshot.state.attempt.level, 2);
    assert!(
        snapshot
            .state
            .bonds
            .values()
            .all(|bond| matches!(bond.state, BondState::Accepted { .. }))
    );
    assert!(
        snapshot
            .state
            .reserves
            .values()
            .all(|reserve| matches!(reserve.state, ReserveState::Released { .. }))
    );
    assert_eq!(
        snapshot.state.solicitation.unwrap().status,
        SolicitationStatus::Closed
    );
    assert_eq!(
        engine.balance(Account::Sender(PRINCIPAL)).unwrap(),
        Money::from_minor_units(1_000)
    );
    assert_eq!(
        engine.total_value().unwrap(),
        Money::from_minor_units(1_000)
    );
}

#[test]
fn rejection_preserves_backoff_and_defers_persistence_release() {
    let engine = engine_with_level(1);
    let spec = AttemptSpec {
        bond: BondId(1),
        reserve: PersistenceReserveId(1),
        attempt: AttemptId(1),
        message: MessageId(1),
        key_base: 4_000,
    };
    let terms = issue_terms(&engine, 1, 3_900);
    let release_at = terms.persistence_release_at;
    reserve(&engine, &spec, terms, 2);
    admit(&engine, &spec, 3);
    engine
        .execute(
            recipient(
                ProtocolCommand::RejectRelationship {
                    expected_version: Version(0),
                },
                5_000,
            ),
            CanonicalTime(4),
            policy(),
        )
        .unwrap();

    let snapshot = engine.snapshot().unwrap();
    assert_eq!(
        snapshot.state.relationship.state,
        RelationshipState::Rejected
    );
    assert_eq!(snapshot.state.attempt.level, 2);
    assert!(matches!(
        snapshot.state.reserves[&spec.reserve].state,
        ReserveState::Reserved
    ));
    assert_eq!(
        engine
            .balance(Account::RecipientProvider(PROVIDER))
            .unwrap(),
        Money::from_minor_units(2)
    );
    assert_eq!(
        engine.balance(Account::Recipient(RECIPIENT)).unwrap(),
        Money::from_minor_units(8)
    );
    assert_eq!(
        engine
            .balance(Account::PersistenceReserve(spec.reserve))
            .unwrap(),
        Money::from_minor_units(5)
    );

    engine
        .execute(
            scheduler(
                ProtocolCommand::ReleasePersistenceReserve {
                    reserve_id: spec.reserve,
                    expected_reserve_version: Version(0),
                },
                5_100,
            ),
            release_at,
            policy(),
        )
        .unwrap();
    assert_eq!(
        engine
            .balance(Account::PersistenceReserve(spec.reserve))
            .unwrap(),
        Money::ZERO
    );
    assert_eq!(
        engine.total_value().unwrap(),
        Money::from_minor_units(1_000)
    );
}

#[test]
fn block_cancels_unadmitted_attempts_and_unblock_does_not_reset_history() {
    let engine = engine_with_level(0);
    let first = AttemptSpec {
        bond: BondId(1),
        reserve: PersistenceReserveId(1),
        attempt: AttemptId(1),
        message: MessageId(1),
        key_base: 6_000,
    };
    let terms = issue_terms(&engine, 1, 5_900);
    reserve(&engine, &first, terms, 2);
    admit(&engine, &first, 3);
    let second = AttemptSpec {
        bond: BondId(2),
        reserve: PersistenceReserveId(2),
        attempt: AttemptId(2),
        message: MessageId(2),
        key_base: 7_000,
    };
    let terms = issue_terms(&engine, 4, 6_900);
    reserve(&engine, &second, terms, 5);

    engine
        .execute(
            recipient(
                ProtocolCommand::BlockRelationship {
                    expected_version: Version(0),
                },
                8_000,
            ),
            CanonicalTime(6),
            policy(),
        )
        .unwrap();
    let blocked = engine.snapshot().unwrap();
    assert_eq!(blocked.state.relationship.state, RelationshipState::Blocked);
    assert!(matches!(
        blocked.state.bonds[&first.bond].state,
        BondState::Rejected { .. }
    ));
    assert!(matches!(
        blocked.state.bonds[&second.bond].state,
        BondState::CancelledUnadmitted { .. }
    ));
    assert_eq!(blocked.state.attempt.level, 1);
    assert!(matches!(
        engine.execute(
            sender(
                ProtocolCommand::IssueContactTerms {
                    quote_id: QuoteId(99)
                },
                8_100
            ),
            CanonicalTime(7),
            policy()
        ),
        Err(EngineError::Protocol(ProtocolError::ContactBlocked))
    ));

    engine
        .execute(
            recipient(
                ProtocolCommand::UnblockRelationship {
                    expected_version: Version(1),
                },
                8_200,
            ),
            CanonicalTime(8),
            policy(),
        )
        .unwrap();
    let unblocked = engine.snapshot().unwrap();
    assert_eq!(
        unblocked.state.relationship.state,
        RelationshipState::Rejected
    );
    assert_eq!(unblocked.state.attempt.level, 1);
    assert_eq!(
        unblocked.state.solicitation.unwrap().status,
        SolicitationStatus::Closed
    );
}

#[test]
fn exact_replay_returns_the_original_result_without_a_second_effect() {
    let engine = engine_with_level(0);
    let command = sender(
        ProtocolCommand::IssueContactTerms {
            quote_id: QuoteId(1),
        },
        9_000,
    );
    let first = engine
        .execute(command.clone(), CanonicalTime(1), policy())
        .unwrap();
    let journal_len = engine.journal().unwrap().len();
    let second = engine
        .execute(command, CanonicalTime(999), policy())
        .unwrap();
    assert!(!first.replayed);
    assert!(second.replayed);
    assert_eq!(first.manifest, second.manifest);
    assert_eq!(engine.journal().unwrap().len(), journal_len);
}

#[test]
fn conflicting_recipient_decisions_serialize_on_relationship_version() {
    let engine = Arc::new(engine_with_level(0));
    let accept_engine = Arc::clone(&engine);
    let block_engine = Arc::clone(&engine);
    let accept = thread::spawn(move || {
        accept_engine.execute(
            recipient(
                ProtocolCommand::AcceptRelationship {
                    expected_version: Version(0),
                },
                10_000,
            ),
            CanonicalTime(1),
            policy(),
        )
    });
    let block = thread::spawn(move || {
        block_engine.execute(
            recipient(
                ProtocolCommand::BlockRelationship {
                    expected_version: Version(0),
                },
                10_001,
            ),
            CanonicalTime(1),
            policy(),
        )
    });
    let results = [accept.join().unwrap(), block.join().unwrap()];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(
                result,
                Err(EngineError::Protocol(ProtocolError::VersionConflict))
            ))
            .count(),
        1
    );
}

#[test]
fn admission_and_timeout_at_the_same_boundary_are_ordered_not_partial() {
    let engine = engine_with_level(0);
    let spec = AttemptSpec {
        bond: BondId(1),
        reserve: PersistenceReserveId(1),
        attempt: AttemptId(1),
        message: MessageId(1),
        key_base: 11_000,
    };
    let terms = issue_terms(&engine, 1, 10_900);
    reserve(&engine, &spec, terms, 2);
    let engine = Arc::new(engine);
    let admit_engine = Arc::clone(&engine);
    let timeout_engine = Arc::clone(&engine);
    let admit_spec = AttemptSpec { ..spec };
    let admit_result = thread::spawn(move || {
        admit_engine.execute(
            sender(
                ProtocolCommand::AdmitAttempt {
                    bond_id: admit_spec.bond,
                    expected_bond_version: Version(0),
                    content_ref: ContentRef(1),
                    delivery_intent_ref: DeliveryIntentRef(1),
                },
                11_100,
            ),
            CanonicalTime(12),
            policy(),
        )
    });
    let timeout_result = thread::spawn(move || {
        timeout_engine.execute(
            scheduler(
                ProtocolCommand::CancelReservedAttempt {
                    bond_id: spec.bond,
                    expected_bond_version: Version(0),
                    reason: CancellationReason::AdmissionTimeout,
                },
                11_101,
            ),
            CanonicalTime(12),
            policy(),
        )
    });
    let _ = admit_result.join().unwrap();
    let _ = timeout_result.join().unwrap();
    let snapshot = engine.snapshot().unwrap();
    assert!(matches!(
        snapshot.state.bonds[&BondId(1)].state,
        BondState::Admitted | BondState::CancelledUnadmitted { .. }
    ));
    assert_eq!(
        engine.total_value().unwrap(),
        Money::from_minor_units(1_000)
    );
}

#[test]
fn cancellation_conserves_value_across_small_policy_matrix() {
    for processing in [0, 1, 7] {
        for collateral in [0, 2, 11] {
            for persistence in [0, 3, 13] {
                let mut custom = policy();
                custom.processing_charge = Money::from_minor_units(processing);
                custom.collateral = Money::from_minor_units(collateral);
                custom.persistence = vec![Money::ZERO, Money::from_minor_units(persistence)];
                let engine = engine_with_level(1);
                let issued = engine
                    .execute(
                        sender(
                            ProtocolCommand::IssueContactTerms {
                                quote_id: QuoteId(1),
                            },
                            20_000 + u128::from(processing * 100 + collateral * 10 + persistence),
                        ),
                        CanonicalTime(1),
                        custom.clone(),
                    )
                    .unwrap();
                let terms = match issued.manifest.terms_outcome.unwrap() {
                    TermsOutcome::BondRequired(terms) => terms,
                    TermsOutcome::NoBondRequired => unreachable!(),
                };
                engine
                    .execute(
                        sender(
                            ProtocolCommand::ReserveAttempt {
                                bond_id: BondId(1),
                                reserve_id: PersistenceReserveId(1),
                                attempt_id: AttemptId(1),
                                message_id: MessageId(1),
                                terms,
                            },
                            30_000 + u128::from(processing * 100 + collateral * 10 + persistence),
                        ),
                        CanonicalTime(2),
                        custom.clone(),
                    )
                    .unwrap();
                engine
                    .execute(
                        sender(
                            ProtocolCommand::CancelReservedAttempt {
                                bond_id: BondId(1),
                                expected_bond_version: Version(0),
                                reason: CancellationReason::SenderRequested,
                            },
                            40_000 + u128::from(processing * 100 + collateral * 10 + persistence),
                        ),
                        CanonicalTime(3),
                        custom,
                    )
                    .unwrap();
                assert_eq!(
                    engine.total_value().unwrap(),
                    Money::from_minor_units(1_000)
                );
                assert_eq!(
                    engine.balance(Account::Sender(PRINCIPAL)).unwrap(),
                    Money::from_minor_units(1_000)
                );
            }
        }
    }
}

#[test]
fn expiry_settles_once_and_lapses_the_last_open_solicitation() {
    let engine = engine_with_level(1);
    let spec = AttemptSpec {
        bond: BondId(1),
        reserve: PersistenceReserveId(1),
        attempt: AttemptId(1),
        message: MessageId(1),
        key_base: 50_000,
    };
    let terms = issue_terms(&engine, 1, 49_900);
    reserve(&engine, &spec, terms, 2);
    admit(&engine, &spec, 3);
    engine
        .execute(
            scheduler(
                ProtocolCommand::ExpireBond {
                    bond_id: spec.bond,
                    expected_bond_version: Version(1),
                },
                50_100,
            ),
            CanonicalTime(54),
            policy(),
        )
        .unwrap();
    let after = engine.snapshot().unwrap();
    assert!(matches!(
        after.state.bonds[&spec.bond].state,
        BondState::Expired { .. }
    ));
    assert!(matches!(
        after.state.reserves[&spec.reserve].state,
        ReserveState::Reserved
    ));
    assert_eq!(
        after.state.solicitation.unwrap().status,
        SolicitationStatus::Lapsed
    );
    assert_eq!(
        engine
            .balance(Account::RecipientProvider(PROVIDER))
            .unwrap(),
        Money::from_minor_units(2)
    );
    assert_eq!(
        engine.balance(Account::Sender(PRINCIPAL)).unwrap(),
        Money::from_minor_units(993)
    );

    let replay = engine
        .execute(
            scheduler(
                ProtocolCommand::ExpireBond {
                    bond_id: spec.bond,
                    expected_bond_version: Version(1),
                },
                50_100,
            ),
            CanonicalTime(99),
            policy(),
        )
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(
        engine.total_value().unwrap(),
        Money::from_minor_units(1_000)
    );
}

#[test]
fn accepted_relationships_are_bond_free_until_revoked() {
    let engine = engine_with_level(0);
    engine
        .execute(
            recipient(
                ProtocolCommand::AcceptRelationship {
                    expected_version: Version(0),
                },
                60_000,
            ),
            CanonicalTime(1),
            policy(),
        )
        .unwrap();
    let accepted_terms = engine
        .execute(
            sender(
                ProtocolCommand::IssueContactTerms {
                    quote_id: QuoteId(1),
                },
                60_001,
            ),
            CanonicalTime(2),
            policy(),
        )
        .unwrap();
    assert_eq!(
        accepted_terms.manifest.terms_outcome,
        Some(TermsOutcome::NoBondRequired)
    );

    engine
        .execute(
            recipient(
                ProtocolCommand::RevokeRelationship {
                    expected_version: Version(1),
                },
                60_002,
            ),
            CanonicalTime(3),
            policy(),
        )
        .unwrap();
    let revoked = engine.snapshot().unwrap();
    assert_eq!(revoked.state.relationship.state, RelationshipState::Revoked);
    assert_eq!(revoked.state.attempt.level, 0);
    let new_terms = engine
        .execute(
            sender(
                ProtocolCommand::IssueContactTerms {
                    quote_id: QuoteId(2),
                },
                60_003,
            ),
            CanonicalTime(4),
            policy(),
        )
        .unwrap();
    assert!(matches!(
        new_terms.manifest.terms_outcome,
        Some(TermsOutcome::BondRequired(_))
    ));
}

#[test]
fn concurrent_acceptance_and_reservation_leave_no_stranded_value() {
    let engine = engine_with_level(0);
    let terms = issue_terms(&engine, 1, 70_000);
    let engine = Arc::new(engine);
    let reserve_engine = Arc::clone(&engine);
    let accept_engine = Arc::clone(&engine);
    let reserve = thread::spawn(move || {
        reserve_engine.execute(
            sender(
                ProtocolCommand::ReserveAttempt {
                    bond_id: BondId(1),
                    reserve_id: PersistenceReserveId(1),
                    attempt_id: AttemptId(1),
                    message_id: MessageId(1),
                    terms,
                },
                70_001,
            ),
            CanonicalTime(2),
            policy(),
        )
    });
    let accept = thread::spawn(move || {
        accept_engine.execute(
            recipient(
                ProtocolCommand::AcceptRelationship {
                    expected_version: Version(0),
                },
                70_002,
            ),
            CanonicalTime(2),
            policy(),
        )
    });
    let reserve_result = reserve.join().unwrap();
    let accept_result = accept.join().unwrap();
    assert!(accept_result.is_ok());
    assert!(
        reserve_result.is_ok()
            || matches!(
                reserve_result,
                Err(EngineError::Protocol(ProtocolError::RelationshipAccepted))
            )
    );
    let snapshot = engine.snapshot().unwrap();
    assert_eq!(
        snapshot.state.relationship.state,
        RelationshipState::Accepted
    );
    assert!(
        snapshot
            .state
            .bonds
            .values()
            .all(|bond| matches!(bond.state, BondState::CancelledUnadmitted { .. }))
    );
    assert_eq!(snapshot.state.attempt.level, 0);
    assert_eq!(
        engine.balance(Account::Sender(PRINCIPAL)).unwrap(),
        Money::from_minor_units(1_000)
    );
}

#[test]
fn insufficient_funds_create_no_protocol_or_ledger_hold() {
    let state = initial_state(PRINCIPAL, SENDER, RECIPIENT, CanonicalTime(0));
    let engine = InMemoryEngine::new(state, UNIT, Money::from_minor_units(5)).unwrap();
    let terms = issue_terms(&engine, 1, 80_000);
    let before = engine.snapshot().unwrap();
    let result = engine.execute(
        sender(
            ProtocolCommand::ReserveAttempt {
                bond_id: BondId(1),
                reserve_id: PersistenceReserveId(1),
                attempt_id: AttemptId(1),
                message_id: MessageId(1),
                terms,
            },
            80_001,
        ),
        CanonicalTime(2),
        policy(),
    );
    assert!(matches!(
        result,
        Err(EngineError::Protocol(ProtocolError::InsufficientFunds))
    ));
    let after = engine.snapshot().unwrap();
    assert_eq!(after.revision, before.revision);
    assert!(after.state.bonds.is_empty());
    assert!(after.state.reserves.is_empty());
    assert_eq!(
        engine.balance(Account::Sender(PRINCIPAL)).unwrap(),
        Money::from_minor_units(5)
    );
}
