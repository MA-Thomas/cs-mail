use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};

use cs_mail_content::{ContentBinding, EncryptedContentRecord, EndpointSecretKey, encrypt};
use cs_mail_primitives::{
    AttemptId, BondId, CanonicalTime, ContentKeyRef, ContentRef, DeliveryIntentRef, Duration,
    IdempotencyKey, MessageId, Money, OperationalKeyRef, PersistenceReserveId, PolicyVersion,
    PrincipalRef, ProtocolIdentity, ProtocolVersion, ProviderRef, QuoteId, SettlementUnit, Version,
};
use cs_mail_protocol::{
    ActorRef, Authorized, PolicySnapshot, ProtocolCommand, ProtocolState, TermsOutcome,
};
use cs_mail_storage_postgres::{OutboxItem, PostgresEngine};
use cs_mail_worker::{DeliverySink, WorkerError, deliver_batch, run_schedule_batch};

const PRINCIPAL: PrincipalRef = PrincipalRef(101);
const SENDER: ProtocolIdentity = ProtocolIdentity(102);
const RECIPIENT: ProtocolIdentity = ProtocolIdentity(103);
const PROVIDER: ProviderRef = ProviderRef(104);
const UNIT: SettlementUnit = SettlementUnit(1);
static NEXT_KEY: AtomicU64 = AtomicU64::new(1);

fn database_url() -> Option<String> {
    std::env::var("CS_MAIL_TEST_DATABASE_URL").ok()
}

fn engine(url: &str, label: &str) -> PostgresEngine {
    PostgresEngine::connect(
        url,
        format!(
            "worker-{label}-{}-{}",
            std::process::id(),
            NEXT_KEY.fetch_add(1, Ordering::Relaxed)
        ),
        &ProtocolState::initial(PRINCIPAL, SENDER, RECIPIENT, CanonicalTime(0)),
        UNIT,
        Money::from_minor_units(1_000),
    )
    .unwrap()
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

fn sender(command: ProtocolCommand, idempotency: u128) -> Authorized<ProtocolCommand> {
    Authorized::assume_verified(
        command,
        ActorRef::Sender(SENDER),
        OperationalKeyRef(1),
        IdempotencyKey(idempotency),
    )
}

fn reserve(engine: &PostgresEngine, id: u128) {
    let terms = match engine
        .execute(
            sender(
                ProtocolCommand::IssueContactTerms {
                    quote_id: QuoteId(id),
                },
                id * 10,
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
        TermsOutcome::NoBondRequired => panic!("expected bond"),
    };
    engine
        .execute(
            sender(
                ProtocolCommand::ReserveAttempt {
                    bond_id: BondId(id),
                    reserve_id: PersistenceReserveId(id),
                    attempt_id: AttemptId(id),
                    message_id: MessageId(id),
                    terms,
                },
                id * 10 + 1,
            ),
            CanonicalTime(2),
            policy(),
        )
        .unwrap();
}

#[derive(Default)]
struct TestSink {
    fail_once: bool,
    published: BTreeSet<i64>,
}

impl DeliverySink for TestSink {
    type Error = &'static str;

    fn publish(
        &mut self,
        item: &OutboxItem,
        _content: Option<&EncryptedContentRecord>,
    ) -> Result<(), Self::Error> {
        if self.fail_once {
            self.fail_once = false;
            return Err("transient");
        }
        self.published.insert(item.id);
        Ok(())
    }
}

#[test]
fn delivery_failure_is_retried_after_lease_expiry() {
    let Some(url) = database_url() else {
        return;
    };
    let engine = engine(&url, "delivery");
    reserve(&engine, 1);
    let (sender_content_key, _) = EndpointSecretKey::generate(ContentKeyRef(2));
    let (_, public) = EndpointSecretKey::generate(ContentKeyRef(1));
    engine
        .store_content(
            &encrypt(
                &sender_content_key,
                public,
                ContentBinding {
                    content_ref: ContentRef(1),
                    message_id: MessageId(1),
                    sender: SENDER,
                    recipient: RECIPIENT,
                    protocol_version: ProtocolVersion(1),
                },
                b"private",
                CanonicalTime(2),
                CanonicalTime(10),
            )
            .unwrap(),
        )
        .unwrap();
    engine
        .execute(
            sender(
                ProtocolCommand::AdmitAttempt {
                    bond_id: BondId(1),
                    expected_bond_version: Version(0),
                    content_ref: ContentRef(1),
                    delivery_intent_ref: DeliveryIntentRef(1),
                },
                12,
            ),
            CanonicalTime(3),
            policy(),
        )
        .unwrap();

    let mut sink = TestSink {
        fail_once: true,
        ..TestSink::default()
    };
    assert!(matches!(
        deliver_batch(&engine, &mut sink, CanonicalTime(4), Duration(10), 10),
        Err(WorkerError::Delivery(_))
    ));
    assert_eq!(
        deliver_batch(&engine, &mut sink, CanonicalTime(5), Duration(10), 10)
            .unwrap()
            .claimed,
        0
    );
    assert_eq!(engine.purge_expired_content(CanonicalTime(10)).unwrap(), 0);
    let report = deliver_batch(&engine, &mut sink, CanonicalTime(14), Duration(10), 10).unwrap();
    assert_eq!(report.completed, 2);
    assert_eq!(engine.purge_expired_content(CanonicalTime(14)).unwrap(), 1);
    assert!(engine.content(ContentRef(1)).unwrap().is_none());
}

#[test]
fn due_admission_timeout_is_materialized_once() {
    let Some(url) = database_url() else {
        return;
    };
    let engine = engine(&url, "schedule");
    reserve(&engine, 2);
    let report = run_schedule_batch(
        &engine,
        CanonicalTime(12),
        Duration(10),
        10,
        PROVIDER,
        OperationalKeyRef(99),
        &policy(),
    )
    .unwrap();
    assert_eq!(report.completed, 1);
    assert_eq!(
        engine.snapshot().unwrap().state.bonds[&BondId(2)].state,
        cs_mail_protocol::BondState::CancelledUnadmitted {
            event: cs_mail_primitives::EventRef(cs_mail_primitives::JournalPosition(3))
        }
    );
}
