use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use cs_mail_adapters::{
    DmarcAlignment, DmarcPass, DomainIdentity, LegacyDmarcEvidence, VerifiedDomain,
};
use cs_mail_capabilities::{
    AdmissionAuthentication, BondFreeAdmission, LaneEvidence, LaneGrant, LaneMode, LaneState,
    LaneSubject, RateLimit, SignedLaneGrant,
};
use cs_mail_content::{ContentBinding, EndpointSecretKey, encrypt};
use cs_mail_ledger::Account;
use cs_mail_primitives::{
    AttemptId, BondId, CanonicalTime, ContentKeyRef, ContentRef, DeliveryIntentRef, Duration,
    IdempotencyKey, LaneId, MessageId, Money, OperationalKeyRef, PersistenceReserveId,
    PolicyVersion, PrincipalRef, ProtocolIdentity, ProtocolVersion, ProviderRef, QuoteId,
    SettlementUnit, Version,
};
use cs_mail_protocol::{
    ActorRef, Authorized, BondState, EffectIntent, PolicySnapshot, ProtocolCommand, ProtocolError,
    ProtocolState, RelationshipState, TermsOutcome,
};
use cs_mail_storage_postgres::{PostgresEngine, StorageError};
use ed25519_dalek::{Signer, SigningKey};

const PRINCIPAL: PrincipalRef = PrincipalRef(1);
const SENDER: ProtocolIdentity = ProtocolIdentity(10);
const RECIPIENT: ProtocolIdentity = ProtocolIdentity(20);
const PROVIDER: ProviderRef = ProviderRef(30);
const UNIT: SettlementUnit = SettlementUnit(1);
static NEXT_KEY: AtomicU64 = AtomicU64::new(1);

fn database_url() -> Option<String> {
    std::env::var("CS_MAIL_TEST_DATABASE_URL").ok()
}

fn aggregate_key(label: &str) -> String {
    format!(
        "test-{label}-{}-{}",
        std::process::id(),
        NEXT_KEY.fetch_add(1, Ordering::Relaxed)
    )
}

fn state() -> ProtocolState {
    ProtocolState::initial(PRINCIPAL, SENDER, RECIPIENT, CanonicalTime(0))
}

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
        backoff: vec![Duration(0), Duration(5)],
        persistence: vec![Money::ZERO, Money::from_minor_units(5)],
    }
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

