use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use cs_mail_capabilities::{
    AdmissionAuthentication, BondFreeAdmission, DeclarationAuthorityConstraint, LaneEvidence,
    LaneGrant, LaneMode, LaneState, LaneSubject, RateLimit, SignedLaneGrant,
};
use cs_mail_content::{
    ContentBinding, ContentCertificateDigest, EndpointSecretKey, encrypt,
    message_declaration_digest,
};
use cs_mail_ledger::Account;
use cs_mail_primitives::{
    CanonicalTime, ContentKeyRef, ContentRef, ContentScopeRef, DeclarationAuthority,
    DeclaredPurpose, DeliveryIntentRef, Duration, IdempotencyKey, KnownPurpose, LaneId,
    MessageDeclarations, MessageId, MessageValidityUntil, Money, OperationalKeyRef,
    OriginDeclaration, OriginMode, PolicyVersion, PrivacyProfileVersion, ProtocolIdentity,
    ProtocolVersion, ProviderRef, QuoteId, RelationshipRef, RequestHistoryRef, RequestId,
    RetentionPolicyVersion, SettlementUnit, Version, WireVersion,
};
use cs_mail_protocol::{
    ActorRef, EffectIntent, PolicySnapshot, ProtocolCommand, ProtocolError, ProtocolState,
    RelationshipState, RequestLifecycle, TermsOutcome,
};
use cs_mail_security::{
    CommandSigner, KeyRegistry, ProviderSigner, SignedCommandBytes, SigningScope,
};
use cs_mail_storage_postgres::{DurableExecutionOutcome, PostgresEngine, StorageError};
use ed25519_dalek::{Signer, SigningKey};

const SENDER: ProtocolIdentity = ProtocolIdentity(10);
const RECIPIENT: ProtocolIdentity = ProtocolIdentity(20);
const PROVIDER: ProviderRef = ProviderRef(30);
const UNIT: SettlementUnit = SettlementUnit(1);
const DEPLOYMENT_DOMAIN: [u8; 32] = [7; 32];
static NEXT_KEY: AtomicU64 = AtomicU64::new(1);

