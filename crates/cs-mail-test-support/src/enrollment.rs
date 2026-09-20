//! Test identity issuer. Only compiled by integration tests; account creation still verifies
//! the signed decision and commits through the production enrollment transaction.
use cs_mail_accounts::EnrollmentInput;
use cs_mail_primitives::{AccountId, CanonicalTime};
use cs_mail_storage_postgres::{PostgresEngine, StorageError};
use ed25519_dalek::SigningKey;
use identity_contract::*;

pub fn enroll(
    engine: &PostgresEngine,
    input: EnrollmentInput,
    at: CanonicalTime,
) -> Result<AccountId, StorageError> {
    engine.accounts(move || at).configure_identity_service(
        "https://identity.example.test",
        "cs-mail/test",
        SigningKey::from_bytes(&[80; 32]).verifying_key().to_bytes(),
    )?;
    let operation = format!("fixture-account-{}", input.bank.account.0);
    let p = cs_mail_application::accounts::AccountEnrollment::new(&engine.accounts(move || at))
        .begin(&operation, &input)?;
    let decision = SignedDecision::sign(
        DecisionClaims {
            issuer: "https://identity.example.test".into(),
            intent: p.intent().clone(),
            subject_ref: format!("fixture-subject-{}", p.input().bank.account.0).try_into()?,
            binding_version: 1,
            security_version: 1,
            policy: CS_MAIL_POLICY.into(),
            evidence_ref: format!("fixture-bank-{}", p.input().bank.account.0),
            issued_at: p.intent().created_at,
            expires_at: p.intent().expires_at,
        },
        &[80; 32],
    )?;
    cs_mail_application::accounts::AccountEnrollment::new(&engine.accounts(move || at))
        .commit(&decision)
}

/// Shared behavioral contract for every enrollment repository adapter.
pub fn assert_repository_contract<R: cs_mail_application::accounts::EnrollmentRepository>(
    repository: &R,
    clock: &TestClock,
    first_input: EnrollmentInput,
    second_input: EnrollmentInput,
    at: CanonicalTime,
) where
    R::Error: std::fmt::Debug,
{
    clock.set(at);
    let first = {
        clock.set(at);
        cs_mail_application::accounts::AccountEnrollment::new(repository)
            .begin("conformance-one", &first_input.clone())
    }
    .unwrap();
    let decision = |pending: &cs_mail_application::accounts::PendingEnrollment| {
        SignedDecision::sign(
            DecisionClaims {
                issuer: "https://identity.example.test".into(),
                intent: pending.intent().clone(),
                subject_ref: "conformance-subject".to_string().try_into().unwrap(),
                binding_version: 1,
                security_version: 1,
                policy: CS_MAIL_POLICY.into(),
                evidence_ref: "conformance-bank-proof".into(),
                issued_at: pending.intent().created_at,
                expires_at: pending.intent().expires_at,
            },
            &[80; 32],
        )
        .unwrap()
    };
    let old = decision(&first);
    let expired = CanonicalTime(at.0 + 900_000);
    assert!(
        {
            clock.set(expired);
            cs_mail_application::accounts::AccountEnrollment::new(repository).commit(&old)
        }
        .is_err()
    );
    assert!(
        {
            clock.set(at);
            cs_mail_application::accounts::AccountEnrollment::new(repository)
                .begin("conflicting-owner", &first_input)
        }
        .is_err()
    );
    assert_eq!(
        {
            clock.set(at);
            cs_mail_application::accounts::AccountEnrollment::new(repository)
                .begin("conformance-one", &first.input().clone())
        }
        .unwrap(),
        first
    );
    let renewed = {
        clock.set(expired);
        cs_mail_application::accounts::AccountEnrollment::new(repository).renew("conformance-one")
    }
    .unwrap();
    assert_eq!(
        (renewed.account(), renewed.principal()),
        (first.account(), first.principal())
    );
    assert_ne!(renewed.intent().challenge, first.intent().challenge);
    assert!(
        {
            clock.set(expired);
            cs_mail_application::accounts::AccountEnrollment::new(repository).commit(&old)
        }
        .is_err()
    );
    let current = decision(&renewed);
    let mut foreign = current.claims.clone();
    foreign.issuer = "https://wrong-issuer.test".into();
    assert!(
        {
            clock.set(expired);
            cs_mail_application::accounts::AccountEnrollment::new(repository)
                .commit(&SignedDecision::sign(foreign, &[80; 32]).unwrap())
        }
        .is_err()
    );
    assert_eq!(
        {
            clock.set(expired);
            cs_mail_application::accounts::AccountEnrollment::new(repository).commit(&current)
        }
        .unwrap(),
        first.account()
    );
    assert_eq!(
        {
            clock.set(CanonicalTime(expired.0 + 900_000));
            cs_mail_application::accounts::AccountEnrollment::new(repository).commit(&current)
        }
        .unwrap(),
        first.account()
    );
    assert!(
        {
            clock.set(CanonicalTime(expired.0 + 900_000));
            cs_mail_application::accounts::AccountEnrollment::new(repository)
                .renew("conformance-one")
        }
        .is_err()
    );
    let second = {
        clock.set(at);
        cs_mail_application::accounts::AccountEnrollment::new(repository)
            .begin("conformance-two", &second_input)
    }
    .unwrap();
    assert!(
        {
            clock.set(at);
            cs_mail_application::accounts::AccountEnrollment::new(repository)
                .commit(&decision(&second))
        }
        .is_err()
    );
}

/// Controllable host clock for existing adapter conformance scenarios.
#[derive(Clone, Default)]
pub struct TestClock(std::sync::Arc<std::sync::atomic::AtomicU64>);
impl TestClock {
    pub fn set(&self, at: CanonicalTime) {
        self.0.store(at.0, std::sync::atomic::Ordering::SeqCst);
    }
}
impl cs_mail_application::accounts::AccountClock for TestClock {
    fn now(&self) -> CanonicalTime {
        CanonicalTime(self.0.load(std::sync::atomic::Ordering::SeqCst))
    }
}