#[test]
#[allow(clippy::too_many_lines)]
fn durable_sequence_survives_reconnect_and_outbox_leases_recover() {
    let Some(url) = database_url() else {
        eprintln!("skipping PostgreSQL test; CS_MAIL_TEST_DATABASE_URL is unset");
        return;
    };
    let key = aggregate_key("durable");
    let engine =
        PostgresEngine::connect(&url, &key, &state(), UNIT, Money::from_minor_units(1_000))
            .unwrap();
    let issued = engine
        .execute(
            sender(
                ProtocolCommand::IssueContactTerms {
                    quote_id: QuoteId(1),
                },
                1,
            ),
            CanonicalTime(1),
            policy(),
        )
        .unwrap();
    let terms = match issued.manifest.terms_outcome.unwrap() {
        TermsOutcome::BondRequired(terms) => terms,
        TermsOutcome::NoBondRequired => panic!("expected bonded terms"),
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
                2,
            ),
            CanonicalTime(2),
            policy(),
        )
        .unwrap();
    let (sender_content_key, _) = EndpointSecretKey::generate(ContentKeyRef(2));
    let (_, recipient_key) = EndpointSecretKey::generate(ContentKeyRef(1));
    engine
        .store_content(
            &encrypt(
                &sender_content_key,
                recipient_key,
                ContentBinding {
                    content_ref: ContentRef(1),
                    message_id: MessageId(1),
                    sender: SENDER,
                    recipient: RECIPIENT,
                    protocol_version: ProtocolVersion(1),
                },
                b"encrypted before upload",
                CanonicalTime(2),
                CanonicalTime(1_000),
            )
            .unwrap(),
        )
        .unwrap();
    let admission = sender(
        ProtocolCommand::AdmitAttempt {
            bond_id: BondId(1),
            expected_bond_version: Version(0),
            content_ref: ContentRef(1),
            delivery_intent_ref: DeliveryIntentRef(1),
        },
        3,
    );
    let first = engine
        .execute(admission.clone(), CanonicalTime(3), policy())
        .unwrap();
    assert!(!first.replayed);
    assert!(
        engine
            .execute(admission, CanonicalTime(999), policy())
            .unwrap()
            .replayed
    );

    let claimed = engine
        .claim_outbox(CanonicalTime(4), Duration(10), 10)
        .unwrap();
    assert_eq!(claimed.len(), 2);
    assert!(claimed.iter().any(|item| matches!(
        item.payload,
        EffectIntent::EstablishRelationshipSolicitation { .. }
    )));
    assert!(
        engine
            .claim_outbox(CanonicalTime(5), Duration(10), 10)
            .unwrap()
            .is_empty()
    );
    let reclaimed = engine
        .claim_outbox(CanonicalTime(14), Duration(10), 10)
        .unwrap();
    assert_eq!(reclaimed.len(), 2);
    assert!(
        engine
            .mark_outbox_published(reclaimed[0].id, CanonicalTime(15))
            .unwrap()
    );

    drop(engine);
    let reopened =
        PostgresEngine::connect(&url, &key, &state(), UNIT, Money::from_minor_units(999_999))
            .unwrap();
    let before_acceptance = reopened.snapshot().unwrap();
    assert!(matches!(
        before_acceptance.state.bonds[&BondId(1)].state,
        BondState::Admitted
    ));
    assert_eq!(
        before_acceptance.ledger.balance(Account::Sender(PRINCIPAL)),
        Money::from_minor_units(990)
    );
    reopened
        .execute(
            recipient(
                ProtocolCommand::AcceptRelationship {
                    expected_version: Version(0),
                },
                4,
            ),
            CanonicalTime(4),
            policy(),
        )
        .unwrap();
    let accepted = reopened.snapshot().unwrap();
    assert_eq!(
        accepted.state.relationship.state,
        RelationshipState::Accepted
    );
    assert_eq!(
        accepted.ledger.balance(Account::Sender(PRINCIPAL)),
        Money::from_minor_units(1_000)
    );
}

#[test]
fn admission_requires_durable_correctly_scoped_ciphertext() {
    let Some(url) = database_url() else {
        eprintln!("skipping PostgreSQL test; CS_MAIL_TEST_DATABASE_URL is unset");
        return;
    };
    let engine = PostgresEngine::connect(
        &url,
        aggregate_key("content"),
        &state(),
        UNIT,
        Money::from_minor_units(1_000),
    )
    .unwrap();
    let terms = match engine
        .execute(
            sender(
                ProtocolCommand::IssueContactTerms {
                    quote_id: QuoteId(44),
                },
                40,
            ),
            CanonicalTime(1),
            policy(),
        )
        .unwrap()
        .manifest
        .terms_outcome
        .unwrap()
    {
        TermsOutcome::BondRequired(terms) => terms,
        TermsOutcome::NoBondRequired => panic!("expected bonded terms"),
    };
    engine
        .execute(
            sender(
                ProtocolCommand::ReserveAttempt {
                    bond_id: BondId(44),
                    reserve_id: PersistenceReserveId(44),
                    attempt_id: AttemptId(44),
                    message_id: MessageId(44),
                    terms,
                },
                41,
            ),
            CanonicalTime(2),
            policy(),
        )
        .unwrap();
    let admission = || {
        sender(
            ProtocolCommand::AdmitAttempt {
                bond_id: BondId(44),
                expected_bond_version: Version(0),
                content_ref: ContentRef(44),
                delivery_intent_ref: DeliveryIntentRef(44),
            },
            42,
        )
    };
    assert!(matches!(
        engine.execute(admission(), CanonicalTime(3), policy()),
        Err(StorageError::ContentMissing)
    ));

    let (sender_content_key, _) = EndpointSecretKey::generate(ContentKeyRef(45));
    let (_, public) = EndpointSecretKey::generate(ContentKeyRef(44));
    let record = encrypt(
        &sender_content_key,
        public,
        ContentBinding {
            content_ref: ContentRef(44),
            message_id: MessageId(44),
            sender: SENDER,
            recipient: RECIPIENT,
            protocol_version: ProtocolVersion(1),
        },
        b"ciphertext only",
        CanonicalTime(2),
        CanonicalTime(100),
    )
    .unwrap();
    engine.store_content(&record).unwrap();
    engine
        .execute(admission(), CanonicalTime(3), policy())
        .unwrap();
}

