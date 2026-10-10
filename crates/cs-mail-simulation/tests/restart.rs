//! Milestone 1 claim 3, challenged against `PostgreSQL`: a restart cannot duplicate a
//! simulated payment operation.
use cs_mail_billing::{
    BillingCommand, LeapDayRule, ServiceOffer, ServicePeriod, SignedBillingCommand,
};
use cs_mail_finance::simulation::SimulationDirective;
use cs_mail_finance::{BankVerification, FinancialScope};
use cs_mail_operations::{OperatorKeyRef, SignedOperatorCommand};
use cs_mail_primitives::{
    BillingAccountId, CanonicalTime, Duration, IdempotencyKey, MemberId, Money, OperationalKeyRef,
    PolicyVersion, ProgramRef, ProtocolIdentity, ProtocolVersion, ProviderRef, RelationshipRef,
    RequestHistoryRef, SettlementUnit, calendar_date,
};
use cs_mail_protocol::{ActorRef, ProtocolState};
use cs_mail_simulation::{PostgresSimulator, SimulationCommand, SimulationOutcome};
use cs_mail_storage_postgres::{PostgresDeployment, PostgresEngine};
use cs_mail_worker::run_utility_payment_batch;
use ed25519_dalek::SigningKey;

const UNIT: SettlementUnit = SettlementUnit(1);
/// The fixture deployment trusts the processor key derived from this secret.
const PROCESSOR_SECRET: [u8; 32] = [7; 32];

fn scope() -> FinancialScope {
    FinancialScope::new(
        [7; 32],
        ProviderRef(30),
        ProgramRef(1),
        [9; 32],
        ProtocolVersion(2),
    )
}
fn at(year: u16, month: u8, day: u8) -> CanonicalTime {
    calendar_date(year, month, day).unwrap()
}
fn database_url() -> String {
    let base =
        std::env::var("CS_MAIL_TEST_DATABASE_URL").expect("requires isolated test PostgreSQL");
    let schema = format!(
        "cs_sim_{}_{}",
        std::process::id(),
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
fn open(url: &str) -> PostgresEngine {
    let state = ProtocolState::initial_scoped(
        RelationshipRef::from_u128_for_test(1),
        RequestHistoryRef::from_u128_for_test(1),
        ProtocolIdentity(10),
        ProtocolIdentity(20),
        at(2025, 1, 1),
    );
    PostgresDeployment::connect(url)
        .unwrap()
        .relationship("simulation", &state, UNIT)
        .unwrap()
}
/// Enrolls one member and buys the 2026 service year, leaving its collection as queued
/// utility-payment work.
fn purchase(engine: &PostgresEngine) {
    cs_mail_test_support::bootstrap(engine.deployment(), scope(), UNIT).unwrap();
    cs_mail_test_support::enrollment::enroll(
        engine.deployment(),
        cs_mail_accounts::EnrollmentInput {
            bank: BankVerification {
                scope: scope(),
                account: BillingAccountId(1),
                member: MemberId(1),
                bank_token: [9; 32],
                unit: UNIT,
                version: 1,
                signature: Vec::new(),
            }
            .sign(&[77; 32])
            .unwrap(),
            persona: ProtocolIdentity(10),
            actor: ActorRef::Sender(ProtocolIdentity(10)),
            key_ref: OperationalKeyRef(1),
            initial_key: SigningKey::from_bytes(&[1; 32]).verifying_key().to_bytes(),
            maximum_unresolved: 2,
        },
        at(2025, 1, 1),
    )
    .unwrap();
    let offer = ServiceOffer::new(
        PolicyVersion(1),
        ServicePeriod::annual(2026, 1, 1, LeapDayRule::February28).unwrap(),
        at(2025, 10, 1),
        Money::from_minor_units(12000),
        UNIT,
    )
    .unwrap();
    cs_mail_test_support::operator(
        engine.deployment(),
        cs_mail_operations::OperatorCommand::PublishServiceOffer(offer),
        at(2025, 1, 1),
    )
    .unwrap();
    let account = engine
        .deployment()
        .billing_account(BillingAccountId(1))
        .unwrap();
    let signed = SignedBillingCommand::sign(
        account.id(),
        OperationalKeyRef(1),
        scope(),
        account.revision(),
        IdempotencyKey(1),
        BillingCommand::PurchaseService {
            offer: PolicyVersion(1),
        },
        &[1; 32],
    )
    .unwrap();
    cs_mail_application::billing::operations::BillingService::new(engine.deployment(), &|| {
        at(2025, 9, 1)
    })
    .execute_command(&signed)
    .unwrap();
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn a_restart_cannot_duplicate_a_simulated_payment() {
    // Falsified if, after the provider records an operation whose response is lost and the
    // service and simulator restart, the simulator holds two records for the operation or
    // the collection is charged twice.
    let url = database_url();
    let engine = open(&url);
    purchase(&engine);
    let authority = engine
        .deployment()
        .deployment_record()
        .unwrap()
        .unwrap()
        .operator_authority();
    let mut simulator = PostgresSimulator::connect(&url, PROCESSOR_SECRET).unwrap();
    let lose = SignedOperatorCommand::sign(
        [7; 32],
        OperatorKeyRef(1),
        IdempotencyKey(1),
        SimulationCommand::QueueDirectives(vec![SimulationDirective {
            lose_response: true,
            ..SimulationDirective::default()
        }]),
        &cs_mail_test_support::FIXTURE_OPERATOR_SECRET,
    )
    .unwrap();
    assert_eq!(
        simulator
            .execute(&authority, &lose, || at(2025, 9, 30))
            .unwrap(),
        SimulationOutcome::Queued { count: 1 }
    );
    // The provider records the charge, then its response is lost.
    let lease = Duration(30_000);
    let first = run_utility_payment_batch(
        engine.deployment(),
        &mut simulator,
        &|| at(2025, 10, 1),
        lease,
        10,
    )
    .unwrap();
    assert_eq!((first.claimed, first.retried), (1, 1));

    // Kill both processes and start again from the durable state.
    drop(simulator);
    drop(engine);
    let engine = open(&url);
    let mut simulator = PostgresSimulator::connect(&url, PROCESSOR_SECRET).unwrap();
    let second = run_utility_payment_batch(
        engine.deployment(),
        &mut simulator,
        &|| at(2025, 10, 2),
        lease,
        10,
    )
    .unwrap();
    assert_eq!((second.claimed, second.completed), (1, 1));
    let third = run_utility_payment_batch(
        engine.deployment(),
        &mut simulator,
        &|| at(2025, 10, 3),
        lease,
        10,
    )
    .unwrap();
    assert_eq!(third.claimed, 0);

    let mut db = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    let mut count = |sql: &str| db.query_one(sql, &[]).unwrap().get::<_, i64>(0);
    assert_eq!(count("SELECT count(*) FROM cs_sim_operations"), 1);
    assert_eq!(
        count("SELECT count(*) FROM cs_sim_directives WHERE consumed_by IS NOT NULL"),
        1
    );
    let account = engine
        .deployment()
        .billing_account(BillingAccountId(1))
        .unwrap();
    assert!(account.covers(at(2026, 1, 1)));
    assert_eq!(account.contracts().len(), 1);
}
