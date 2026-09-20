use super::*;
use cs_mail_accounts::EnrollmentInput;
use cs_mail_application::accounts::PendingEnrollment;
use cs_mail_application::accounts::{EnrollmentEvidence, EnrollmentOutcome};
use cs_mail_primitives::{BillingAccountId, MemberId};
use identity_application::enrollment::{Config, DecisionSigner, EnrollmentService};
use identity_contract::*;
use identity_model::{OidcClientConfig, StaticOidcSessionVerifier, VerifiedOidcSession, time};
use identity_storage_postgres::enrollment::PostgresEnrollmentStore;
use postgres::types::Json;
use std::sync::{Arc, Barrier};

const PRODUCT: &str = "cs-mail/test";
const ISSUER: &str = "https://identity.example.test";
fn key(secret: u8) -> [u8; 32] {
    SigningKey::from_bytes(&[secret; 32])
        .verifying_key()
        .to_bytes()
}
struct Clock(i64);
impl identity_application::enrollment::Clock for Clock {
    fn now(&self) -> Result<i64, Error> {
        Ok(self.0)
    }
}
type Service = EnrollmentService<PostgresEnrollmentStore, StaticOidcSessionVerifier, Clock>;
struct LocalClient {
    service: Arc<Service>,
    runtime: tokio::runtime::Runtime,
}
impl IdentityClient for LocalClient {
    fn call(&self, request: &SignedRequest) -> Result<Response, ClientError> {
        self.runtime
            .block_on(self.service.handle(request))
            .map_err(|e| ClientError::with_source(e.code(), e))
    }
}
fn engine(url: &str, label: &str, sender: ProtocolIdentity) -> PostgresEngine {
    let state = ProtocolState::initial_scoped(
        RelationshipRef::from_u128_for_test(sender.0),
        RequestHistoryRef::from_u128_for_test(sender.0),
        sender,
        RECIPIENT,
        test_time(0),
    );
    let host = PostgresEngine::connect(url, label, &state, UNIT).unwrap();
    let mut providers = KeyRegistry::default();
    providers
        .register(
            OperationalKeyRef(3),
            ActorRef::Provider(PROVIDER),
            key(3),
            test_time(0),
        )
        .unwrap();
    host.initialize_key_registry(&providers, test_time(0))
        .unwrap();
    support::arrangement(&host, &policy()).unwrap();
    host.configure_ingress(DEPLOYMENT_DOMAIN, &policy())
        .unwrap();
    host.accounts(|| test_time(0))
        .configure_identity_service(ISSUER, PRODUCT, key(80))
        .unwrap();
    host
}
fn input(id: u128) -> EnrollmentInput {
    EnrollmentInput {
        bank: cs_mail_finance::BankVerification {
            scope: policy().financial.scope,
            account: BillingAccountId(id),
            member: MemberId(id),
            person: [5; 32],
            bank_token: [u8::try_from(id).unwrap(); 32],
            unit: UNIT,
            version: 1,
            signature: vec![],
        }
        .sign(&[77; 32])
        .unwrap(),
        persona: RECIPIENT,
        actor: ActorRef::Recipient(RECIPIENT),
        key_ref: OperationalKeyRef(101),
        initial_key: key(81),
        maximum_unresolved: 10,
    }
}
fn proofs(pending: &PendingEnrollment) -> (VerifiedOidcSession, SignedBankOwnership) {
    let intent = pending.intent();
    let now = intent.created_at;
    let mut session = VerifiedOidcSession::keycloak(
        "https://login.example.test",
        "login-1",
        "enrollment",
        "session",
        time::unix_seconds_to_timestamp(now),
        time::unix_seconds_to_timestamp(intent.expires_at),
    );
    session.nonce = Some(intent.challenge.clone());
    session.auth_time = Some(time::unix_seconds_to_timestamp(now));
    let bank = SignedBankOwnership::sign(
        BankOwnershipClaims {
            product: PRODUCT.into(),
            operation: intent.operation.clone(),
            challenge: intent.challenge.clone(),
            login_issuer: session.issuer.clone(),
            login_subject: session.subject.clone(),
            bank_digest: intent.bank_digest,
            evidence_ref: "bank-evidence-1".into(),
            ownership: Ownership::Confirmed,
            issued_at: now,
            expires_at: intent.expires_at,
        },
        &[82; 32],
    )
    .unwrap();
    (session, bank)
}
fn service(url: &str, session: VerifiedOidcSession) -> LocalClient {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let now = time::timestamp_to_unix_seconds(&session.issued_at).unwrap();
    let service = runtime.block_on(async {
        let (db, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls)
            .await
            .unwrap();
        tokio::spawn(async move {
            connection.await.unwrap();
        });
        EnrollmentService::new(
            PostgresEnrollmentStore::new(db).await.unwrap(),
            Config {
                issuer: ISSUER.into(),
                product: PRODUCT.into(),
                oidc: OidcClientConfig::keycloak("https://login.example.test", "enrollment"),
                product_key: key(83),
                bank_key: key(82),
            },
            StaticOidcSessionVerifier::new("token", session),
            Clock(now),
            DecisionSigner::new([80; 32]),
        )
        .unwrap()
    });
    LocalClient {
        service: Arc::new(service),
        runtime,
    }
}
fn request(pending: &PendingEnrollment, bank: SignedBankOwnership) -> SignedRequest {
    SignedRequest::sign(
        PRODUCT.into(),
        Request::Enroll {
            intent: pending.intent().clone(),
            oidc_token: "token".into(),
            bank,
            device_proof: pending.intent().prove_possession(&[81; 32]).unwrap(),
        },
        &[83; 32],
    )
    .unwrap()
}
fn decision(client: &LocalClient, wire_request: &SignedRequest) -> SignedDecision {
    let Response::Eligible(issued_decision) = client.call(wire_request).unwrap() else {
        panic!("expected eligible")
    };
    *issued_decision
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn enrollment_survives_lost_responses_restart_and_duplicate_confirmation() {
    let cs = database_url();
    let ids = database_url();
    let host = engine(&cs, "first", SENDER);
    let pending =
        cs_mail_application::accounts::AccountEnrollment::new(&host.accounts(move || test_time(0)))
            .begin("enrollment-1", &input(1))
            .unwrap();
    assert_eq!(
        pending,
        cs_mail_application::accounts::AccountEnrollment::new(&host.accounts(move || test_time(1)))
            .begin("enrollment-1", &input(1))
            .unwrap()
    );
    let mut changed = input(1);
    changed.maximum_unresolved = 2;
    assert!(
        cs_mail_application::accounts::AccountEnrollment::new(&host.accounts(move || test_time(0)))
            .begin("enrollment-1", &changed)
            .is_err()
    );
    let (session, bank) = proofs(&pending);
    let client = service(&ids, session.clone());
    let wire_request = request(&pending, bank);
    let issued_decision = decision(&client, &wire_request); // The first response is lost before cs-mail commits.
    drop(client);
    drop(host);
    let client = service(&ids, session);
    let host = engine(&cs, "first", SENDER);
    let id = cs_mail_application::accounts::reconcile_account_enrollment(
        &host.accounts(|| test_time(0)),
        "enrollment-1",
        &client,
        &[83; 32],
    )
    .unwrap();
    assert_eq!(id, EnrollmentOutcome::Enrolled(pending.account()));
    let id = pending.account();
    let account = host.accounts(|| test_time(0)).product_account(id).unwrap();
    assert_eq!(account.principal, pending.principal());
    assert_eq!(account.billing, BillingAccountId(1));
    assert_eq!(
        host.accounts(|| test_time(0))
            .persona_principal(RECIPIENT)
            .unwrap(),
        pending.principal()
    );
    assert_ne!(account.membership_identity, pending.input().bank.person);
    assert_eq!(
        account.identity.subject_ref.as_str(),
        issued_decision.claims.subject_ref.as_str()
    );
    // Exact replay survives expiration; no new authority is created.
    assert_eq!(
        cs_mail_application::accounts::AccountEnrollment::new(
            &host.accounts(move || test_time(2_000_000))
        )
        .commit(&issued_decision)
        .unwrap(),
        id
    );
    let confirm = SignedRequest::sign(
        PRODUCT.into(),
        Request::Confirm {
            decision: issued_decision.clone(),
        },
        &[83; 32],
    )
    .unwrap();
    assert_eq!(client.call(&confirm).unwrap(), Response::Confirmed); // ACK lost locally.
    assert_eq!(
        cs_mail_application::accounts::confirm_identity_enrollments(
            &host.accounts(|| test_time(0)),
            &client,
            &[83; 32],
            10,
            test_time(0)
        )
        .unwrap()
        .confirmed,
        1
    );
    assert_eq!(
        cs_mail_application::accounts::confirm_identity_enrollments(
            &host.accounts(|| test_time(0)),
            &client,
            &[83; 32],
            10,
            test_time(0)
        )
        .unwrap()
        .confirmed,
        0
    );
    let mut db = postgres::Client::connect(&cs, postgres::NoTls).unwrap();
    assert_eq!(
        db.query_one("SELECT count(*) FROM cs_product_accounts", &[])
            .unwrap()
            .get::<_, i64>(0),
        1
    );
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn service_rejects_wrong_nonce_failed_bank_and_changed_operation() {
    let cs = database_url();
    let ids = database_url();
    let host = engine(&cs, "boundary", SENDER);
    let pending =
        cs_mail_application::accounts::AccountEnrollment::new(&host.accounts(move || test_time(0)))
            .begin("boundary", &input(1))
            .unwrap();
    let (mut session, bank) = proofs(&pending);
    session.nonce = Some("wrong".into());
    let bad = service(&ids, session);
    assert_eq!(
        bad.call(&request(&pending, bank.clone()))
            .unwrap_err()
            .code(),
        Error::Context
    );
    let (session, _) = proofs(&pending);
    let client = service(&ids, session);
    let mut forged = bank.clone();
    forged.claims.evidence_ref = "forged-bank-proof".into();
    assert_eq!(
        client.call(&request(&pending, forged)).unwrap_err().code(),
        Error::Signature
    );
    let issued_decision = decision(&client, &request(&pending, bank.clone()));
    let mut altered = issued_decision.clone();
    altered.claims.intent.account = "attacker".into();
    assert!(
        cs_mail_application::accounts::AccountEnrollment::new(&host.accounts(move || test_time(0)))
            .commit(&altered)
            .is_err()
    );
    assert!(
        cs_mail_application::accounts::AccountEnrollment::new(
            &host.accounts(move || test_time(900_000))
        )
        .commit(&issued_decision)
        .is_err()
    );
    let mut changed = bank;
    changed.claims.evidence_ref = "substituted".into();
    assert_eq!(
        client.call(&request(&pending, changed)).unwrap_err().code(),
        Error::Conflict
    );
    let mut db = postgres::Client::connect(&cs, postgres::NoTls).unwrap();
    assert_eq!(
        db.query_one("SELECT count(*) FROM cs_product_accounts", &[])
            .unwrap()
            .get::<_, i64>(0),
        0
    );
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn account_and_outbox_rollback_together_and_can_retry() {
    let cs = database_url();
    let ids = database_url();
    let host = engine(&cs, "atomic", SENDER);
    let pending =
        cs_mail_application::accounts::AccountEnrollment::new(&host.accounts(move || test_time(0)))
            .begin("atomic", &input(1))
            .unwrap();
    let (session, bank) = proofs(&pending);
    let client = service(&ids, session);
    let issued_decision = decision(&client, &request(&pending, bank));
    let mut db = postgres::Client::connect(&cs, postgres::NoTls).unwrap();
    db.batch_execute("CREATE FUNCTION reject_identity_outbox() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected commit failure'; END $$; CREATE TRIGGER fail_outbox BEFORE INSERT ON cs_identity_outbox FOR EACH ROW EXECUTE FUNCTION reject_identity_outbox()").unwrap();
    assert!(
        cs_mail_application::accounts::AccountEnrollment::new(&host.accounts(move || test_time(0)))
            .commit(&issued_decision)
            .is_err()
    );
    for table in [
        "cs_product_accounts",
        "cs_billing_accounts",
        "cs_persona_owners",
        "cs_identity_outbox",
    ] {
        assert_eq!(
            db.query_one(&format!("SELECT count(*) FROM {table}"), &[])
                .unwrap()
                .get::<_, i64>(0),
            0
        );
    }
    db.batch_execute("DROP TRIGGER fail_outbox ON cs_identity_outbox")
        .unwrap();
    assert_eq!(
        cs_mail_application::accounts::AccountEnrollment::new(&host.accounts(move || test_time(0)))
            .commit(&issued_decision)
            .unwrap(),
        pending.account()
    );
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn concurrent_service_and_local_retries_converge() {
    let cs = database_url();
    let ids = database_url();
    let host = engine(&cs, "race", SENDER);
    let pending =
        cs_mail_application::accounts::AccountEnrollment::new(&host.accounts(move || test_time(0)))
            .begin("race", &input(1))
            .unwrap();
    let (session, bank) = proofs(&pending);
    let wire_request = request(&pending, bank);
    let c1 = service(&ids, session.clone());
    let c2 = service(&ids, session);
    let barrier = Arc::new(Barrier::new(2));
    let threads = [c1, c2]
        .into_iter()
        .map(|client| {
            let wire_request = wire_request.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                decision(&client, &wire_request)
            })
        })
        .collect::<Vec<_>>();
    let results = threads
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results[0], results[1]);
    let other = engine(&cs, "race", SENDER);
    let barrier = Arc::new(Barrier::new(2));
    let threads = [host, other]
        .into_iter()
        .map(|engine| {
            let issued_decision = results[0].clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                cs_mail_application::accounts::AccountEnrollment::new(
                    &engine.accounts(move || test_time(0)),
                )
                .commit(&issued_decision)
                .unwrap()
            })
        })
        .collect::<Vec<_>>();
    for worker in threads {
        assert_eq!(worker.join().unwrap(), pending.account());
    }
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn same_login_cannot_reserve_a_second_account() {
    let cs = database_url();
    let ids = database_url();
    let host = engine(&cs, "duplicate", SENDER);
    let pending =
        cs_mail_application::accounts::AccountEnrollment::new(&host.accounts(move || test_time(0)))
            .begin("first", &input(1))
            .unwrap();
    let (session, bank) = proofs(&pending);
    let client = service(&ids, session);
    decision(&client, &request(&pending, bank));
    let mut other_input = input(2);
    other_input.persona = SENDER;
    other_input.actor = ActorRef::Sender(SENDER);
    other_input.key_ref = OperationalKeyRef(102);
    let p2 =
        cs_mail_application::accounts::AccountEnrollment::new(&host.accounts(move || test_time(0)))
            .begin("second", &other_input)
            .unwrap();
    let (session, bank) = proofs(&p2);
    let client = service(&ids, session);
    assert_eq!(
        client.call(&request(&p2, bank)).unwrap_err().code(),
        Error::Conflict
    );
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn revocation_applies_across_relationships_and_preserves_receipt_authority() {
    let cs = database_url();
    let ids = database_url();
    let host = engine(&cs, "one", SENDER);
    let e2 = engine(&cs, "two", ProtocolIdentity(11));
    let pending =
        cs_mail_application::accounts::AccountEnrollment::new(&host.accounts(move || test_time(0)))
            .begin("keys", &input(1))
            .unwrap();
    let (session, bank) = proofs(&pending);
    let client = service(&ids, session);
    let result = cs_mail_application::accounts::enroll_with_identity(
        &host.accounts(|| test_time(0)),
        "keys",
        EnrollmentEvidence {
            oidc_token: "token".into(),
            bank,
            device_proof: pending.intent().prove_possession(&[81; 32]).unwrap(),
        },
        &client,
        &[83; 32],
    )
    .unwrap();
    assert_eq!(result, EnrollmentOutcome::Enrolled(pending.account()));
    // Both relationships resolve account authority without copying the key into either registry.
    let accepted = host
        .receive_signed(
            &block_command(10, 100, 101, 81),
            DEPLOYMENT_DOMAIN,
            || test_time(1),
            policy(),
        )
        .unwrap();
    e2.receive_signed(
        &block_command(11, 101, 101, 81),
        DEPLOYMENT_DOMAIN,
        || test_time(2),
        policy(),
    )
    .unwrap();
    host.accounts(|| test_time(0))
        .revoke_account_key(
            pending.account(),
            OperationalKeyRef(101),
            Version(0),
            test_time(3),
        )
        .unwrap();
    assert!(
        e2.receive_signed(
            &block_command(11, 102, 101, 81),
            DEPLOYMENT_DOMAIN,
            || test_time(4),
            policy()
        )
        .is_err()
    );
    // Old relationship-local recipient key cannot reappear as fallback.
    let old = block_command(11, 103, 2, 2);
    assert!(
        e2.receive_signed(&old, DEPLOYMENT_DOMAIN, || test_time(4), policy())
            .is_err()
    );
    let mut db = postgres::Client::connect(&cs, postgres::NoTls).unwrap();
    let frozen = db
        .query_one(
            "SELECT authority FROM cs_received_commands WHERE position=$1",
            &[&i64::try_from(accepted.position().0).unwrap()],
        )
        .unwrap()
        .get::<_, Json<cs_mail_security::AuthoritySnapshot>>(0)
        .0;
    assert!(
        frozen
            .active_verifying_key(
                OperationalKeyRef(101),
                ActorRef::Recipient(RECIPIENT),
                test_time(4)
            )
            .is_ok()
    );
    assert!(
        host.accounts(|| test_time(0))
            .account_key_registry(pending.account())
            .unwrap()
            .transparency()
            .verify()
    );
}

#[test]
#[ignore = "requires isolated PostgreSQL and loopback HTTP"]
fn http_transport_preserves_signed_contract_and_reconciliation() {
    let cs = database_url();
    let ids = database_url();
    let host = engine(&cs, "http", SENDER);
    let pending =
        cs_mail_application::accounts::AccountEnrollment::new(&host.accounts(move || test_time(0)))
            .begin("http", &input(1))
            .unwrap();
    let (session, bank) = proofs(&pending);
    let service = service(&ids, session);
    let (address_tx, address_rx) = std::sync::mpsc::channel();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let server = thread::spawn(move || {
        let router = identity_enrollment::router(service.service.clone());
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            address_tx.send(listener.local_addr().unwrap()).unwrap();
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = stop_rx.await;
                })
                .await
                .unwrap();
        });
        drop(service);
    });
    let address = address_rx.recv().unwrap();
    let client = cs_mail_identity_client::HttpIdentityClient::new(&format!(
        "http://{address}/v1/enrollment"
    ))
    .unwrap();
    let result = cs_mail_application::accounts::enroll_with_identity(
        &host.accounts(|| test_time(0)),
        "http",
        EnrollmentEvidence {
            oidc_token: "token".into(),
            bank,
            device_proof: pending.intent().prove_possession(&[81; 32]).unwrap(),
        },
        &client,
        &[83; 32],
    )
    .unwrap();
    assert_eq!(result, EnrollmentOutcome::Enrolled(pending.account()));
    assert_eq!(
        cs_mail_application::accounts::confirm_identity_enrollments(
            &host.accounts(|| test_time(0)),
            &client,
            &[83; 32],
            10,
            test_time(0)
        )
        .unwrap()
        .confirmed,
        1
    );
    assert_eq!(
        cs_mail_application::accounts::reconcile_account_enrollment(
            &host.accounts(|| test_time(0)),
            "http",
            &client,
            &[83; 32]
        )
        .unwrap(),
        EnrollmentOutcome::Enrolled(pending.account())
    );
    stop_tx.send(()).unwrap();
    server.join().unwrap();
}

