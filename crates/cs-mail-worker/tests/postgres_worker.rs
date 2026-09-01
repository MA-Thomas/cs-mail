use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};

use cs_mail_content::{
    ContentBinding, ContentCertificateDigest, EncryptedContentRecord, EndpointSecretKey, encrypt,
    message_declaration_digest,
};
use cs_mail_primitives::{
    AttemptId, AttemptSubjectRef, BondId, CanonicalTime, ContentKeyRef, ContentRef,
    ContentScopeRef, DeclarationAuthority, DeclaredPurpose, DeliveryIntentRef, Duration,
    IdempotencyKey, KnownPurpose, LedgerAccountRef, MessageDeclarations, MessageId,
    MessageValidityUntil, Money, OperationalKeyRef, OriginDeclaration, OriginMode,
    PersistenceReserveId, PolicyVersion, PrincipalRef, PrivacyProfileVersion, ProtocolIdentity,
    ProtocolVersion, ProviderRef, QuoteId, RelationshipRef, RetentionPolicyVersion, SettlementUnit,
    Version, WireVersion,
};
use cs_mail_protocol::{ActorRef, PolicySnapshot, ProtocolCommand, ProtocolState, TermsOutcome};
use cs_mail_security::{
    CommandSigner, KeyRegistry, ProviderSigner, SignedCommandBytes, SigningScope,
};
use cs_mail_storage_postgres::{DurableExecutionOutcome, OutboxItem, PostgresEngine, StorageError};
use cs_mail_worker::{DeliverySink, WorkerError, deliver_batch, run_schedule_batch};

const PRINCIPAL: PrincipalRef = PrincipalRef(101);
const SENDER: ProtocolIdentity = ProtocolIdentity(102);
const RECIPIENT: ProtocolIdentity = ProtocolIdentity(103);
const PROVIDER: ProviderRef = ProviderRef(104);
const UNIT: SettlementUnit = SettlementUnit(1);
const DEPLOYMENT_DOMAIN: [u8; 32] = [7; 32];
static NEXT_KEY: AtomicU64 = AtomicU64::new(1);

fn database_url() -> String {
    std::env::var("CS_MAIL_TEST_DATABASE_URL")
        .expect("ignored PostgreSQL tests require CS_MAIL_TEST_DATABASE_URL")
}

fn engine(url: &str, label: &str) -> PostgresEngine {
    let attempt_seed = if label == "delivery" { 1_001 } else { 1_002 };
    PostgresEngine::connect(
        url,
        format!(
            "worker-{label}-{}-{}",
            std::process::id(),
            NEXT_KEY.fetch_add(1, Ordering::Relaxed)
        ),
        &ProtocolState::initial_scoped(
            RelationshipRef::from_u128_for_test(SENDER.0 ^ RECIPIENT.0),
            AttemptSubjectRef::from_u128_for_test(attempt_seed),
            LedgerAccountRef::from_u128_for_test(PRINCIPAL.0),
            SENDER,
            RECIPIENT,
            CanonicalTime(0),
        ),
        UNIT,
        Money::from_minor_units(1_000),
    )
    .unwrap()
}

