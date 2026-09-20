use cs_mail_accounts::EnrollmentInput;
use cs_mail_application::InMemoryStore;
use cs_mail_application::accounts::*;
use cs_mail_finance::{BankVerification, FinancialScope};
use cs_mail_primitives::*;
use cs_mail_protocol::ActorRef;
use ed25519_dalek::SigningKey;
use identity_contract::*;

fn scope() -> FinancialScope {
    FinancialScope::new(
        [7; 32],
        ProviderRef(30),
        ProgramRef(1),
        [9; 32],
        ProtocolVersion(2),
    )
}
fn input(id: u8) -> EnrollmentInput {
    EnrollmentInput {
        bank: BankVerification {
            scope: scope(),
            account: BillingAccountId(u128::from(id)),
            member: MemberId(u128::from(id)),
            person: [id; 32],
            bank_token: [id; 32],
            unit: SettlementUnit(1),
            version: 1,
            signature: vec![],
        }
        .sign(&[77; 32])
        .unwrap(),
        persona: ProtocolIdentity(u128::from(id)),
        actor: ActorRef::Sender(ProtocolIdentity(u128::from(id))),
        key_ref: OperationalKeyRef(u128::from(id)),
        initial_key: SigningKey::from_bytes(&[id; 32]).verifying_key().to_bytes(),
        maximum_unresolved: 2,
    }
}
fn proof(p: &PendingEnrollment) -> SignedDecision {
    SignedDecision::sign(
        DecisionClaims {
            issuer: "https://identity.example.test".into(),
            intent: p.intent().clone(),
            subject_ref: "same-subject".to_string().try_into().unwrap(),
            binding_version: 1,
            security_version: 1,
            policy: CS_MAIL_POLICY.into(),
            evidence_ref: "bank-proof".into(),
            issued_at: p.intent().created_at,
            expires_at: p.intent().expires_at,
        },
        &[80; 32],
    )
    .unwrap()
}
fn repository(clock: cs_mail_test_support::enrollment::TestClock) -> MemoryAccountRepository {
    MemoryAccountRepository::new(
        InMemoryStore::new(scope()),
        DecisionVerifier::new(
            "https://identity.example.test".into(),
            "cs-mail/test".into(),
            SigningKey::from_bytes(&[80; 32]).verifying_key().to_bytes(),
        )
        .unwrap(),
        SigningKey::from_bytes(&[77; 32]).verifying_key().to_bytes(),
        clock,
    )
    .unwrap()
}
#[test]
fn memory_enrollment_checks_current_authority_and_preserves_ownership_on_renewal() {
    let clock = cs_mail_test_support::enrollment::TestClock::default();
    cs_mail_test_support::enrollment::assert_repository_contract(
        &repository(clock.clone()),
        &clock,
        input(1),
        input(2),
        CanonicalTime(100_000),
    );
}
#[test]
fn rejected_confirmation_does_not_starve_a_healthy_account() {
    struct Client;
    impl IdentityClient for Client {
        fn call(&self, r: &SignedRequest) -> Result<Response, ClientError> {
            let Request::Confirm { decision } = &r.request else {
                panic!("unexpected request")
            };
            Ok(if decision.claims.intent.operation == "a" {
                Response::Rejected(Error::Conflict)
            } else {
                Response::Confirmed
            })
        }
    }
    let clock = cs_mail_test_support::enrollment::TestClock::default();
    clock.set(CanonicalTime(100_000));
    let repo = repository(clock);
    for (operation, id) in [("a", 1), ("b", 2)] {
        let pending = cs_mail_application::accounts::AccountEnrollment::new(&repo)
            .begin(operation, &input(id))
            .unwrap();
        let mut decision = proof(&pending);
        decision.claims.subject_ref = operation.to_string().try_into().unwrap();
        decision = SignedDecision::sign(decision.claims, &[80; 32]).unwrap();
        cs_mail_application::accounts::AccountEnrollment::new(&repo)
            .commit(&decision)
            .unwrap();
    }
    let report =
        confirm_identity_enrollments(&repo, &Client, &[83; 32], 10, CanonicalTime(100_000))
            .unwrap();
    assert_eq!((report.confirmed, report.intervention), (1, 1));
    assert!(
        repo.claim_confirmations(10, CanonicalTime(200_000))
            .unwrap()
            .is_empty()
    );
}