fn database_url() -> String {
    let base =
        std::env::var("CS_MAIL_TEST_DATABASE_URL").expect("requires isolated test PostgreSQL");
    let schema = format!(
        "cs_test_{}_{}_{}",
        std::process::id(),
        NEXT_KEY.fetch_add(1, Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut client = postgres::Client::connect(&base, postgres::NoTls).unwrap();
    client
        .batch_execute(&format!("CREATE SCHEMA {schema}"))
        .unwrap();
    format!("{base}?options=-csearch_path%3D{schema}")
}

fn aggregate_key(label: &str) -> String {
    format!(
        "test-{label}-{}-{}",
        std::process::id(),
        NEXT_KEY.fetch_add(1, Ordering::Relaxed)
    )
}

fn state(attempt_seed: u128) -> ProtocolState {
    ProtocolState::initial_scoped(
        RelationshipRef::from_u128_for_test(SENDER.0 ^ RECIPIENT.0),
        RequestHistoryRef::from_u128_for_test(attempt_seed),
        SENDER,
        RECIPIENT,
        CanonicalTime(0),
    )
}

fn policy() -> PolicySnapshot {
    PolicySnapshot {
        protocol_version: ProtocolVersion(2),
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
        backoff: vec![Duration(0), Duration(5)],
        financial: cs_mail_finance::FinancialTerms {
            scope: cs_mail_finance::FinancialScope::new(
                [7; 32],
                cs_mail_primitives::ProviderRef(30),
                cs_mail_primitives::ProgramRef(1),
                [9; 32],
                cs_mail_primitives::ProtocolVersion(2),
            ),
            policy_version: PolicyVersion(1),
            corporate_basis_points: 300,
            maturity_delay: Duration(10),
        },
        payment_provider_key: cs_mail_finance::SimulatedProvider::new([7; 32]).verifying_key(),
        expiry_cooldown: Duration(30),
        rejection_cooldown: Duration(90),
    }
}

fn command_signer(actor: ActorRef, key: OperationalKeyRef, secret: u8) -> CommandSigner {
    CommandSigner::from_secret_bytes(actor, key, &[secret; 32])
}

fn signing_scope() -> SigningScope {
    SigningScope {
        deployment_domain: DEPLOYMENT_DOMAIN,
        intended_provider: PROVIDER,
        relationship: RelationshipRef::from_u128_for_test(SENDER.0 ^ RECIPIENT.0),
    }
}

fn sender(command: ProtocolCommand, key: u128) -> SignedCommandBytes {
    command_signer(ActorRef::Sender(SENDER), OperationalKeyRef(1), 1)
        .sign(
            signing_scope(),
            ProtocolVersion(2),
            IdempotencyKey(key),
            command,
        )
        .unwrap()
}

fn recipient(command: ProtocolCommand, key: u128) -> SignedCommandBytes {
    command_signer(ActorRef::Recipient(RECIPIENT), OperationalKeyRef(2), 2)
        .sign(
            signing_scope(),
            ProtocolVersion(2),
            IdempotencyKey(key),
            command,
        )
        .unwrap()
}

fn provider_signer() -> ProviderSigner {
    ProviderSigner::from_secret_bytes(PROVIDER, OperationalKeyRef(3), &[3; 32])
}

fn registry() -> KeyRegistry {
    let mut registry = KeyRegistry::default();
    for (reference, actor, key) in [
        (
            OperationalKeyRef(1),
            ActorRef::Sender(SENDER),
            command_signer(ActorRef::Sender(SENDER), OperationalKeyRef(1), 1).verifying_key_bytes(),
        ),
        (
            OperationalKeyRef(2),
            ActorRef::Recipient(RECIPIENT),
            command_signer(ActorRef::Recipient(RECIPIENT), OperationalKeyRef(2), 2)
                .verifying_key_bytes(),
        ),
        (
            OperationalKeyRef(3),
            ActorRef::Provider(PROVIDER),
            provider_signer().verifying_key_bytes(),
        ),
    ] {
        registry
            .register(reference, actor, key, CanonicalTime(0))
            .unwrap();
    }
    registry
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
        self.initialize_key_registry(&registry(), CanonicalTime(0))?;
        self.configure_ingress(DEPLOYMENT_DOMAIN, &policy)?;
        let handle = self.receive_signed(&command, DEPLOYMENT_DOMAIN, || now, policy)?;
        let outcome = self.process_received(&handle)?;
        self.sign_artifacts_batch(&provider_signer(), CanonicalTime(0), Duration(30_000), 100)?;
        match outcome {
            cs_mail_storage_postgres::ReceivedOutcome::Protocol(o) => Ok(*o),
            cs_mail_storage_postgres::ReceivedOutcome::Refused(e) => Err(e.into_error()),
            _ => unreachable!(),
        }
    }
}

fn native_declarations() -> MessageDeclarations {
    MessageDeclarations {
        purpose: DeclaredPurpose::Known(KnownPurpose::Transactional),
        origin: OriginDeclaration {
            mode: OriginMode::HumanInitiated,
            authority: DeclarationAuthority::NativeSender(OperationalKeyRef(1)),
        },
        payload_schema: None,
    }
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
#[allow(clippy::too_many_lines)]
fn durable_sequence_survives_reconnect_and_outbox_leases_recover() {
    let url = database_url();
    let key = aggregate_key("durable");
    let engine = PostgresEngine::connect(&url, &key, &state(1), UNIT).unwrap();
    let issued = engine
        .execute(
            sender(
                ProtocolCommand::IssueRequestTerms {
                    quote_id: QuoteId(1),
                    declaration_digest: None,
                },
                1,
            ),
            CanonicalTime(1),
            policy(),
        )
        .unwrap();
    let terms = match issued.transition.terms_outcome.unwrap() {
        TermsOutcome::ChargeRequired(terms) => terms,
        TermsOutcome::NoChargeRequired | TermsOutcome::ExistingRequest(_) => {
            panic!("expected bonded terms")
        }
    };
    engine
        .execute(
            sender(
                ProtocolCommand::CreateRequest {
                    request_id: RequestId(1),

                    message_id: MessageId(1),
                    terms,
                    payment_method: [9; 32],
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
        .store_trusted_content(
            &encrypt(
                &sender_content_key,
                recipient_key,
                ContentBinding {
                    wire_version: WireVersion(1),
                    content_ref: ContentRef(1),
                    message_id: MessageId(1),
                    sender: SENDER,
                    recipient: RECIPIENT,
                    protocol_version: ProtocolVersion(2),
                    relationship: RelationshipRef::from_u128_for_test(SENDER.0 ^ RECIPIENT.0),
                    content_scope: ContentScopeRef::from_u128_for_test(1),
                    sender_certificate: ContentCertificateDigest([1; 32]),
                    declarations: native_declarations(),
                    message_valid_until: MessageValidityUntil(CanonicalTime(1_000)),
                    capability: None,
                },
                b"encrypted before upload",
                CanonicalTime(2),
                CanonicalTime(1_000),
            )
            .unwrap(),
            cs_mail_primitives::RetentionPolicyVersion(1),
        )
        .unwrap();
    confirm_capture(&engine, RequestId(1), 2);
    let admission = sender(
        ProtocolCommand::AdmitRequest {
            request_id: RequestId(1),
            expected_request_version: Version(0),
            content_ref: ContentRef(1),
            delivery_intent_ref: DeliveryIntentRef(1),
            declaration_digest: message_declaration_digest(&native_declarations()).unwrap(),
            message_valid_until: MessageValidityUntil(CanonicalTime(1_000)),
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
        .claim_work(
            cs_mail_storage_postgres::WorkQueue::Delivery,
            CanonicalTime(4),
            Duration(10),
            10,
        )
        .unwrap();
    assert_eq!(claimed.len(), 2);
    assert!(claimed.iter().any(|item| matches!(
        item.payload,
        cs_mail_storage_postgres::WorkPayload::Effect(
            EffectIntent::EstablishRequestSolicitation { .. }
        )
    )));
    assert!(
        engine
            .claim_work(
                cs_mail_storage_postgres::WorkQueue::Delivery,
                CanonicalTime(5),
                Duration(10),
                10
            )
            .unwrap()
            .is_empty()
    );
    let reclaimed = engine
        .claim_work(
            cs_mail_storage_postgres::WorkQueue::Delivery,
            CanonicalTime(14),
            Duration(10),
            10,
        )
        .unwrap();
    assert_eq!(reclaimed.len(), 2);
    assert!(
        engine
            .complete_work(&reclaimed[0], CanonicalTime(15))
            .unwrap()
    );

    drop(engine);
    let reopened = PostgresEngine::connect(&url, &key, &state(1), UNIT).unwrap();
    let before_acceptance = reopened.snapshot().unwrap();
    assert!(matches!(
        before_acceptance.state.requests[&RequestId(1)].lifecycle,
        RequestLifecycle::Open(_)
    ));
    assert_eq!(
        before_acceptance.ledger.balance(Account::RequestEscrow(
            before_acceptance.payments[&RequestId(1)].capture.id
        )),
        Money::from_minor_units(10)
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
        accepted.ledger.balance(Account::RefundPayable(
            accepted.payments[&RequestId(1)]
                .refund()
                .operation()
                .unwrap()
                .id
        )),
        Money::from_minor_units(10)
    );
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
#[allow(clippy::too_many_lines)]
fn admission_requires_durable_correctly_scoped_ciphertext() {
    let url = database_url();
    let engine = PostgresEngine::connect(&url, aggregate_key("content"), &state(2), UNIT).unwrap();
    let terms = match engine
        .execute(
            sender(
                ProtocolCommand::IssueRequestTerms {
                    quote_id: QuoteId(44),
                    declaration_digest: None,
                },
                40,
            ),
            CanonicalTime(1),
            policy(),
        )
        .unwrap()
        .transition
        .terms_outcome
        .unwrap()
    {
        TermsOutcome::ChargeRequired(terms) => terms,
        TermsOutcome::NoChargeRequired | TermsOutcome::ExistingRequest(_) => {
            panic!("expected bonded terms")
        }
    };
    engine
        .execute(
            sender(
                ProtocolCommand::CreateRequest {
                    request_id: RequestId(44),

                    message_id: MessageId(44),
                    terms,
                    payment_method: [9; 32],
                },
                41,
            ),
            CanonicalTime(2),
            policy(),
        )
        .unwrap();
    confirm_capture(&engine, RequestId(44), 2);
    let admission = || {
        sender(
            ProtocolCommand::AdmitRequest {
                request_id: RequestId(44),
                expected_request_version: Version(0),
                content_ref: ContentRef(44),
                delivery_intent_ref: DeliveryIntentRef(44),
                declaration_digest: message_declaration_digest(&native_declarations()).unwrap(),
                message_valid_until: MessageValidityUntil(CanonicalTime(100)),
            },
            42,
        )
    };
    let handle = engine
        .receive_signed(
            &admission(),
            DEPLOYMENT_DOMAIN,
            || CanonicalTime(3),
            policy(),
        )
        .unwrap();
    // Uploading after receipt, before processing, must not backdate availability.
    let (sender_content_key, _) = EndpointSecretKey::generate(ContentKeyRef(45));
    let (_, public) = EndpointSecretKey::generate(ContentKeyRef(44));
    let record = encrypt(
        &sender_content_key,
        public,
        ContentBinding {
            wire_version: WireVersion(1),
            content_ref: ContentRef(44),
            message_id: MessageId(44),
            sender: SENDER,
            recipient: RECIPIENT,
            protocol_version: ProtocolVersion(2),
            relationship: RelationshipRef::from_u128_for_test(SENDER.0 ^ RECIPIENT.0),
            content_scope: ContentScopeRef::from_u128_for_test(44),
            sender_certificate: ContentCertificateDigest([1; 32]),
            declarations: native_declarations(),
            message_valid_until: MessageValidityUntil(CanonicalTime(100)),
            capability: None,
        },
        b"ciphertext only",
        CanonicalTime(2),
        CanonicalTime(100),
    )
    .unwrap();
    engine
        .store_trusted_content(&record, cs_mail_primitives::RetentionPolicyVersion(1))
        .unwrap();
    let cs_mail_storage_postgres::ReceivedOutcome::Protocol(refusal) =
        engine.process_received(&handle).unwrap()
    else {
        panic!("expected cancellation")
    };
    assert_eq!(
        refusal.admission_failure,
        Some(cs_mail_protocol::admission::AdmissionFailure::ContentMissing)
    );
    assert!(matches!(
        engine.snapshot().unwrap().state.requests[&RequestId(44)].lifecycle,
        RequestLifecycle::Cancelled { .. }
    ));
    let replay = engine
        .execute(admission(), CanonicalTime(3), policy())
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.admission_failure, refusal.admission_failure);
    assert!(engine.snapshot().unwrap().messages.is_empty());
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn database_row_lock_serializes_conflicting_decisions() {
    let url = database_url();
    let key = aggregate_key("race");
    let first = PostgresEngine::connect(&url, &key, &state(3), UNIT).unwrap();
    let second = PostgresEngine::connect(&url, &key, &state(3), UNIT).unwrap();
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
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
#[allow(clippy::too_many_lines)]
fn lane_admission_is_bond_free_replay_safe_and_atomically_revoked_by_block() {
    let url = database_url();
    let engine = PostgresEngine::connect(&url, aggregate_key("lane"), &state(4), UNIT).unwrap();
    let recipient_signing = SigningKey::from_bytes(&[2; 32]);
    let grant = LaneGrant {
        id: LaneId(90),
        subject: LaneSubject::Native(cs_mail_privacy::ScopedHandle([1; 32])),
        sender: SENDER,
        recipient: RECIPIENT,
        protocol_version: ProtocolVersion(2),
        deployment_domain: DEPLOYMENT_DOMAIN,
        intended_provider: PROVIDER,
        recipient_operational_key: OperationalKeyRef(2),
        purpose: DeclaredPurpose::Known(KnownPurpose::Transactional),
        origin: Some(OriginMode::HumanInitiated),
        declaration_authority: DeclarationAuthorityConstraint::NativeSender,
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
        .initialize_key_registry(&registry(), CanonicalTime(0))
        .unwrap();
    engine
        .configure_ingress(DEPLOYMENT_DOMAIN, &policy())
        .unwrap();
    let handle = engine
        .receive_grant(
            &signed,
            IdempotencyKey(90),
            DEPLOYMENT_DOMAIN,
            || CanonicalTime(1),
            policy(),
        )
        .unwrap();
    assert!(matches!(
        engine.process_received(&handle).unwrap(),
        cs_mail_storage_postgres::ReceivedOutcome::Lane(_)
    ));

    let (sender_content_key, _) = EndpointSecretKey::generate(ContentKeyRef(90));
    let (_, recipient_content_key) = EndpointSecretKey::generate(ContentKeyRef(91));
    engine
        .store_trusted_content(
            &encrypt(
                &sender_content_key,
                recipient_content_key,
                ContentBinding {
                    wire_version: WireVersion(1),
                    content_ref: ContentRef(90),
                    message_id: MessageId(90),
                    sender: SENDER,
                    recipient: RECIPIENT,
                    protocol_version: ProtocolVersion(2),
                    relationship: RelationshipRef::from_u128_for_test(SENDER.0 ^ RECIPIENT.0),
                    content_scope: ContentScopeRef::from_u128_for_test(90),
                    sender_certificate: ContentCertificateDigest([1; 32]),
                    declarations: native_declarations(),
                    message_valid_until: MessageValidityUntil(CanonicalTime(1_000)),
                    capability: Some(LaneId(90)),
                },
                b"lane ciphertext",
                CanonicalTime(1),
                CanonicalTime(1_000),
            )
            .unwrap(),
            cs_mail_primitives::RetentionPolicyVersion(1),
        )
        .unwrap();
    let evidence = LaneEvidence::Native(cs_mail_privacy::ScopedHandle([1; 32]));
    let admission = BondFreeAdmission {
        wire_version: WireVersion(1),
        sender: SENDER,
        recipient: RECIPIENT,
        message_id: MessageId(90),
        content_ref: ContentRef(90),
        delivery_intent_ref: DeliveryIntentRef(90),
        declarations: native_declarations(),
        message_valid_until: MessageValidityUntil(CanonicalTime(1_000)),
        capability: Some(LaneId(90)),
        evidence: Some(evidence),
        idempotency_key: IdempotencyKey(91),
        protocol_version: ProtocolVersion(2),
        deployment_domain: DEPLOYMENT_DOMAIN,
        intended_provider: PROVIDER,
        authentication: AdmissionAuthentication::NativeKey(OperationalKeyRef(1)),
    };
    assert!(
        !sign_and_admit(&engine, &admission, CanonicalTime(2))
            .unwrap()
            .replayed
    );
    assert!(
        sign_and_admit(&engine, &admission, CanonicalTime(3))
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
        .store_trusted_content(
            &encrypt(
                &sender_content_key,
                second_recipient_content_key,
                ContentBinding {
                    wire_version: WireVersion(1),
                    content_ref: ContentRef(91),
                    message_id: MessageId(91),
                    sender: SENDER,
                    recipient: RECIPIENT,
                    protocol_version: ProtocolVersion(2),
                    relationship: RelationshipRef::from_u128_for_test(SENDER.0 ^ RECIPIENT.0),
                    content_scope: ContentScopeRef::from_u128_for_test(91),
                    sender_certificate: ContentCertificateDigest([1; 32]),
                    declarations: native_declarations(),
                    message_valid_until: MessageValidityUntil(CanonicalTime(1_000)),
                    capability: None,
                },
                b"accepted ciphertext",
                CanonicalTime(3),
                CanonicalTime(1_000),
            )
            .unwrap(),
            cs_mail_primitives::RetentionPolicyVersion(1),
        )
        .unwrap();
    let accepted_admission = BondFreeAdmission {
        wire_version: WireVersion(1),
        sender: SENDER,
        recipient: RECIPIENT,
        message_id: MessageId(91),
        content_ref: ContentRef(91),
        delivery_intent_ref: DeliveryIntentRef(91),
        declarations: native_declarations(),
        message_valid_until: MessageValidityUntil(CanonicalTime(1_000)),
        capability: None,
        evidence: None,
        idempotency_key: IdempotencyKey(95),
        protocol_version: ProtocolVersion(2),
        deployment_domain: DEPLOYMENT_DOMAIN,
        intended_provider: PROVIDER,
        authentication: AdmissionAuthentication::NativeKey(OperationalKeyRef(1)),
    };
    assert!(matches!(
        sign_and_admit(&engine, &accepted_admission, CanonicalTime(3))
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
        sign_and_admit(&engine, &blocked, CanonicalTime(5)),
        Err(StorageError::Protocol(ProtocolError::ContactBlocked))
    ));
}

fn confirm_capture(engine: &PostgresEngine, id: RequestId, at: u64) {
    use cs_mail_finance::PaymentProvider;
    let request = engine.snapshot().unwrap().payments[&id].clone();
    let mut provider = cs_mail_finance::SimulatedProvider::new([7; 32]);
    let receipt = provider.submit(&request.capture, false).unwrap();
    engine
        .confirm_request_payment(id, receipt, CanonicalTime(at), policy())
        .unwrap();
}

fn open_request(engine: &PostgresEngine) {
    let issued = engine
        .execute(
            sender(
                ProtocolCommand::IssueRequestTerms {
                    quote_id: QuoteId(1),
                    declaration_digest: None,
                },
                1,
            ),
            CanonicalTime(1),
            policy(),
        )
        .unwrap();
    let terms = match issued.transition.terms_outcome.unwrap() {
        TermsOutcome::ChargeRequired(terms) => terms,
        TermsOutcome::NoChargeRequired | TermsOutcome::ExistingRequest(_) => {
            panic!("expected bonded terms")
        }
    };
    engine
        .execute(
            sender(
                ProtocolCommand::CreateRequest {
                    request_id: RequestId(1),

                    message_id: MessageId(1),
                    terms,
                    payment_method: [9; 32],
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
        .store_trusted_content(
            &encrypt(
                &sender_content_key,
                recipient_key,
                ContentBinding {
                    wire_version: WireVersion(1),
                    content_ref: ContentRef(1),
                    message_id: MessageId(1),
                    sender: SENDER,
                    recipient: RECIPIENT,
                    protocol_version: ProtocolVersion(2),
                    relationship: RelationshipRef::from_u128_for_test(SENDER.0 ^ RECIPIENT.0),
                    content_scope: ContentScopeRef::from_u128_for_test(1),
                    sender_certificate: ContentCertificateDigest([1; 32]),
                    declarations: native_declarations(),
                    message_valid_until: MessageValidityUntil(CanonicalTime(1_000)),
                    capability: None,
                },
                b"encrypted before upload",
                CanonicalTime(2),
                CanonicalTime(1_000),
            )
            .unwrap(),
            cs_mail_primitives::RetentionPolicyVersion(1),
        )
        .unwrap();
    confirm_capture(engine, RequestId(1), 2);
    let admission = sender(
        ProtocolCommand::AdmitRequest {
            request_id: RequestId(1),
            expected_request_version: Version(0),
            content_ref: ContentRef(1),
            delivery_intent_ref: DeliveryIntentRef(1),
            declaration_digest: message_declaration_digest(&native_declarations()).unwrap(),
            message_valid_until: MessageValidityUntil(CanonicalTime(1_000)),
        },
        3,
    );
    engine
        .execute(admission, CanonicalTime(3), policy())
        .unwrap();
}

fn financial_command(
    engine: &PostgresEngine,
    command: cs_mail_finance::ProgramCommand,
    at: CanonicalTime,
) -> cs_mail_finance::ProgramOutcome {
    let revision = engine.financial_program(UNIT).unwrap().revision;
    let signed = cs_mail_finance::SignedProgramCommand::sign(
        cs_mail_finance::FinancialScope::new(
            [7; 32],
            cs_mail_primitives::ProviderRef(30),
            cs_mail_primitives::ProgramRef(1),
            [9; 32],
            cs_mail_primitives::ProtocolVersion(2),
        ),
        UNIT,
        IdempotencyKey(u128::MAX - u128::from(revision)),
        revision,
        command,
        &[42; 32],
    )
    .unwrap();
    engine.execute_financial_command(&signed, at).unwrap()
}
#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
#[allow(clippy::too_many_lines)] // One complete request-to-quarter-to-provider recovery scenario.
fn forfeiture_distribution_and_payment_survive_reconnect() {
    use cs_mail_finance::{
        DAY_MILLIS, EligibilityPolicy, IntentionalActivity, MembershipStatus, PaymentProvider,
        ProgramCommand, ProgramOutcome, QuarterSchedule, SignedProgramCommand, SimulatedProvider,
    };
    use cs_mail_primitives::{FinancialEventId, MemberId};
    let url = database_url();
    let key = aggregate_key("finance");
    let engine = PostgresEngine::connect(&url, &key, &state(90), UNIT).unwrap();
    let mut provider = SimulatedProvider::new([7; 32]);
    engine
        .configure_financial_program(
            cs_mail_finance::FinancialScope::new(
                [7; 32],
                cs_mail_primitives::ProviderRef(30),
                cs_mail_primitives::ProgramRef(1),
                [9; 32],
                cs_mail_primitives::ProtocolVersion(2),
            ),
            UNIT,
            SigningKey::from_bytes(&[42; 32]).verifying_key().to_bytes(),
            provider.verifying_key(),
        )
        .unwrap();
    let schedule = QuarterSchedule::utc(
        1970,
        1,
        EligibilityPolicy {
            version: PolicyVersion(1),
            minimum_tenure: Duration(30 * DAY_MILLIS),
            minimum_active_days: 1,
        },
    )
    .unwrap();
    financial_command(
        &engine,
        ProgramCommand::PublishQuarter(schedule.clone()),
        CanonicalTime(0),
    );
    for member in 1..=2 {
        financial_command(
            &engine,
            ProgramCommand::Enroll {
                member: MemberId(member),
                identity_digest: [u8::try_from(member).unwrap(); 32],
                status: MembershipStatus {
                    opted_in: true,
                    verified: true,
                    suspended: false,
                },
            },
            CanonicalTime(0),
        );
    }
    let revision = engine.financial_program(UNIT).unwrap().revision;
    let unauthorized = SignedProgramCommand::sign(
        cs_mail_finance::FinancialScope::new(
            [7; 32],
            cs_mail_primitives::ProviderRef(30),
            cs_mail_primitives::ProgramRef(1),
            [9; 32],
            cs_mail_primitives::ProtocolVersion(2),
        ),
        UNIT,
        IdempotencyKey(1),
        revision,
        ProgramCommand::FinalizeQuarter(schedule.id),
        &[43; 32],
    )
    .unwrap();
    assert!(
        engine
            .execute_financial_command(&unauthorized, schedule.cutoff)
            .is_err()
    );
    assert_eq!(engine.financial_program(UNIT).unwrap().revision, revision);
    open_request(&engine);
    engine
        .execute(
            recipient(
                ProtocolCommand::RejectRelationship {
                    expected_version: Version(0),
                },
                4,
            ),
            CanonicalTime(4),
            policy(),
        )
        .unwrap();
    let capture = engine.snapshot().unwrap().payments[&RequestId(1)]
        .capture
        .id;
    assert_eq!(
        engine
            .financial_program(UNIT)
            .unwrap()
            .ledger()
            .balance(Account::PendingForfeiture(capture)),
        Money::from_minor_units(8)
    );
    financial_command(
        &engine,
        ProgramCommand::ClearMaturity {
            source: capture,
            evidence: FinancialEventId(1),
        },
        CanonicalTime(15),
    );
    for member in 1..=2 {
        financial_command(
            &engine,
            ProgramCommand::RecordActivity {
                member: MemberId(member),
                activity: IntentionalActivity::Read,
            },
            CanonicalTime(DAY_MILLIS),
        );
    }
    let revision = engine.financial_program(UNIT).unwrap().revision;
    let finalize = SignedProgramCommand::sign(
        cs_mail_finance::FinancialScope::new(
            [7; 32],
            cs_mail_primitives::ProviderRef(30),
            cs_mail_primitives::ProgramRef(1),
            [9; 32],
            cs_mail_primitives::ProtocolVersion(2),
        ),
        UNIT,
        IdempotencyKey(u128::MAX / 2),
        revision,
        ProgramCommand::FinalizeQuarter(schedule.id),
        &[42; 32],
    )
    .unwrap();
    let outcome = engine
        .execute_financial_command(&finalize, schedule.cutoff)
        .unwrap();
    let ProgramOutcome::Quarter(ref allocation) = outcome else {
        panic!("expected quarter")
    };
    assert_eq!(allocation.each, Money::from_minor_units(4));
    assert_eq!(allocation.members, vec![MemberId(1), MemberId(2)]);
    drop(engine);
    let engine = PostgresEngine::connect(&url, &key, &state(90), UNIT).unwrap();
    assert_eq!(
        engine
            .execute_financial_command(&finalize, schedule.cutoff)
            .unwrap(),
        outcome
    );
    let program = engine.financial_program(UNIT).unwrap();
    assert_eq!(program.payables().count(), 2);
    let payable = program.payables().next().unwrap().clone();
    financial_command(
        &engine,
        ProgramCommand::PreparePayout {
            allocation: payable.id,
            destination: [9; 32],
            minimum: Money::from_minor_units(5),
        },
        schedule.cutoff,
    );
    assert!(
        engine
            .financial_program(UNIT)
            .unwrap()
            .payables()
            .all(|p| p.lifecycle.pending().is_none())
    );
    financial_command(
        &engine,
        ProgramCommand::PreparePayout {
            allocation: payable.id,
            destination: [9; 32],
            minimum: Money::from_minor_units(1),
        },
        schedule.cutoff,
    );
    let pending: Vec<_> = engine
        .financial_program(UNIT)
        .unwrap()
        .payables()
        .filter_map(|p| p.lifecycle.pending().map(|o| (p.id, o.clone())))
        .collect();
    assert_eq!(pending.len(), 1);
    provider.lose_next_response();
    assert!(provider.submit(&pending[0].1, false).is_err());
    assert_eq!(
        engine
            .financial_program(UNIT)
            .unwrap()
            .ledger()
            .balance(Account::MemberPayable(payable.id)),
        payable.amount
    );
    let receipt = provider.lookup(pending[0].1.id).unwrap().unwrap();
    engine
        .confirm_member_payment(UNIT, payable.id, &receipt, schedule.cutoff)
        .unwrap();
    let settled = engine.financial_program(UNIT).unwrap();
    engine
        .confirm_member_payment(UNIT, payable.id, &receipt, schedule.cutoff)
        .unwrap();
    assert_eq!(engine.financial_program(UNIT).unwrap(), settled);
    assert_eq!(
        engine
            .financial_program(UNIT)
            .unwrap()
            .ledger()
            .balance(Account::MemberPayable(payable.id)),
        Money::ZERO
    );
    assert!(
        engine
            .financial_program(UNIT)
            .unwrap()
            .payables()
            .all(|p| p.lifecycle.pending().is_none())
    );
    assert_eq!(
        engine
            .financial_program(UNIT)
            .unwrap()
            .ledger()
            .total_value(),
        Ok(Money::ZERO)
    );
}

#[test]
#[ignore = "requires native multi-session CS_MAIL_TEST_DATABASE_URL"]
fn concurrent_quarter_finalization_records_one_allocation() {
    use cs_mail_finance::{
        EligibilityPolicy, ProgramCommand, QuarterSchedule, SignedProgramCommand,
    };
    let url = database_url();
    let key = aggregate_key("quarter-race");
    let first = PostgresEngine::connect(&url, &key, &state(100), UNIT).unwrap();
    let second = PostgresEngine::connect(&url, &key, &state(100), UNIT).unwrap();
    first
        .configure_financial_program(
            cs_mail_finance::FinancialScope::new(
                [7; 32],
                cs_mail_primitives::ProviderRef(30),
                cs_mail_primitives::ProgramRef(1),
                [9; 32],
                cs_mail_primitives::ProtocolVersion(2),
            ),
            UNIT,
            SigningKey::from_bytes(&[42; 32]).verifying_key().to_bytes(),
            cs_mail_finance::SimulatedProvider::new([7; 32]).verifying_key(),
        )
        .unwrap();
    let schedule = QuarterSchedule::utc(
        1970,
        1,
        EligibilityPolicy {
            version: PolicyVersion(1),
            minimum_tenure: Duration(0),
            minimum_active_days: 1,
        },
    )
    .unwrap();
    financial_command(
        &first,
        ProgramCommand::PublishQuarter(schedule.clone()),
        CanonicalTime(0),
    );
    let revision = first.financial_program(UNIT).unwrap().revision;
    let command = SignedProgramCommand::sign(
        cs_mail_finance::FinancialScope::new(
            [7; 32],
            cs_mail_primitives::ProviderRef(30),
            cs_mail_primitives::ProgramRef(1),
            [9; 32],
            cs_mail_primitives::ProtocolVersion(2),
        ),
        UNIT,
        IdempotencyKey(100),
        revision,
        ProgramCommand::FinalizeQuarter(schedule.id),
        &[42; 32],
    )
    .unwrap();
    let other = command.clone();
    let cutoff = schedule.cutoff;
    let a = thread::spawn(move || first.execute_financial_command(&command, cutoff));
    let b = thread::spawn(move || second.execute_financial_command(&other, cutoff));
    let results = [a.join().unwrap().unwrap(), b.join().unwrap().unwrap()];
    assert_eq!(results[0], results[1]);
    let reopened = PostgresEngine::connect(&url, &key, &state(100), UNIT).unwrap();
    let program = reopened.financial_program(UNIT).unwrap();
    assert_eq!(program.revision, revision + 1);
    assert!(program.quarter(schedule.id).is_some());
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn domain_owners_are_separate_and_history_survives_alias_registration() {
    let url = database_url();
    let key = aggregate_key("domain-owners");
    let first = PostgresEngine::connect(&url, &key, &state(500), UNIT).unwrap();
    first
        .initialize_key_registry(&registry(), CanonicalTime(0))
        .unwrap();
    open_request(&first);
    let alias = ProtocolState::initial_scoped(
        RelationshipRef::from_u128_for_test(500),
        state(500).relationship.history,
        ProtocolIdentity(11),
        RECIPIENT,
        CanonicalTime(10),
    );
    let second = PostgresEngine::connect(&url, aggregate_key("alias"), &alias, UNIT).unwrap();
    assert_eq!(
        second.snapshot().unwrap().history,
        first.snapshot().unwrap().history
    );
    assert!(second.snapshot().unwrap().state.requests.is_empty());
    assert!(second.snapshot().unwrap().payments.is_empty());
    let mut db = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    let count: i64 = db
        .query_one("SELECT count(*) FROM cs_request_histories", &[])
        .unwrap()
        .get(0);
    assert_eq!(count, 1);
    let row = db
        .query_one(
            "SELECT relationship_state FROM cs_relationship_aggregates WHERE aggregate_key=$1",
            &[&key],
        )
        .unwrap();
    let state = row.get::<_, postgres::types::Json<serde_json::Value>>(0).0;
    assert!(state.get("history").is_none());
    assert!(state["requests"]["1"].get("capture").is_none());
    let original = first.snapshot().unwrap();
    drop(first);
    let reopened = PostgresEngine::connect(&url, &key, &crate::state(500), UNIT).unwrap();
    assert_eq!(reopened.snapshot().unwrap(), original);
    assert_eq!(original.messages.len(), 1);
    assert_eq!(original.payments.len(), 1);
    db.execute(
        "UPDATE cs_relationship_aggregates SET protocol_format_version=3 WHERE aggregate_key=$1",
        &[&key],
    )
    .unwrap();
    assert!(matches!(
        reopened.snapshot(),
        Err(StorageError::UnsupportedStoredProtocolFormat(3))
    ));
}

fn sign_and_admit(
    engine: &PostgresEngine,
    admission: &BondFreeAdmission,
    now: CanonicalTime,
) -> Result<cs_mail_storage_postgres::BondFreeAdmissionOutcome, StorageError> {
    let signed = cs_mail_capabilities::SignedBondFreeAdmission {
        admission: admission.clone(),
        signature: SigningKey::from_bytes(&[1; 32])
            .sign(&admission.signing_bytes().unwrap())
            .to_bytes(),
    };
    let handle = engine.receive_message(&signed, DEPLOYMENT_DOMAIN, || now, policy())?;
    match engine.process_received(&handle)? {
        cs_mail_storage_postgres::ReceivedOutcome::Message(o) => Ok(o),
        cs_mail_storage_postgres::ReceivedOutcome::Refused(e) => Err(e.into_error()),
        _ => unreachable!(),
    }
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn receipt_survives_restart_and_timely_decision_precedes_queued_expiry() {
    use cs_mail_security::ReceiptKind;
    use cs_mail_storage_postgres::ReceivedOutcome;
    use sha2::Digest;
    let url = database_url();
    let key = aggregate_key("received-order");
    let engine = PostgresEngine::connect(&url, &key, &state(90), UNIT).unwrap();
    open_request(&engine);
    let scheduler = command_signer(ActorRef::Scheduler(PROVIDER), OperationalKeyRef(4), 4);
    engine
        .register_operational_key(
            OperationalKeyRef(4),
            ActorRef::Scheduler(PROVIDER),
            scheduler.verifying_key_bytes(),
            CanonicalTime(4),
        )
        .unwrap();
    let decision = recipient(
        ProtocolCommand::AcceptRelationship {
            expected_version: Version(0),
        },
        100,
    );
    let handle = engine
        .receive_signed(&decision, DEPLOYMENT_DOMAIN, || CanonicalTime(50), policy())
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().state.relationship.state,
        RelationshipState::Unknown
    );
    assert!(matches!(
        engine.revoke_operational_key(OperationalKeyRef(2), Version(0), CanonicalTime(51)),
        Err(StorageError::PendingCommands)
    ));
    assert!(matches!(
        engine.run_retention(CanonicalTime(2000), 100),
        Err(StorageError::PendingCommands)
    ));
    let expiry = scheduler
        .sign(
            signing_scope(),
            ProtocolVersion(2),
            IdempotencyKey(101),
            ProtocolCommand::ExpireRequest {
                request_id: RequestId(1),
                expected_request_version: Version(1),
            },
        )
        .unwrap();
    let expired = engine
        .receive_signed(&expiry, DEPLOYMENT_DOMAIN, || CanonicalTime(54), policy())
        .unwrap();
    assert!(handle.position() < expired.position());
    drop(engine);
    let reopened = PostgresEngine::connect(&url, &key, &state(90), UNIT).unwrap();
    // Processing the later handle must apply the earlier receipt first.
    let outcome = reopened.process_received(&expired).unwrap();
    assert!(
        matches!(outcome,ReceivedOutcome::Protocol(ref o) if o.transition.protocol_events.is_empty())
    );
    let snapshot = reopened.snapshot().unwrap();
    assert!(matches!(
        snapshot.state.requests[&RequestId(1)].lifecycle,
        RequestLifecycle::Accepted { .. }
    ));
    assert!(
        snapshot.payments[&RequestId(1)]
            .refund()
            .operation()
            .is_some()
    );
    assert!(reopened.receipt(&handle).unwrap().is_none());
    // Restarting the signer recovers both evidence intents, including the no-op.
    reopened
        .sign_artifacts_batch(&provider_signer(), CanonicalTime(0), Duration(30_000), 100)
        .unwrap();
    let receipt = reopened.receipt(&handle).unwrap().unwrap();
    assert_eq!(receipt.payload.received_at, CanonicalTime(50));
    assert_eq!(receipt.payload.journal_position, handle.position());
    assert_eq!(
        reopened.receipt(&expired).unwrap().unwrap().payload.kind,
        ReceiptKind::NoChange
    );
    reopened
        .revoke_operational_key(OperationalKeyRef(2), Version(0), CanonicalTime(55))
        .unwrap();
    let replay = reopened
        .receive_signed(
            &decision,
            DEPLOYMENT_DOMAIN,
            || panic!("replay must not acquire a new time"),
            policy(),
        )
        .unwrap();
    assert_eq!(reopened.receipt(&replay).unwrap().unwrap(), receipt);
    let replayed = reopened.process_received(&replay).unwrap();
    assert!(matches!(replayed,ReceivedOutcome::Protocol(ref o) if o.replayed));
    assert_eq!(
        receipt.payload.outcome_digest.0,
        <[u8; 32]>::from(sha2::Sha256::digest(serde_json::to_vec(&replayed).unwrap()))
    );
    assert_eq!(reopened.snapshot().unwrap(), snapshot);
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn detached_verification_cannot_cross_durable_revocation_and_refusals_have_receipts() {
    let url = database_url();
    let engine =
        PostgresEngine::connect(&url, aggregate_key("revocation"), &state(91), UNIT).unwrap();
    engine
        .initialize_key_registry(&registry(), CanonicalTime(0))
        .unwrap();
    engine
        .configure_ingress(DEPLOYMENT_DOMAIN, &policy())
        .unwrap();
    let command = recipient(
        ProtocolCommand::AcceptRelationship {
            expected_version: Version(0),
        },
        201,
    );
    registry()
        .verify(
            &command,
            CanonicalTime(1),
            ProtocolVersion(2),
            signing_scope(),
        )
        .unwrap();
    engine
        .revoke_operational_key(OperationalKeyRef(2), Version(0), CanonicalTime(2))
        .unwrap();
    assert!(matches!(
        engine.receive_signed(&command, DEPLOYMENT_DOMAIN, || CanonicalTime(3), policy()),
        Err(StorageError::Security(_))
    ));
    assert!(!engine.process_next_received().unwrap());
    let invalid = sender(
        ProtocolCommand::AdmitRequest {
            request_id: RequestId(999),
            expected_request_version: Version(0),
            content_ref: ContentRef(1),
            delivery_intent_ref: DeliveryIntentRef(1),
            declaration_digest: message_declaration_digest(&native_declarations()).unwrap(),
            message_valid_until: MessageValidityUntil(CanonicalTime(100)),
        },
        202,
    );
    let handle = engine
        .receive_signed(&invalid, DEPLOYMENT_DOMAIN, || CanonicalTime(3), policy())
        .unwrap();
    assert!(matches!(
        engine.process_received(&handle).unwrap(),
        cs_mail_storage_postgres::ReceivedOutcome::Refused(_)
    ));
    // The signer authority was pinned at receipt; later key revocation cannot erase this work.
    engine
        .revoke_operational_key(OperationalKeyRef(3), Version(0), CanonicalTime(4))
        .unwrap();
    engine
        .sign_artifacts_batch(&provider_signer(), CanonicalTime(0), Duration(30_000), 100)
        .unwrap();
    let receipt = engine.receipt(&handle).unwrap().unwrap();
    assert_eq!(receipt.payload.kind, cs_mail_security::ReceiptKind::Refused);
    receipt
        .verify(&provider_signer().verifying_key_bytes())
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().revision, 0);
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
#[allow(clippy::too_many_lines)]
fn policy_change_after_upload_cancels_preparation_with_void_or_full_refund() {
    use cs_mail_primitives::{ExtensionCriticality, NamespacedIdentifier, PayloadSchema};
    use cs_mail_protocol::admission::{AdmissionFailure, AdmissionPolicy};
    for captured in [false, true] {
        let url = database_url();
        let engine =
            PostgresEngine::connect(&url, aggregate_key("policy-change"), &state(92), UNIT)
                .unwrap();
        let supported = AdmissionPolicy {
            version: Version(1),
            supported_critical_schemas: std::collections::BTreeSet::from([
                NamespacedIdentifier::new("com.example", "invoice", 1).unwrap(),
            ]),
        };
        engine.configure_admission_policy(&supported).unwrap();
        let issued = engine
            .execute(
                sender(
                    ProtocolCommand::IssueRequestTerms {
                        quote_id: QuoteId(1),
                        declaration_digest: None,
                    },
                    1,
                ),
                CanonicalTime(1),
                policy(),
            )
            .unwrap();
        let Some(TermsOutcome::ChargeRequired(terms)) = issued.transition.terms_outcome else {
            panic!("expected terms")
        };
        engine
            .execute(
                sender(
                    ProtocolCommand::CreateRequest {
                        request_id: RequestId(1),
                        message_id: MessageId(1),
                        terms,
                        payment_method: [9; 32],
                    },
                    2,
                ),
                CanonicalTime(2),
                policy(),
            )
            .unwrap();
        if captured {
            confirm_capture(&engine, RequestId(1), 2);
        }
        let mut declarations = native_declarations();
        declarations.payload_schema = Some(PayloadSchema {
            id: NamespacedIdentifier::new("com.example", "invoice", 1).unwrap(),
            criticality: ExtensionCriticality::Critical,
        });
        supported.evaluate(&declarations).unwrap();
        let (secret, _) = EndpointSecretKey::generate(ContentKeyRef(1));
        let (_, public) = EndpointSecretKey::generate(ContentKeyRef(2));
        let record = encrypt(
            &secret,
            public,
            ContentBinding {
                wire_version: WireVersion(1),
                content_ref: ContentRef(1),
                message_id: MessageId(1),
                sender: SENDER,
                recipient: RECIPIENT,
                protocol_version: ProtocolVersion(2),
                relationship: signing_scope().relationship,
                content_scope: ContentScopeRef::from_u128_for_test(1),
                sender_certificate: ContentCertificateDigest([1; 32]),
                declarations: declarations.clone(),
                message_valid_until: MessageValidityUntil(CanonicalTime(100)),
                capability: None,
            },
            b"ciphertext",
            CanonicalTime(2),
            CanonicalTime(100),
        )
        .unwrap();
        engine
            .store_trusted_content(&record, RetentionPolicyVersion(1))
            .unwrap();
        engine
            .configure_admission_policy(&AdmissionPolicy {
                version: Version(2),
                ..AdmissionPolicy::default()
            })
            .unwrap();
        let command = sender(
            ProtocolCommand::AdmitRequest {
                request_id: RequestId(1),
                expected_request_version: Version(0),
                content_ref: ContentRef(1),
                delivery_intent_ref: DeliveryIntentRef(1),
                declaration_digest: message_declaration_digest(&declarations).unwrap(),
                message_valid_until: MessageValidityUntil(CanonicalTime(100)),
            },
            3,
        );
        let handle = engine
            .receive_signed(&command, DEPLOYMENT_DOMAIN, || CanonicalTime(3), policy())
            .unwrap();
        assert!(matches!(
            engine.configure_admission_policy(&AdmissionPolicy {
                version: Version(3),
                ..supported.clone()
            }),
            Err(StorageError::PendingCommands)
        ));
        // Reopening with the identical policy is safe even with a pending inbox.
        engine
            .configure_admission_policy(&AdmissionPolicy {
                version: Version(2),
                ..AdmissionPolicy::default()
            })
            .unwrap();
        let outcome = engine.process_received(&handle).unwrap();
        assert!(
            matches!(outcome,cs_mail_storage_postgres::ReceivedOutcome::Protocol(ref o) if o.admission_failure==Some(AdmissionFailure::UnsupportedCriticalExtension))
        );
        let snapshot = engine.snapshot().unwrap();
        assert!(matches!(
            snapshot.state.requests[&RequestId(1)].lifecycle,
            RequestLifecycle::Cancelled { .. }
        ));
        assert_eq!(snapshot.history.level, 0);
        assert!(snapshot.messages.is_empty());
        let financials = &snapshot.payments[&RequestId(1)];
        if captured {
            assert_eq!(
                financials.refund().operation().unwrap().amount,
                Money::from_minor_units(10)
            );
        } else {
            assert_eq!(
                financials.capture_status(),
                cs_mail_finance::CaptureStatus::CancellationRequested
            );
        }
        engine
            .configure_admission_policy(&AdmissionPolicy {
                version: Version(3),
                ..supported
            })
            .unwrap();
        let replay = engine
            .receive_signed(&command, DEPLOYMENT_DOMAIN, || CanonicalTime(4), policy())
            .unwrap();
        assert_eq!(
            engine.process_received(&replay).unwrap(),
            match outcome {
                cs_mail_storage_postgres::ReceivedOutcome::Protocol(mut o) => {
                    o.replayed = true;
                    cs_mail_storage_postgres::ReceivedOutcome::Protocol(o)
                }
                _ => unreachable!(),
            }
        );
        assert_eq!(engine.snapshot().unwrap(), snapshot);
    }
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn issued_quote_remains_bound_to_its_original_signing_authority() {
    let url = database_url();
    let engine =
        PostgresEngine::connect(&url, aggregate_key("quote-authority"), &state(93), UNIT).unwrap();
    let issued = engine
        .execute(
            sender(
                ProtocolCommand::IssueRequestTerms {
                    quote_id: QuoteId(1),
                    declaration_digest: None,
                },
                1,
            ),
            CanonicalTime(1),
            policy(),
        )
        .unwrap();
    let Some(TermsOutcome::ChargeRequired(terms)) = issued.transition.terms_outcome else {
        panic!("expected terms")
    };
    let original = engine.signed_quote(QuoteId(1)).unwrap();
    let replacement = ProviderSigner::from_secret_bytes(PROVIDER, OperationalKeyRef(5), &[5; 32]);
    engine
        .register_operational_key(
            OperationalKeyRef(5),
            ActorRef::Provider(PROVIDER),
            replacement.verifying_key_bytes(),
            CanonicalTime(2),
        )
        .unwrap();
    engine
        .revoke_operational_key(OperationalKeyRef(3), Version(0), CanonicalTime(2))
        .unwrap();
    let command = sender(
        ProtocolCommand::CreateRequest {
            request_id: RequestId(1),
            message_id: MessageId(1),
            terms,
            payment_method: [9; 32],
        },
        2,
    );
    let handle = engine
        .receive_signed(&command, DEPLOYMENT_DOMAIN, || CanonicalTime(3), policy())
        .unwrap();
    assert!(matches!(
        engine.process_received(&handle).unwrap(),
        cs_mail_storage_postgres::ReceivedOutcome::Protocol(_)
    ));
    engine
        .sign_artifacts_batch(&replacement, CanonicalTime(0), Duration(30_000), 100)
        .unwrap();
    assert_eq!(engine.signed_quote(QuoteId(1)).unwrap(), original);
    assert_eq!(
        engine
            .receipt(&handle)
            .unwrap()
            .unwrap()
            .provider_operational_key,
        OperationalKeyRef(5)
    );
    let mut altered = command;
    altered.signature[0] ^= 1;
    assert!(matches!(
        engine.receive_signed(&altered, DEPLOYMENT_DOMAIN, || CanonicalTime(4), policy()),
        Err(StorageError::DuplicateConflict)
    ));
}

fn preparing_request(engine: &PostgresEngine, id: u128) {
    let issued = engine
        .execute(
            sender(
                ProtocolCommand::IssueRequestTerms {
                    quote_id: QuoteId(id),
                    declaration_digest: None,
                },
                id * 10,
            ),
            CanonicalTime(1),
            policy(),
        )
        .unwrap();
    let Some(TermsOutcome::ChargeRequired(terms)) = issued.transition.terms_outcome else {
        panic!("expected charge");
    };
    engine
        .execute(
            sender(
                ProtocolCommand::CreateRequest {
                    request_id: RequestId(id),
                    message_id: MessageId(id),
                    terms,
                    payment_method: [9; 32],
                },
                id * 10 + 1,
            ),
            CanonicalTime(2),
            policy(),
        )
        .unwrap();
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn reclaimed_work_fences_stale_completion_retry_and_foreign_engines() {
    use cs_mail_storage_postgres::{WorkFailure, WorkQueue};
    let url = database_url();
    let key = aggregate_key("fencing");
    let engine = PostgresEngine::connect(&url, &key, &state(200), UNIT).unwrap();
    preparing_request(&engine, 1);
    let old = engine
        .claim_work(
            WorkQueue::RequestPayments,
            CanonicalTime(3),
            Duration(10),
            1,
        )
        .unwrap()
        .remove(0);
    drop(engine);
    let engine = PostgresEngine::connect(&url, &key, &state(200), UNIT).unwrap();
    let new = engine
        .claim_work(
            WorkQueue::RequestPayments,
            CanonicalTime(13),
            Duration(10),
            1,
        )
        .unwrap()
        .remove(0);
    assert_eq!(old.id, new.id);
    assert_eq!(new.attempts, 2);
    assert!(!engine.complete_work(&old, CanonicalTime(14)).unwrap());
    assert!(
        !engine
            .retry_work(&old, CanonicalTime(14), WorkFailure::DependencyUnavailable)
            .unwrap()
    );
    let mut other_state = state(201);
    other_state.relationship.key.reference = RelationshipRef::from_u128_for_test(999);
    other_state.relationship.key.sender = ProtocolIdentity(999);
    let other =
        PostgresEngine::connect(&url, aggregate_key("foreign"), &other_state, UNIT).unwrap();
    assert!(!other.complete_work(&new, CanonicalTime(14)).unwrap());
    assert!(
        engine
            .block_work(&new, CanonicalTime(14), WorkFailure::InvalidEvidence)
            .unwrap()
    );
    assert!(
        engine
            .claim_work(
                WorkQueue::RequestPayments,
                CanonicalTime(100),
                Duration(10),
                1
            )
            .unwrap()
            .is_empty()
    );
    assert!(
        engine
            .resume_work(WorkQueue::RequestPayments, new.id, CanonicalTime(100))
            .unwrap()
    );
    let resumed = engine
        .claim_work(
            WorkQueue::RequestPayments,
            CanonicalTime(100),
            Duration(10),
            1,
        )
        .unwrap()
        .remove(0);
    assert!(engine.complete_work(&resumed, CanonicalTime(101)).unwrap());
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
#[allow(clippy::too_many_lines)] // One end-to-end recovery sequence.
fn request_erasure_preserves_pending_refunds_and_receipt_replay_identity() {
    use cs_mail_finance::PaymentProvider;
    use cs_mail_storage_postgres::{LifecyclePolicy, ReceivedOutcome, WorkQueue};
    let url = database_url();
    let key = aggregate_key("erasure");
    let engine = PostgresEngine::connect(&url, &key, &state(202), UNIT).unwrap();
    engine
        .configure_lifecycle(&LifecyclePolicy {
            version: RetentionPolicyVersion(1),
            formation_lifetime: Duration(0),
            replay_lifetime: Duration(0),
        })
        .unwrap();
    preparing_request(&engine, 1);
    let cancel = sender(
        ProtocolCommand::CancelPreparingRequest {
            request_id: RequestId(1),
            expected_request_version: Version(0),
            reason: cs_mail_protocol::CancellationReason::SenderRequested,
        },
        99,
    );
    engine
        .execute(cancel.clone(), CanonicalTime(3), policy())
        .unwrap();
    let handle = engine
        .receive_signed(&cancel, DEPLOYMENT_DOMAIN, || CanonicalTime(4), policy())
        .unwrap();
    let original_receipt = engine.receipt(&handle).unwrap().unwrap();
    engine
        .update_retention(
            "request",
            "1",
            Some(CanonicalTime(20)),
            None,
            CanonicalTime(4),
            "request-specific dispute",
        )
        .unwrap();
    engine.run_retention(CanonicalTime(10), 100).unwrap();
    assert!(
        engine
            .snapshot()
            .unwrap()
            .state
            .requests
            .contains_key(&RequestId(1))
    );
    engine.run_retention(CanonicalTime(20), 100).unwrap();
    let snapshot = engine.snapshot().unwrap();
    assert!(snapshot.state.requests.is_empty());
    let mut provider = cs_mail_finance::SimulatedProvider::new([7; 32]);
    let capture = provider
        .submit(&snapshot.payments[&RequestId(1)].capture, false)
        .unwrap();
    engine
        .confirm_request_payment(RequestId(1), capture, CanonicalTime(21), policy())
        .unwrap();
    let snapshot = engine.snapshot().unwrap();
    assert!(snapshot.state.requests.is_empty());
    let refund = snapshot.payments[&RequestId(1)]
        .refund()
        .operation()
        .unwrap()
        .clone();
    assert_eq!(refund.amount, Money::from_minor_units(10));
    let receipt = provider.submit(&refund, false).unwrap();
    engine
        .confirm_request_payment(RequestId(1), receipt, CanonicalTime(22), policy())
        .unwrap();
    assert_eq!(
        engine
            .snapshot()
            .unwrap()
            .ledger
            .balance(Account::RefundPayable(refund.id)),
        Money::ZERO
    );
    // All provider obligations are now completed; acknowledge the replay-safe leftover work.
    for item in engine
        .claim_work(
            WorkQueue::RequestPayments,
            CanonicalTime(23),
            Duration(10),
            100,
        )
        .unwrap()
    {
        engine.complete_work(&item, CanonicalTime(23)).unwrap();
    }
    engine
        .sign_artifacts_batch(&provider_signer(), CanonicalTime(23), Duration(10), 100)
        .unwrap();
    engine.run_retention(CanonicalTime(1100), 100).unwrap();
    let replay = engine
        .receive_signed(&cancel, DEPLOYMENT_DOMAIN, || CanonicalTime(1101), policy())
        .unwrap();
    assert!(
        matches!(engine.process_received(&replay).unwrap(),ReceivedOutcome::Retired{outcome_digest} if outcome_digest==original_receipt.payload.outcome_digest)
    );
    assert_eq!(engine.receipt(&replay).unwrap().unwrap(), original_receipt);
    let repeated_capture = provider
        .lookup(snapshot.payments[&RequestId(1)].capture.id)
        .unwrap()
        .unwrap();
    assert!(matches!(
        engine
            .confirm_request_payment(
                RequestId(1),
                repeated_capture,
                CanonicalTime(1102),
                policy()
            )
            .unwrap(),
        ReceivedOutcome::Retired { .. }
    ));
    let mut db = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    let copies:i64=db.query_one("SELECT count(*) FROM cs_received_commands WHERE aggregate_key=$1 AND (operation IS NOT NULL OR authority IS NOT NULL OR policy IS NOT NULL)",&[&key]).unwrap().get(0);
    assert_eq!(copies, 0);
    let header = db
        .query_one(
            "SELECT relationship_state FROM cs_relationship_aggregates WHERE aggregate_key=$1",
            &[&key],
        )
        .unwrap()
        .get::<_, postgres::types::Json<serde_json::Value>>(0)
        .0;
    assert!(header.get("requests").is_none());
    assert!(header.get("quotes").is_none());
    assert!(engine.reconcile_restored_deletions().unwrap() > 0);
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn ciphertext_retention_honors_extended_deadlines_and_scoped_holds() {
    let url = database_url();
    let key = aggregate_key("content-retention");
    let engine = PostgresEngine::connect(&url, &key, &state(203), UNIT).unwrap();
    let (secret, _) = EndpointSecretKey::generate(ContentKeyRef(1));
    let (_, public) = EndpointSecretKey::generate(ContentKeyRef(2));
    let record = encrypt(
        &secret,
        public,
        ContentBinding {
            wire_version: WireVersion(1),
            content_ref: ContentRef(1),
            message_id: MessageId(1),
            sender: SENDER,
            recipient: RECIPIENT,
            protocol_version: ProtocolVersion(2),
            relationship: signing_scope().relationship,
            content_scope: ContentScopeRef::from_u128_for_test(1),
            sender_certificate: ContentCertificateDigest([1; 32]),
            declarations: native_declarations(),
            message_valid_until: MessageValidityUntil(CanonicalTime(10)),
            capability: None,
        },
        b"retained",
        CanonicalTime(1),
        CanonicalTime(10),
    )
    .unwrap();
    engine
        .store_trusted_content(&record, RetentionPolicyVersion(1))
        .unwrap();
    engine
        .update_retention(
            "content",
            "1",
            None,
            Some(CanonicalTime(20)),
            CanonicalTime(2),
            "delivery dispute window",
        )
        .unwrap();
    assert_eq!(
        engine.run_retention(CanonicalTime(10), 10).unwrap().deleted,
        0
    );
    engine
        .update_retention(
            "content",
            "1",
            Some(CanonicalTime(30)),
            None,
            CanonicalTime(11),
            "scoped investigation",
        )
        .unwrap();
    assert_eq!(
        engine.run_retention(CanonicalTime(20), 10).unwrap().deleted,
        0
    );
    assert_eq!(
        engine.run_retention(CanonicalTime(30), 10).unwrap().deleted,
        1
    );
    assert!(engine.content(ContentRef(1)).unwrap().is_none());
    // A restored backup containing the old ciphertext cannot resurrect it.
    let mut db = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    db.execute("INSERT INTO cs_encrypted_content(aggregate_key,content_ref,record,expires_at,created_at,retention_policy_version,record_ref) SELECT $1,'1',$2,10,1,1,record_ref FROM cs_retention_records WHERE aggregate_key=$1 AND record_domain='content' AND object_ref='1'",&[&key,&postgres::types::Json(&record)]).unwrap();
    engine.reconcile_restored_deletions().unwrap();
    assert!(engine.content(ContentRef(1)).unwrap().is_none());
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn failed_work_insert_rolls_back_every_owner_but_preserves_received_command() {
    use cs_mail_storage_postgres::ReceivedOutcome;
    let url = database_url();
    let key = aggregate_key("atomic-work");
    let engine = PostgresEngine::connect(&url, &key, &state(204), UNIT).unwrap();
    let issued = engine
        .execute(
            sender(
                ProtocolCommand::IssueRequestTerms {
                    quote_id: QuoteId(1),
                    declaration_digest: None,
                },
                1,
            ),
            CanonicalTime(1),
            policy(),
        )
        .unwrap();
    let Some(TermsOutcome::ChargeRequired(terms)) = issued.transition.terms_outcome else {
        panic!("expected terms");
    };
    let submission = sender(
        ProtocolCommand::CreateRequest {
            request_id: RequestId(1),
            message_id: MessageId(1),
            terms,
            payment_method: [9; 32],
        },
        2,
    );
    let handle = engine
        .receive_signed(
            &submission,
            DEPLOYMENT_DOMAIN,
            || CanonicalTime(2),
            policy(),
        )
        .unwrap();
    let before = engine.snapshot().unwrap();
    let mut db = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    db.batch_execute("CREATE FUNCTION fail_payment_work() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.kind='request-payment' THEN RAISE EXCEPTION 'injected storage failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_payment_work BEFORE INSERT ON cs_work FOR EACH ROW EXECUTE FUNCTION fail_payment_work();").unwrap();
    assert!(matches!(
        engine.process_received(&handle),
        Err(StorageError::Database(_))
    ));
    assert_eq!(engine.snapshot().unwrap(), before);
    let pending: i64 = db
        .query_one(
            "SELECT count(*) FROM cs_received_commands WHERE aggregate_key=$1 AND outcome IS NULL",
            &[&key],
        )
        .unwrap()
        .get(0);
    assert_eq!(pending, 1);
    db.batch_execute(
        "DROP TRIGGER fail_payment_work ON cs_work; DROP FUNCTION fail_payment_work();",
    )
    .unwrap();
    assert!(matches!(
        engine.process_received(&handle).unwrap(),
        ReceivedOutcome::Protocol(_)
    ));
    let after = engine.snapshot().unwrap();
    assert_eq!(after.payments.len(), 1);
    assert_eq!(after.state.requests.len(), 1);
    assert!(after.history.preparation.is_some());
    let work: i64 = db
        .query_one(
            "SELECT count(*) FROM cs_work WHERE aggregate_key=$1 AND kind='request-payment'",
            &[&key],
        )
        .unwrap()
        .get(0);
    assert_eq!(work, 1);
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn quote_replay_keeps_signed_terms_until_its_own_replay_window_expires() {
    use cs_mail_storage_postgres::{LifecyclePolicy, ReceivedOutcome};
    let url = database_url();
    let engine =
        PostgresEngine::connect(&url, aggregate_key("quote-retention"), &state(205), UNIT).unwrap();
    engine
        .configure_lifecycle(&LifecyclePolicy {
            version: RetentionPolicyVersion(1),
            formation_lifetime: Duration(0),
            replay_lifetime: Duration(100),
        })
        .unwrap();
    let submission = sender(
        ProtocolCommand::IssueRequestTerms {
            quote_id: QuoteId(1),
            declaration_digest: None,
        },
        1,
    );
    engine
        .execute(submission.clone(), CanonicalTime(1), policy())
        .unwrap();
    let original = engine.signed_quote(QuoteId(1)).unwrap();
    engine.run_retention(CanonicalTime(22), 100).unwrap();
    assert_eq!(engine.signed_quote(QuoteId(1)).unwrap(), original);
    let replay = engine
        .receive_signed(
            &submission,
            DEPLOYMENT_DOMAIN,
            || CanonicalTime(23),
            policy(),
        )
        .unwrap();
    assert!(matches!(
        engine.process_received(&replay).unwrap(),
        ReceivedOutcome::Protocol(_)
    ));
    engine.run_retention(CanonicalTime(102), 100).unwrap();
    engine.run_retention(CanonicalTime(1022), 100).unwrap();
    assert!(matches!(
        engine.signed_quote(QuoteId(1)),
        Err(StorageError::QuoteMissing)
    ));
    let duplicate = engine.execute(
        sender(
            ProtocolCommand::IssueRequestTerms {
                quote_id: QuoteId(1),
                declaration_digest: None,
            },
            2,
        ),
        CanonicalTime(1023),
        policy(),
    );
    assert!(matches!(
        duplicate,
        Err(StorageError::Protocol(ProtocolError::DuplicateConflict))
    ));
}
