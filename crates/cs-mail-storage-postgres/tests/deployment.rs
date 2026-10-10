//! Milestone 1 claim 1, challenged against `PostgreSQL`: repeating or interrupting bootstrap
//! cannot install a second provider signing key or replace the deployment's keys.
use cs_mail_application::operations::{OperatorService, bootstrap};
use cs_mail_finance::FinancialScope;
use cs_mail_operations::{
    BootstrapOutcome, DeploymentBootstrap, OperationsError, OperatorCommand, OperatorKeyRef,
    OperatorKeys, SignedOperatorCommand,
};
use cs_mail_primitives::{
    CanonicalTime, IdempotencyKey, Money, PolicyVersion, ProgramRef, ProtocolVersion, ProviderRef,
    SettlementUnit,
};
use cs_mail_storage_postgres::{PostgresDeployment, StorageError};
use ed25519_dalek::SigningKey;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};

static NEXT: AtomicU64 = AtomicU64::new(1);
const UNIT: SettlementUnit = SettlementUnit(1);

fn database_url() -> String {
    let base =
        std::env::var("CS_MAIL_TEST_DATABASE_URL").expect("requires isolated test PostgreSQL");
    let schema = format!(
        "cs_deploy_{}_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    postgres::Client::connect(&base, postgres::NoTls)
        .unwrap()
        .batch_execute(&format!("CREATE SCHEMA {schema}"))
        .unwrap();
    format!("{base}?options=-csearch_path%3D{schema}")
}
fn scope() -> FinancialScope {
    FinancialScope::new(
        [7; 32],
        ProviderRef(30),
        ProgramRef(1),
        [9; 32],
        ProtocolVersion(2),
    )
}
fn public(secret: u8) -> [u8; 32] {
    SigningKey::from_bytes(&[secret; 32])
        .verifying_key()
        .to_bytes()
}
/// The fixture deployment, or a rival with its own provider and operator keys.
fn deployment(rival: bool) -> DeploymentBootstrap {
    let mut record = cs_mail_test_support::fixture_deployment(scope(), UNIT)
        .unwrap()
        .record()
        .clone();
    if rival {
        record.provider_key.verifying_key = public(33);
        record.operators = OperatorKeys::new([(OperatorKeyRef(2), public(91))].into()).unwrap();
    }
    DeploymentBootstrap::new(record).unwrap()
}
fn pricing_command(operator: u128, secret: u8) -> SignedOperatorCommand<OperatorCommand> {
    SignedOperatorCommand::sign(
        [7; 32],
        OperatorKeyRef(operator),
        IdempotencyKey(u128::from(secret)),
        OperatorCommand::PublishRequestPricing(
            cs_mail_protocol::pricing::RequestPricingPolicy::new(
                PolicyVersion(1),
                UNIT,
                Money::from_minor_units(2),
                Money::from_minor_units(1),
                Money::from_minor_units(1000),
            )
            .unwrap(),
        ),
        &[secret; 32],
    )
    .unwrap()
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn racing_and_repeated_bootstraps_leave_one_deployment() {
    // Falsified if both racing bootstraps succeed, if the stored provider key or operators
    // change afterwards, or if the losing bootstrap's operator key is accepted.
    let url = database_url();
    let barrier = Arc::new(Barrier::new(2));
    let results = [false, true]
        .map(|rival| {
            let url = url.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let store = PostgresDeployment::connect(&url).unwrap();
                barrier.wait();
                bootstrap(&store, &deployment(rival), || CanonicalTime(1)).map(|o| (rival, o))
            })
        })
        .map(|thread| thread.join().unwrap());
    let winners: Vec<_> = results.iter().filter_map(|r| r.as_ref().ok()).collect();
    assert_eq!(
        winners.len(),
        1,
        "exactly one bootstrap creates the deployment"
    );
    let (winner, outcome) = *winners[0];
    assert_eq!(outcome, BootstrapOutcome::Created);
    assert!(results.iter().any(|r| matches!(
        r,
        Err(StorageError::Operations(OperationsError::BootstrapConflict))
    )));

    let store = PostgresDeployment::connect(&url).unwrap();
    assert_eq!(store.deployment_record().unwrap(), Some(deployment(winner)));
    // Repeating the winner changes nothing; repeating the loser is refused.
    assert_eq!(
        bootstrap(&store, &deployment(winner), || CanonicalTime(2)).unwrap(),
        BootstrapOutcome::AlreadyBootstrapped
    );
    assert!(bootstrap(&store, &deployment(!winner), || CanonicalTime(2)).is_err());
    assert_eq!(store.deployment_record().unwrap(), Some(deployment(winner)));

    // Only the winner's operator key is accepted.
    let (accepted, refused) = if winner {
        (pricing_command(2, 91), pricing_command(1, 90))
    } else {
        (pricing_command(1, 90), pricing_command(2, 91))
    };
    let operators = OperatorService::new(&store);
    assert!(matches!(
        operators.execute(&refused, || CanonicalTime(3)),
        Err(StorageError::Operations(OperationsError::Unauthorized))
    ));
    operators.execute(&accepted, || CanonicalTime(3)).unwrap();
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn a_bootstrap_failing_after_its_first_writes_leaves_nothing() {
    // Falsified if a bootstrap that fails part-way leaves a deployment record or payment
    // trust behind, which a later bootstrap would then have to reconcile with.
    let url = database_url();
    let store = PostgresDeployment::connect(&url).unwrap();
    let mut db = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    // Identity trust is written last; a conflicting row makes the bootstrap fail there.
    db.execute(
        "INSERT INTO cs_identity_configuration(singleton,issuer,product,decision_keys) \
         VALUES(TRUE,'https://other.example.test','cs-mail/other','[]')",
        &[],
    )
    .unwrap();
    assert!(bootstrap(&store, &deployment(false), || CanonicalTime(1)).is_err());
    for table in ["cs_deployment", "cs_payment_arrangement"] {
        assert_eq!(
            db.query_one(&format!("SELECT count(*) FROM {table}"), &[])
                .unwrap()
                .get::<_, i64>(0),
            0,
            "{table} must be rolled back"
        );
    }
    assert_eq!(store.deployment_record().unwrap(), None);
}
