use super::*;
use cs_mail_accounts::EnrollmentInput;
use cs_mail_application::accounts::PendingEnrollment;
use cs_mail_application::accounts::review::{ReviewError, ReviewOutcome, review_enrollments};
use cs_mail_application::accounts::{EnrollmentEvidence, EnrollmentOutcome};
use cs_mail_operations::{OperatorKeyRef, ReviewCommand, SignedOperatorCommand};
use cs_mail_primitives::{BillingAccountId, MemberId};
use identity_application::enrollment::{
    Config, DecisionSigner, EnrollmentService, evidence::EvidenceKey,
};
use identity_contract::{evidence::*, *};
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
    let host = PostgresDeployment::connect(url)
        .unwrap()
        .relationship(label, &state, UNIT)
        .unwrap();
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
    support::bootstrap(host.deployment(), policy().financial.scope, policy().unit).unwrap();
    host.configure_ingress(DEPLOYMENT_DOMAIN, &policy())
        .unwrap();
    host
}
/// Account storage of the host's deployment, with its trusted clock fixed at `at`.
fn accounts_at(
    host: &PostgresEngine,
    at: u64,
) -> cs_mail_storage_postgres::PostgresAccountRepository {
    host.deployment().accounts(move || test_time(at))
}
fn input(id: u128) -> EnrollmentInput {
    EnrollmentInput {
        bank: cs_mail_finance::BankVerification {
            scope: policy().financial.scope,
            account: BillingAccountId(id),
            member: MemberId(id),
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
/// A person's evidence, as they state it and as their bank reports it.
#[derive(Clone, Copy)]
struct Person {
    name: &'static str,
    date_of_birth: &'static str,
    address: &'static str,
    phone: &'static str,
}
const ALICE: Person = Person {
    name: "Alice Example",
    date_of_birth: "1980-02-29",
    address: "1 Main Street, Springfield",
    phone: "+1 555 010 0001",
};
impl Person {
    fn applicant(self) -> ApplicantClaims {
        ApplicantClaims {
            name: self.name.into(),
            date_of_birth: self.date_of_birth.into(),
            address: self.address.into(),
            phone: self.phone.into(),
        }
    }
}
fn proofs(pending: &PendingEnrollment) -> (VerifiedOidcSession, SignedBankOwnership) {
    proofs_for(pending, "login-1", ALICE)
}
/// A fresh login by `login` and the bank's attestation that `owner` holds the account
/// the enrollment reserved.
fn proofs_for(
    pending: &PendingEnrollment,
    login: &str,
    owner: Person,
) -> (VerifiedOidcSession, SignedBankOwnership) {
    attest(
        pending.intent(),
        pending.input().bank.bank_token,
        login,
        owner,
    )
}
/// A fresh login by `login` and the bank's attestation that `owner` holds bank account
/// `token`, for `intent`.
fn attest(
    intent: &EnrollmentIntent,
    token: [u8; 32],
    login: &str,
    owner: Person,
) -> (VerifiedOidcSession, SignedBankOwnership) {
    let now = intent.created_at;
    let mut session = VerifiedOidcSession::keycloak(
        "https://login.example.test",
        login,
        "enrollment",
        "session",
        time::unix_seconds_to_timestamp(now),
        time::unix_seconds_to_timestamp(intent.expires_at),
    );
    session.nonce = Some(intent.challenge.clone());
    session.auth_time = Some(time::unix_seconds_to_timestamp(now));
    let bank = SignedBankOwnership::sign(
        BankOwnershipClaims {
            product: intent.product.clone(),
            operation: intent.operation.clone(),
            challenge: intent.challenge.clone(),
            login_issuer: session.issuer.clone(),
            login_subject: session.subject.clone(),
            bank_digest: intent.bank_digest,
            account_token: token,
            owners: vec![ReportedOwner {
                names: vec![owner.name.into()],
                addresses: vec![owner.address.into()],
                phones: vec![owner.phone.into()],
            }],
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
    service_for(url, session, PRODUCT)
}
fn service_for(url: &str, session: VerifiedOidcSession, product: &str) -> LocalClient {
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
                product: product.into(),
                oidc: OidcClientConfig::keycloak("https://login.example.test", "enrollment"),
                product_key: key(83),
                bank_key: key(82),
            },
            StaticOidcSessionVerifier::new("token", session),
            Clock(now),
            DecisionSigner::new([80; 32]),
            EvidenceKey::new([84; 32]),
        )
        .unwrap()
    });
    LocalClient {
        service: Arc::new(service),
        runtime,
    }
}
fn request(pending: &PendingEnrollment, bank: SignedBankOwnership) -> SignedRequest {
    request_for(pending, bank, ALICE)
}
fn request_for(
    pending: &PendingEnrollment,
    bank: SignedBankOwnership,
    applicant: Person,
) -> SignedRequest {
    SignedRequest::sign(
        PRODUCT.into(),
        Request::Enroll {
            intent: pending.intent().clone(),
            oidc_token: "token".into(),
            bank,
            device_proof: pending.intent().prove_possession(&[81; 32]).unwrap(),
            applicant: applicant.applicant(),
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
    let pending = cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&host, 0))
        .begin("enrollment-1", &input(1))
        .unwrap();
    assert_eq!(
        pending,
        cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&host, 1))
            .begin("enrollment-1", &input(1))
            .unwrap()
    );
    let mut changed = input(1);
    changed.maximum_unresolved = 2;
    assert!(
        cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&host, 0))
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
        &accounts_at(&host, 0),
        "enrollment-1",
        &client,
        &[83; 32],
    )
    .unwrap();
    assert_eq!(id, EnrollmentOutcome::Enrolled(pending.account()));
    let id = pending.account();
    let account = accounts_at(&host, 0).product_account(id).unwrap();
    assert_eq!(account.principal, pending.principal());
    assert_eq!(account.billing, BillingAccountId(1));
    assert_eq!(
        accounts_at(&host, 0).persona_principal(RECIPIENT).unwrap(),
        pending.principal()
    );
    assert_eq!(
        account.identity.subject_ref.as_str(),
        issued_decision.claims.subject_ref.as_str()
    );
    // Exact replay survives expiration; no new authority is created.
    assert_eq!(
        cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&host, 2_000_000))
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
            &accounts_at(&host, 0),
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
            &accounts_at(&host, 0),
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
    let pending = cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&host, 0))
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
        cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&host, 0))
            .commit(&altered)
            .is_err()
    );
    assert!(
        cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&host, 900_000))
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
    let pending = cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&host, 0))
        .begin("atomic", &input(1))
        .unwrap();
    let (session, bank) = proofs(&pending);
    let client = service(&ids, session);
    let issued_decision = decision(&client, &request(&pending, bank));
    let mut db = postgres::Client::connect(&cs, postgres::NoTls).unwrap();
    db.batch_execute("CREATE FUNCTION reject_identity_outbox() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected commit failure'; END $$; CREATE TRIGGER fail_outbox BEFORE INSERT ON cs_identity_outbox FOR EACH ROW EXECUTE FUNCTION reject_identity_outbox()").unwrap();
    assert!(
        cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&host, 0))
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
        cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&host, 0))
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
    let pending = cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&host, 0))
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
                cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&engine, 0))
                    .commit(&issued_decision)
                    .unwrap()
            })
        })
        .collect::<Vec<_>>();
    for worker in threads {
        assert_eq!(worker.join().unwrap(), pending.account());
    }
}