fn block_command(sender: u128, id: u128, key_ref: u128, secret: u8) -> SignedCommandBytes {
    command_signer(
        ActorRef::Recipient(RECIPIENT),
        OperationalKeyRef(key_ref),
        secret,
    )
    .sign(
        SigningScope {
            deployment_domain: DEPLOYMENT_DOMAIN,
            intended_provider: PROVIDER,
            relationship: RelationshipRef::from_u128_for_test(sender),
        },
        ProtocolVersion(2),
        IdempotencyKey(id),
        ProtocolCommand::BlockRelationship {
            expected_version: Version(0),
        },
    )
    .unwrap()
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn postgres_implements_the_shared_enrollment_contract() {
    let url = database_url();
    let _engine = engine(&url, "conformance", SENDER);
    let clock = cs_mail_test_support::enrollment::TestClock::default();
    let repository =
        cs_mail_storage_postgres::PostgresAccountRepository::connect(&url, clock.clone()).unwrap();
    let mut second = input(2);
    second.persona = SENDER;
    second.actor = ActorRef::Sender(SENDER);
    second.key_ref = OperationalKeyRef(102);
    cs_mail_test_support::enrollment::assert_repository_contract(
        &repository,
        &clock,
        input(1),
        second,
        test_time(0),
    );
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn expired_or_reviewed_attempts_renew_the_same_binding() {
    let cs = database_url();
    let ids = database_url();
    let host = engine(&cs, "renewal", SENDER);
    let clock = cs_mail_test_support::enrollment::TestClock::default();
    let repo = host.accounts(clock.clone());
    let first = {
        clock.set(test_time(0));
        cs_mail_application::accounts::AccountEnrollment::new(&repo).begin("renew", &input(1))
    }
    .unwrap();
    let (session, bank) = proofs(&first);
    let client = service(&ids, session);
    let original = decision(&client, &request(&first, bank));
    let renewed = {
        clock.set(test_time(900_000));
        cs_mail_application::accounts::AccountEnrollment::new(&repo).renew("renew")
    }
    .unwrap();
    let (session, bank) = proofs(&renewed);
    let client = service(&ids, session);
    let next = decision(&client, &request(&renewed, bank));
    assert_eq!(original.claims.subject_ref, next.claims.subject_ref);
    assert!(
        {
            clock.set(test_time(900_000));
            cs_mail_application::accounts::AccountEnrollment::new(&repo).commit(&original)
        }
        .is_err()
    );
    assert_eq!(
        {
            clock.set(test_time(900_000));
            cs_mail_application::accounts::AccountEnrollment::new(&repo).commit(&next)
        }
        .unwrap(),
        first.account()
    );
    // A separate review outcome is durable, and only a new authenticated attempt can replace it.
    let ids = database_url();
    let cs = database_url();
    let host = engine(&cs, "review", SENDER);
    let clock = cs_mail_test_support::enrollment::TestClock::default();
    let repo = host.accounts(clock.clone());
    let pending = {
        clock.set(test_time(0));
        cs_mail_application::accounts::AccountEnrollment::new(&repo).begin("review", &input(1))
    }
    .unwrap();
    let (session, mut bank) = proofs(&pending);
    bank.claims.ownership = Ownership::ReviewRequired;
    bank = SignedBankOwnership::sign(bank.claims, &[82; 32]).unwrap();
    let client = service(&ids, session);
    let review = request(&pending, bank);
    assert_eq!(client.call(&review).unwrap(), Response::ReviewRequired);
    assert_eq!(client.call(&review).unwrap(), Response::ReviewRequired);
    let next = {
        clock.set(test_time(900_000));
        cs_mail_application::accounts::AccountEnrollment::new(&repo).renew("review")
    }
    .unwrap();
    let (session, bank) = proofs(&next);
    let client = service(&ids, session);
    let accepted = decision(&client, &request(&next, bank));
    assert_eq!(
        {
            clock.set(test_time(900_000));
            cs_mail_application::accounts::AccountEnrollment::new(&repo).commit(&accepted)
        }
        .unwrap(),
        pending.account()
    );
}