fn policy() -> PolicySnapshot {
    PolicySnapshot {
        protocol_version: ProtocolVersion(1),
        policy_version: PolicyVersion(1),
        privacy_profile_version: PrivacyProfileVersion(1),
        retention_policy_version: RetentionPolicyVersion(1),
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

fn sender_signer() -> CommandSigner {
    CommandSigner::from_secret_bytes(ActorRef::Sender(SENDER), OperationalKeyRef(1), &[1; 32])
}

fn provider_signer() -> ProviderSigner {
    ProviderSigner::from_secret_bytes(PROVIDER, OperationalKeyRef(2), &[2; 32])
}

fn signing_scope() -> SigningScope {
    SigningScope {
        deployment_domain: DEPLOYMENT_DOMAIN,
        intended_provider: PROVIDER,
        relationship: RelationshipRef::from_u128_for_test(SENDER.0 ^ RECIPIENT.0),
    }
}

fn registry() -> KeyRegistry {
    let mut registry = KeyRegistry::default();
    registry
        .register(
            OperationalKeyRef(1),
            ActorRef::Sender(SENDER),
            sender_signer().verifying_key_bytes(),
            CanonicalTime(0),
        )
        .unwrap();
    registry
        .register(
            OperationalKeyRef(2),
            ActorRef::Provider(PROVIDER),
            provider_signer().verifying_key_bytes(),
            CanonicalTime(0),
        )
        .unwrap();
    registry
        .register(
            OperationalKeyRef(99),
            ActorRef::Scheduler(PROVIDER),
            CommandSigner::from_secret_bytes(
                ActorRef::Scheduler(PROVIDER),
                OperationalKeyRef(99),
                &[99; 32],
            )
            .verifying_key_bytes(),
            CanonicalTime(0),
        )
        .unwrap();
    registry
}

fn sender(command: ProtocolCommand, idempotency: u128) -> SignedCommandBytes {
    sender_signer()
        .sign(
            signing_scope(),
            ProtocolVersion(1),
            IdempotencyKey(idempotency),
            command,
        )
        .unwrap()
}

trait TestExecute {
    fn execute(
        &self,
        command: SignedCommandBytes,
        now: CanonicalTime,
        policy: PolicySnapshot,
    ) -> Result<DurableExecutionOutcome, StorageError>;
}

impl TestExecute for PostgresEngine {
    fn execute(
        &self,
        command: SignedCommandBytes,
        now: CanonicalTime,
        policy: PolicySnapshot,
    ) -> Result<DurableExecutionOutcome, StorageError> {
        let outcome = self.execute_signed(&registry(), &command, DEPLOYMENT_DOMAIN, now, policy)?;
        if let Some(TermsOutcome::BondRequired(terms)) = &outcome.manifest.terms_outcome {
            self.attach_signed_quote(&provider_signer().sign_contact_terms((**terms).clone())?)?;
        }
        Ok(outcome)
    }
}

fn declarations() -> MessageDeclarations {
    MessageDeclarations {
        purpose: DeclaredPurpose::Known(KnownPurpose::Transactional),
        origin: OriginDeclaration {
            mode: OriginMode::HumanInitiated,
            authority: DeclarationAuthority::NativeSender(OperationalKeyRef(1)),
        },
        payload_schema: None,
    }
}

fn reserve(engine: &PostgresEngine, id: u128) {
    let terms = match engine
        .execute(
            sender(
                ProtocolCommand::IssueContactTerms {
                    quote_id: QuoteId(id),
                    declaration_digest: None,
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
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn delivery_failure_is_retried_after_lease_expiry() {
    let url = database_url();
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
                    wire_version: WireVersion(1),
                    content_ref: ContentRef(1),
                    message_id: MessageId(1),
                    sender: SENDER,
                    recipient: RECIPIENT,
                    protocol_version: ProtocolVersion(1),
                    relationship: RelationshipRef::from_u128_for_test(SENDER.0 ^ RECIPIENT.0),
                    content_scope: ContentScopeRef::from_u128_for_test(1),
                    sender_certificate: ContentCertificateDigest([1; 32]),
                    declarations: declarations(),
                    message_valid_until: MessageValidityUntil(CanonicalTime(9)),
                    capability: None,
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
                    declaration_digest: message_declaration_digest(&declarations()).unwrap(),
                    message_valid_until: MessageValidityUntil(CanonicalTime(9)),
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
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn due_admission_timeout_is_materialized_once() {
    let url = database_url();
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
    assert!(matches!(
        engine.snapshot().unwrap().state.bonds[&BondId(2)].state,
        cs_mail_protocol::BondState::CancelledUnadmitted { .. }
    ));
}