/// Another enrollment's input: its own billing account, member, persona and key, and
/// bank account `bank`.
fn other_input(id: u128, persona: ProtocolIdentity, key: u128, bank: u8) -> EnrollmentInput {
    let mut other = input(id);
    let mut verification = other.bank.clone();
    verification.bank_token = [bank; 32];
    verification.signature = vec![];
    other.bank = verification.sign(&[77; 32]).unwrap();
    other.persona = persona;
    other.actor = ActorRef::Sender(persona);
    other.key_ref = OperationalKeyRef(key);
    other
}
/// Signs a review command with operator key 1's `secret` and forwards it.
fn review(
    host: &PostgresEngine,
    client: &LocalClient,
    command: ReviewCommand,
    secret: [u8; 32],
) -> Result<ReviewOutcome, ReviewError> {
    let record = host.deployment().deployment_record().unwrap().unwrap();
    let signed = SignedOperatorCommand::sign(
        record.record().deployment_domain,
        OperatorKeyRef(1),
        IdempotencyKey(7),
        command,
        &secret,
    )
    .unwrap();
    review_enrollments(
        &record.operator_authority(),
        &signed,
        PRODUCT,
        client,
        &[83; 32],
    )
}
fn begin(
    host: &PostgresEngine,
    at: u64,
    operation: &str,
    input: &EnrollmentInput,
) -> PendingEnrollment {
    cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(host, at))
        .begin(operation, input)
        .unwrap()
}
fn commit(
    host: &PostgresEngine,
    at: u64,
    decision: &SignedDecision,
) -> cs_mail_primitives::AccountId {
    cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(host, at))
        .commit(decision)
        .unwrap()
}
fn product_accounts(url: &str) -> i64 {
    postgres::Client::connect(url, postgres::NoTls)
        .unwrap()
        .query_one("SELECT count(*) FROM cs_product_accounts", &[])
        .unwrap()
        .get(0)
}
/// Alice enrolls through `login-1` with bank account `[1; 32]`; returns her enrollment.
fn enroll_alice(host: &PostgresEngine, ids: &str) -> PendingEnrollment {
    let alice = begin(host, 0, "alice", &input(1));
    let (session, bank) = proofs(&alice);
    let client = service(ids, session);
    commit(host, 0, &decision(&client, &request(&alice, bank)));
    alice
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn overlapping_evidence_is_held_and_a_denial_is_final() {
    // Claim: an enrollment whose evidence matches an existing account on two fields
    // cannot become eligible until an operator decides, and once denied it cannot
    // activate, even through a fresh attempt. Falsified by an eligible decision or a
    // second account for it. Matters because review is the only gate between one person
    // and a second account.
    let cs = database_url();
    let ids = database_url();
    let host = engine(&cs, "review-deny", SENDER);
    let alice = enroll_alice(&host, &ids);
    // Alice's name and date of birth, with another login, bank account, address and phone.
    let twin = Person {
        address: "9 Elm Road, Shelbyville",
        phone: "(555) 010-0009",
        ..ALICE
    };
    let second = begin(&host, 0, "second", &other_input(2, SENDER, 102, 2));
    let (session, bank) = proofs_for(&second, "login-2", twin);
    let client = service(&ids, session);
    let held = request_for(&second, bank, twin);
    assert_eq!(client.call(&held).unwrap(), Response::UnderReview);
    assert_eq!(client.call(&held).unwrap(), Response::UnderReview);
    let ReviewOutcome::Open(cases) = review(
        &host,
        &client,
        ReviewCommand::Open,
        support::FIXTURE_OPERATOR_SECRET,
    )
    .unwrap() else {
        panic!("expected open cases")
    };
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].account, second.intent().account);
    assert_eq!(
        cases[0].matches,
        vec![ReviewMatch {
            account: alice.intent().account.clone(),
            fields: vec![EvidenceField::Name, EvidenceField::DateOfBirth],
        }]
    );
    let deny = ReviewCommand::Resolve {
        case: cases[0].id.clone(),
        resolution: ReviewResolution::Deny,
    };
    // A key the deployment does not list cannot decide.
    assert!(matches!(
        review(&host, &client, deny.clone(), [91; 32]),
        Err(ReviewError::Operations(
            cs_mail_operations::OperationsError::Unauthorized
        ))
    ));
    review(&host, &client, deny, support::FIXTURE_OPERATOR_SECRET).unwrap();
    assert_eq!(
        cs_mail_application::accounts::reconcile_account_enrollment(
            &accounts_at(&host, 0),
            "second",
            &client,
            &[83; 32]
        )
        .unwrap(),
        EnrollmentOutcome::Denied
    );
    let renewed =
        cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&host, 900_000))
            .renew("second")
            .unwrap();
    let (session, bank) = proofs_for(&renewed, "login-2", twin);
    let client = service(&ids, session);
    assert_eq!(
        client.call(&request_for(&renewed, bank, twin)).unwrap(),
        Response::Denied
    );
    assert_eq!(product_accounts(&cs), 1);
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn a_shared_bank_account_is_held_and_an_approval_activates_once() {
    // Claim: an enrollment sharing only a bank account with an existing account is held
    // for review, and an operator's approval lets exactly that enrollment activate, with
    // its own funding source. Falsified by an eligible decision before approval, a
    // decision that can be reversed, or an approved enrollment that cannot activate.
    // Matters because the decision is CSQD's and must bind.
    let cs = database_url();
    let ids = database_url();
    let host = engine(&cs, "review-approve", SENDER);
    let alice = enroll_alice(&host, &ids);
    let bob = Person {
        name: "Bob Example",
        date_of_birth: "1979-07-01",
        address: "3 Oak Lane, Springfield",
        phone: "+1 555 010 0002",
    };
    let second = begin(&host, 0, "bob", &other_input(2, SENDER, 102, 1));
    let (session, bank) = proofs_for(&second, "login-2", bob);
    let client = service(&ids, session);
    assert_eq!(
        client.call(&request_for(&second, bank, bob)).unwrap(),
        Response::UnderReview
    );
    let ReviewOutcome::Open(cases) = review(
        &host,
        &client,
        ReviewCommand::Open,
        support::FIXTURE_OPERATOR_SECRET,
    )
    .unwrap() else {
        panic!("expected open cases")
    };
    assert_eq!(
        cases[0].matches,
        vec![ReviewMatch {
            account: alice.intent().account.clone(),
            fields: vec![EvidenceField::BankAccount],
        }]
    );
    let approve = ReviewCommand::Resolve {
        case: cases[0].id.clone(),
        resolution: ReviewResolution::Approve,
    };
    let ReviewOutcome::Resolved(approved) = review(
        &host,
        &client,
        approve.clone(),
        support::FIXTURE_OPERATOR_SECRET,
    )
    .unwrap() else {
        panic!("expected a resolved case")
    };
    assert_eq!(
        approved
            .decision
            .as_ref()
            .map(|d| (d.resolution, d.reviewer.as_str())),
        Some((ReviewResolution::Approve, "operator:1"))
    );
    assert_eq!(
        review(&host, &client, approve, support::FIXTURE_OPERATOR_SECRET).unwrap(),
        ReviewOutcome::Resolved(approved.clone())
    );
    let reversal = review(
        &host,
        &client,
        ReviewCommand::Resolve {
            case: approved.id.clone(),
            resolution: ReviewResolution::Deny,
        },
        support::FIXTURE_OPERATOR_SECRET,
    );
    // The in-process client reports refusals as errors; HTTP carries them as `Rejected`.
    assert!(
        matches!(reversal, Err(ReviewError::Contract(Error::Conflict)))
            || matches!(&reversal, Err(ReviewError::Client(e)) if e.code() == Error::Conflict)
    );
    assert_eq!(
        cs_mail_application::accounts::reconcile_account_enrollment(
            &accounts_at(&host, 0),
            "bob",
            &client,
            &[83; 32]
        )
        .unwrap(),
        EnrollmentOutcome::ReviewApproved
    );
    // Bob completes again with a fresh login after renewing the expired attempt.
    let renewed =
        cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&host, 900_000))
            .renew("bob")
            .unwrap();
    let (session, bank) = proofs_for(&renewed, "login-2", bob);
    let client = service(&ids, session);
    let eligible = decision(&client, &request_for(&renewed, bank, bob));
    assert_eq!(commit(&host, 900_000, &eligible), second.account());
    assert_eq!(product_accounts(&cs), 2);
    let sources: i64 = postgres::Client::connect(&cs, postgres::NoTls)
        .unwrap()
        .query_one("SELECT count(*) FROM cs_funding_sources", &[])
        .unwrap()
        .get(0);
    assert_eq!(sources, 2, "each account has its own funding source");
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn concurrent_matching_enrollments_cannot_both_skip_review() {
    // Claim: two concurrent enrollments by one person, through different logins and bank
    // accounts, cannot both become eligible without review. Falsified by two eligible
    // decisions. Matters because concurrency must not bypass the only gate.
    let cs = database_url();
    let ids = database_url();
    let host = engine(&cs, "review-race", SENDER);
    let attempts = [
        (begin(&host, 0, "a", &input(1)), "login-1"),
        (
            begin(&host, 0, "b", &other_input(2, SENDER, 102, 2)),
            "login-2",
        ),
    ]
    .map(|(pending, login)| {
        let (session, bank) = proofs_for(&pending, login, ALICE);
        (service(&ids, session), request_for(&pending, bank, ALICE))
    });
    let barrier = Arc::new(Barrier::new(2));
    let responses = attempts
        .into_iter()
        .map(|(client, request)| {
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                client.call(&request).unwrap()
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    let eligible = responses
        .iter()
        .filter(|r| matches!(r, Response::Eligible(_)))
        .count();
    let held = responses
        .iter()
        .filter(|r| **r == Response::UnderReview)
        .count();
    assert_eq!((eligible, held), (1, 1), "{responses:?}");
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn evidence_is_not_compared_across_products() {
    // Claim: a product's enrollments are screened only against that product's. Falsified
    // when identical evidence under another product is held for review. Matters because
    // each product defines its own evidence policy.
    let cs = database_url();
    let ids = database_url();
    let host = engine(&cs, "products", SENDER);
    let alice = enroll_alice(&host, &ids);
    let mut intent = alice.intent().clone();
    intent.product = "cs-mail/other".into();
    intent.operation = "other-product".into();
    intent.account = "other-account".into();
    let (session, bank) = attest(&intent, alice.input().bank.bank_token, "login-9", ALICE);
    let client = service_for(&ids, session, "cs-mail/other");
    let request = SignedRequest::sign(
        intent.product.clone(),
        Request::Enroll {
            device_proof: intent.prove_possession(&[81; 32]).unwrap(),
            intent,
            oidc_token: "token".into(),
            bank,
            applicant: ALICE.applicant(),
        },
        &[83; 32],
    )
    .unwrap();
    assert!(matches!(
        client.call(&request).unwrap(),
        Response::Eligible(_)
    ));
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn same_login_cannot_reserve_a_second_account() {
    let cs = database_url();
    let ids = database_url();
    let host = engine(&cs, "duplicate", SENDER);
    let pending = cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&host, 0))
        .begin("first", &input(1))
        .unwrap();
    let (session, bank) = proofs(&pending);
    let client = service(&ids, session);
    decision(&client, &request(&pending, bank));
    let mut other_input = input(2);
    other_input.persona = SENDER;
    other_input.actor = ActorRef::Sender(SENDER);
    other_input.key_ref = OperationalKeyRef(102);
    let p2 = cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&host, 0))
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
    let pending = cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&host, 0))
        .begin("keys", &input(1))
        .unwrap();
    let (session, bank) = proofs(&pending);
    let client = service(&ids, session);
    let result = cs_mail_application::accounts::enroll_with_identity(
        &accounts_at(&host, 0),
        "keys",
        EnrollmentEvidence {
            oidc_token: "token".into(),
            bank,
            device_proof: pending.intent().prove_possession(&[81; 32]).unwrap(),
            applicant: ALICE.applicant(),
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
    accounts_at(&host, 0)
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
        accounts_at(&host, 0)
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
    let pending = cs_mail_application::accounts::AccountEnrollment::new(&accounts_at(&host, 0))
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
        &accounts_at(&host, 0),
        "http",
        EnrollmentEvidence {
            oidc_token: "token".into(),
            bank,
            device_proof: pending.intent().prove_possession(&[81; 32]).unwrap(),
            applicant: ALICE.applicant(),
        },
        &client,
        &[83; 32],
    )
    .unwrap();
    assert_eq!(result, EnrollmentOutcome::Enrolled(pending.account()));
    assert_eq!(
        cs_mail_application::accounts::confirm_identity_enrollments(
            &accounts_at(&host, 0),
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
            &accounts_at(&host, 0),
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
    let repo = host.deployment().accounts(clock.clone());
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
    let repo = host.deployment().accounts(clock.clone());
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