#[test]
fn database_row_lock_serializes_conflicting_decisions() {
    let Some(url) = database_url() else {
        eprintln!("skipping PostgreSQL test; CS_MAIL_TEST_DATABASE_URL is unset");
        return;
    };
    let key = aggregate_key("race");
    let first = PostgresEngine::connect(&url, &key, &state(), UNIT, Money::from_minor_units(1_000))
        .unwrap();
    let second =
        PostgresEngine::connect(&url, &key, &state(), UNIT, Money::from_minor_units(1_000))
            .unwrap();
    let accept = thread::spawn(move || {
        first.execute(
            recipient(
                ProtocolCommand::AcceptRelationship {
                    expected_version: Version(0),
                },
                10,
            ),
            CanonicalTime(1),
            policy(),
        )
    });
    let block = thread::spawn(move || {
        second.execute(
            recipient(
                ProtocolCommand::BlockRelationship {
                    expected_version: Version(0),
                },
                11,
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
                Err(StorageError::Protocol(ProtocolError::VersionConflict))
            ))
            .count(),
        1
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn lane_admission_is_bond_free_replay_safe_and_atomically_revoked_by_block() {
    let Some(url) = database_url() else {
        eprintln!("skipping PostgreSQL test; CS_MAIL_TEST_DATABASE_URL is unset");
        return;
    };
    let engine = PostgresEngine::connect(
        &url,
        aggregate_key("lane"),
        &state(),
        UNIT,
        Money::from_minor_units(1_000),
    )
    .unwrap();
    let recipient_signing = SigningKey::from_bytes(&[9; 32]);
    let grant = LaneGrant {
        id: LaneId(90),
        subject: LaneSubject::LegacyDomain(DomainIdentity::parse_ascii("bank.com").unwrap()),
        sender: SENDER,
        recipient: RECIPIENT,
        protocol_version: ProtocolVersion(1),
        deployment_domain: [3; 32],
        intended_provider: PROVIDER,
        recipient_operational_key: OperationalKeyRef(2),
        purpose_label: "account notices".into(),
        lifetime: Duration(1_000),
        rate_limit: RateLimit {
            max_messages: 2,
            interval: Duration(100),
        },
        mode: LaneMode::Expiring,
        issued_at: CanonicalTime(1),
        not_before: CanonicalTime(1),
        not_after: CanonicalTime(10_000),
        version: Version(1),
    };
    let signed = SignedLaneGrant {
        signature: recipient_signing
            .sign(&grant.signing_bytes().unwrap())
            .to_bytes(),
        grant,
    };
    engine
        .grant_lane(
            &signed,
            &recipient_signing.verifying_key().to_bytes(),
            IdempotencyKey(90),
            CanonicalTime(1),
        )
        .unwrap();

    let (sender_content_key, _) = EndpointSecretKey::generate(ContentKeyRef(90));
    let (_, recipient_content_key) = EndpointSecretKey::generate(ContentKeyRef(91));
    engine
        .store_content(
            &encrypt(
                &sender_content_key,
                recipient_content_key,
                ContentBinding {
                    content_ref: ContentRef(90),
                    message_id: MessageId(90),
                    sender: SENDER,
                    recipient: RECIPIENT,
                    protocol_version: ProtocolVersion(1),
                },
                b"lane ciphertext",
                CanonicalTime(1),
                CanonicalTime(1_000),
            )
            .unwrap(),
        )
        .unwrap();
    let evidence = LaneEvidence::Legacy(
        LegacyDmarcEvidence::new(
            DomainIdentity::parse_ascii("bank.com").unwrap(),
            VerifiedDomain::passed(
                DomainIdentity::parse_ascii("alerts.e.bank.com").unwrap(),
                Some(DmarcPass {
                    alignment: DmarcAlignment::Relaxed,
                }),
                None,
                CanonicalTime(2),
            )
            .unwrap(),
        )
        .unwrap(),
    );
    let admission = BondFreeAdmission {
        sender: SENDER,
        recipient: RECIPIENT,
        message_id: MessageId(90),
        content_ref: ContentRef(90),
        delivery_intent_ref: DeliveryIntentRef(90),
        evidence: Some(evidence),
        idempotency_key: IdempotencyKey(91),
        protocol_version: ProtocolVersion(1),
        deployment_domain: [3; 32],
        intended_provider: PROVIDER,
        authentication: AdmissionAuthentication::LegacyDmarc,
    };
    assert!(
        !engine
            .admit_bond_free(&admission, CanonicalTime(2))
            .unwrap()
            .replayed
    );
    assert!(
        engine
            .admit_bond_free(&admission, CanonicalTime(3))
            .unwrap()
            .replayed
    );
    assert_eq!(engine.lane().unwrap().unwrap().messages_in_rate_window, 1);

    engine
        .execute(
            recipient(
                ProtocolCommand::AcceptRelationship {
                    expected_version: Version(0),
                },
                94,
            ),
            CanonicalTime(3),
            policy(),
        )
        .unwrap();
    let (_, second_recipient_content_key) = EndpointSecretKey::generate(ContentKeyRef(92));
    engine
        .store_content(
            &encrypt(
                &sender_content_key,
                second_recipient_content_key,
                ContentBinding {
                    content_ref: ContentRef(91),
                    message_id: MessageId(91),
                    sender: SENDER,
                    recipient: RECIPIENT,
                    protocol_version: ProtocolVersion(1),
                },
                b"accepted ciphertext",
                CanonicalTime(3),
                CanonicalTime(1_000),
            )
            .unwrap(),
        )
        .unwrap();
    let accepted_admission = BondFreeAdmission {
        sender: SENDER,
        recipient: RECIPIENT,
        message_id: MessageId(91),
        content_ref: ContentRef(91),
        delivery_intent_ref: DeliveryIntentRef(91),
        evidence: None,
        idempotency_key: IdempotencyKey(95),
        protocol_version: ProtocolVersion(1),
        deployment_domain: [3; 32],
        intended_provider: PROVIDER,
        authentication: AdmissionAuthentication::NativeKey(OperationalKeyRef(1)),
    };
    assert!(matches!(
        engine
            .admit_bond_free(&accepted_admission, CanonicalTime(3))
            .unwrap()
            .authority,
        cs_mail_storage_postgres::BondFreeAuthority::AcceptedRelationship
    ));

    engine
        .execute(
            recipient(
                ProtocolCommand::BlockRelationship {
                    expected_version: Version(1),
                },
                92,
            ),
            CanonicalTime(4),
            policy(),
        )
        .unwrap();
    assert_eq!(engine.lane().unwrap().unwrap().state, LaneState::Revoked);
    let mut blocked = admission;
    blocked.idempotency_key = IdempotencyKey(93);
    assert!(matches!(
        engine.admit_bond_free(&blocked, CanonicalTime(5)),
        Err(StorageError::Protocol(ProtocolError::ContactBlocked))
    ));
}
