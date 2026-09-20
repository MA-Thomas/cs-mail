use cs_mail_test_support::enrollment;
// Product acceptance tests through signed account actions and durable workers.
use cs_mail_billing::*;
use cs_mail_finance::*;
use cs_mail_primitives::*;
use cs_mail_protocol::{ActorRef, ProtocolState};
use cs_mail_security::KeyRegistry;
use cs_mail_storage_postgres::PostgresEngine;
use cs_mail_worker::{
    run_annual_distribution_batch, run_member_payment_batch, run_utility_payment_batch,
};
use ed25519_dalek::SigningKey;
use postgres::{Client, NoTls, types::Json};
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
fn key(secret: u8) -> [u8; 32] {
    SigningKey::from_bytes(&[secret; 32])
        .verifying_key()
        .to_bytes()
}
fn setup() -> (String, PostgresEngine) {
    let base = std::env::var("CS_MAIL_TEST_DATABASE_URL").unwrap();
    let schema = format!(
        "domain_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    Client::connect(&base, NoTls)
        .unwrap()
        .batch_execute(&format!("CREATE SCHEMA {schema}"))
        .unwrap();
    let url = format!("{base}?options=-csearch_path%3D{schema}");
    let state = ProtocolState::initial_scoped(
        RelationshipRef::from_u128_for_test(1),
        RequestHistoryRef::from_u128_for_test(1),
        ProtocolIdentity(10),
        ProtocolIdentity(20),
        at(2025, 1, 1),
    );
    let engine = PostgresEngine::connect(&url, "domain", &state, SettlementUnit(1)).unwrap();
    engine
        .configure_payment_arrangement(scope(), SettlementUnit(1), key(7), key(77))
        .unwrap();
    engine
        .configure_financial_program(scope(), SettlementUnit(1), key(42), key(7))
        .unwrap();
    engine
        .initialize_key_registry(&KeyRegistry::default(), at(2025, 1, 1))
        .unwrap();
    for (id, persona) in [(1, 10), (2, 20)] {
        enrollment::enroll(
            &engine,
            cs_mail_accounts::EnrollmentInput {
                bank: bank(id, 1),
                persona: ProtocolIdentity(persona),
                actor: ActorRef::Sender(ProtocolIdentity(persona)),
                key_ref: OperationalKeyRef(id),
                initial_key: key(u8::try_from(id).unwrap()),
                maximum_unresolved: 2,
            },
            at(2025, 1, 1),
        )
        .unwrap();
    }
    (url, engine)
}
fn bank(account: u128, version: u64) -> BankVerification {
    BankVerification {
        scope: scope(),
        account: BillingAccountId(account),
        member: MemberId(account),
        person: [u8::try_from(account).unwrap(); 32],
        bank_token: [u8::try_from(account + 8).unwrap(); 32],
        unit: SettlementUnit(1),
        version,
        signature: Vec::new(),
    }
    .sign(&[77; 32])
    .unwrap()
}
fn account_command(
    engine: &PostgresEngine,
    command: BillingCommand,
    id: u128,
    time: CanonicalTime,
) -> Vec<PaymentOperation> {
    let account = engine.billing_account(BillingAccountId(1)).unwrap();
    let signed = SignedBillingCommand::sign(
        account.id(),
        OperationalKeyRef(1),
        scope(),
        account.revision(),
        IdempotencyKey(id),
        command,
        &[1; 32],
    )
    .unwrap();
    cs_mail_application::billing::operations::BillingService::new(engine, &|| time)
        .execute_command(&signed)
        .unwrap()
}
fn program_command(
    engine: &PostgresEngine,
    command: ProgramCommand,
    id: u128,
    time: CanonicalTime,
) {
    let revision = engine
        .financial_program(SettlementUnit(1))
        .unwrap()
        .revision();
    let signed = SignedProgramCommand::sign(
        scope(),
        SettlementUnit(1),
        IdempotencyKey(id),
        revision,
        command,
        &[42; 32],
    )
    .unwrap();
    engine.execute_financial_command(&signed, time).unwrap();
}
fn offer(version: u64, year: u16) -> ServiceOffer {
    ServiceOffer::new(
        PolicyVersion(version),
        ServicePeriod::annual(year, 1, 1, LeapDayRule::February28).unwrap(),
        at(year - 1, 10, 1),
        Money::from_minor_units(12000),
        SettlementUnit(1),
    )
    .unwrap()
}
#[test]
#[ignore = "requires isolated PostgreSQL"]
fn collection_is_advance_fixed_and_rechecks_a_queued_source() {
    let (_url, engine) = setup();
    for (v, y) in [(1, 2026), (2, 2027)] {
        engine.publish_service_offer(&offer(v, y)).unwrap();
    }
    let first = account_command(
        &engine,
        BillingCommand::PurchaseService {
            offer: PolicyVersion(1),
        },
        1,
        at(2025, 9, 1),
    )
    .remove(0);
    let second = account_command(
        &engine,
        BillingCommand::PurchaseService {
            offer: PolicyVersion(2),
        },
        2,
        at(2025, 9, 1),
    )
    .remove(0);
    let mut processor = SimulatedProcessor::new([7; 32]);
    let lease = Duration(30_000);
    assert_eq!(
        run_utility_payment_batch(&engine, &mut processor, &|| at(2025, 9, 30), lease, 10)
            .unwrap()
            .claimed,
        0
    );
    processor.pend_next_submission();
    assert_eq!(
        run_utility_payment_batch(&engine, &mut processor, &|| at(2025, 10, 1), lease, 10)
            .unwrap()
            .retried,
        1
    );
    processor
        .resolve(first.id, FinancialEventId(900), PaymentOutcome::Failed)
        .unwrap();
    run_utility_payment_batch(&engine, &mut processor, &|| at(2025, 10, 2), lease, 10).unwrap();
    let report =
        run_utility_payment_batch(&engine, &mut processor, &|| at(2026, 10, 1), lease, 10).unwrap();
    assert_eq!(report.retried, 1);
    assert_eq!(processor.operation_count(), 1);
    assert!(processor.lookup(second.id).unwrap().is_none());
    cs_mail_application::billing::operations::BillingService::new(&engine, &|| at(2025, 1, 1))
        .reverify_funding(&bank(1, 2))
        .unwrap();
    let retry = account_command(
        &engine,
        BillingCommand::RetryCollection {
            contract: ServiceContractId(first.id.0),
        },
        3,
        at(2026, 11, 1),
    )
    .remove(0);
    assert_ne!(retry.id, first.id);
    run_utility_payment_batch(&engine, &mut processor, &|| at(2026, 11, 1), lease, 10).unwrap();
    let account = engine.billing_account(BillingAccountId(1)).unwrap();
    assert_eq!(
        account.contracts()[&ServiceContractId(first.id.0)]
            .offer()
            .period(),
        offer(1, 2026).period()
    );
    assert!(!account.covers(at(2026, 1, 1)));
    assert!(account.covers(at(2026, 11, 1)));
    assert!(!account.contracts()[&ServiceContractId(first.id.0)].covers(at(2027, 1, 1)));
}
#[test]
#[ignore = "requires isolated PostgreSQL"]
fn bank_identity_and_account_authority_cannot_be_substituted() {
    let (_url, engine) = setup();
    // A distinct enrollment cannot claim an existing persona, even with valid bank evidence.
    assert!(
        enrollment::enroll(
            &engine,
            cs_mail_accounts::EnrollmentInput {
                bank: bank(3, 1),
                persona: ProtocolIdentity(10),
                actor: ActorRef::Sender(ProtocolIdentity(10)),
                key_ref: OperationalKeyRef(3),
                initial_key: key(3),
                maximum_unresolved: 2,
            },
            at(2025, 1, 1)
        )
        .is_err()
    );
    assert!(
        engine
            .configure_payment_arrangement(scope(), SettlementUnit(1), key(8), key(77))
            .is_err()
    );
    let repository = engine.accounts(|| at(2025, 2, 1));
    let product = repository.persona_account(ProtocolIdentity(10)).unwrap();
    let signed = cs_mail_accounts::control::SignedAccountCommand {
        account: product,
        operational_key: OperationalKeyRef(2),
        expected_revision: 0,
        idempotency_key: IdempotencyKey(90),
        product: "cs-mail/test".into(),
        command: cs_mail_accounts::control::AccountCommand::SetStatus(
            cs_mail_accounts::control::AccountStatus::Closed,
        ),
        signature: Vec::new(),
    }
    .sign(&[2; 32])
    .unwrap();
    assert!(
        cs_mail_application::accounts::operations::AccountService::new(&repository, &|| at(
            2025, 2, 1
        ))
        .execute_command(&signed)
        .is_err()
    );
    assert_eq!(
        repository.account_control(product).unwrap().status(),
        cs_mail_accounts::control::AccountStatus::Active
    );
    let ghost = SignedProgramCommand::sign(
        scope(),
        SettlementUnit(1),
        IdempotencyKey(999),
        0,
        ProgramCommand::Enroll {
            member: MemberId(99),
            identity_digest: [99; 32],
            status: MembershipStatus {
                opted_in: true,
                verified: true,
                suspended: false,
            },
        },
        &[42; 32],
    )
    .unwrap();
    assert!(
        engine
            .execute_financial_command(&ghost, at(2025, 2, 1))
            .is_err()
    );
}
/// Fixture setup only: the pool contains a matured, unassessed $170 forfeiture.
/// Allocation, scheduling, payment and reconciliation below use production entry points.
fn seed_pool(url: &str, engine: &PostgresEngine) {
    let mut program = engine.financial_program(SettlementUnit(1)).unwrap();
    program
        .record_forfeiture(
            Forfeiture {
                id: PaymentOperationId(500),
                amount: Money::from_minor_units(17000),
                unit: SettlementUnit(1),
                forfeited_at: at(2025, 1, 1),
                requires_review: false,
                terms: FinancialTerms {
                    scope: scope(),
                    policy_version: PolicyVersion(1),
                    corporate_basis_points: 0,
                    maturity_delay: Duration(0),
                },
            },
            at(2025, 1, 1),
        )
        .unwrap();
    program
        .clear_maturity(
            PaymentOperationId(500),
            FinancialEventId(501),
            at(2025, 1, 1),
        )
        .unwrap();
    let mut client = Client::connect(url, NoTls).unwrap();
    let mut tx = client.transaction().unwrap();
    for (id, record) in &program.records().funding {
        tx.execute(
            "INSERT INTO cs_program_lots(settlement_unit,id,record) VALUES(1,$1,$2)",
            &[&id.0.to_string(), &Json(record)],
        )
        .unwrap();
    }
    for (account, balance) in program.ledger().balances() {
        tx.execute("INSERT INTO cs_program_accounts(settlement_unit,account_key,account,balance) VALUES(1,$1,$2,$3::text::numeric)",&[&serde_json::to_string(account).unwrap(),&Json(account),&balance.to_string()]).unwrap();
    }
    tx.execute(
        "UPDATE cs_financial_programs SET metadata=$1,ledger_revision=$2 WHERE settlement_unit=1",
        &[
            &Json(program.metadata()),
            &i64::try_from(program.ledger().revision).unwrap(),
        ],
    )
    .unwrap();
    tx.commit().unwrap();
}
#[test]
#[ignore = "requires isolated PostgreSQL"]
#[allow(clippy::too_many_lines)] // Keep the full annual allocation, closure and recovery journey together.
fn annual_allocation_automatically_pays_one_lump_sum_after_closure() {
    let (url, engine) = setup();
    let start = at(2025, 1, 1);
    let due = at(2026, 3, 1);
    let schedule = AnnualDistributionSchedule::utc(
        2025,
        EligibilityPolicy {
            version: PolicyVersion(1),
            minimum_tenure: Duration(0),
            minimum_active_days: 1,
        },
        DistributionTerms::new(PolicyVersion(1), Money::from_minor_units(12000), due).unwrap(),
    )
    .unwrap();
    program_command(
        &engine,
        ProgramCommand::PublishAnnualDistribution(schedule),
        1,
        start,
    );
    program_command(
        &engine,
        ProgramCommand::Enroll {
            member: MemberId(1),
            identity_digest: {
                let mut db = Client::connect(&url, NoTls).unwrap();
                db.query_one(
                    "SELECT membership_identity FROM cs_product_accounts WHERE billing='1'",
                    &[],
                )
                .unwrap()
                .get::<_, Vec<u8>>(0)
                .try_into()
                .unwrap()
            },
            status: MembershipStatus {
                opted_in: true,
                verified: true,
                suspended: false,
            },
        },
        2,
        start,
    );
    program_command(
        &engine,
        ProgramCommand::RecordActivity {
            member: MemberId(1),
            activity: IntentionalActivity::Read,
        },
        3,
        start,
    );
    seed_pool(&url, &engine);
    engine.publish_service_offer(&offer(1, 2026)).unwrap();
    account_command(
        &engine,
        BillingCommand::PurchaseService {
            offer: PolicyVersion(1),
        },
        1,
        at(2025, 9, 1),
    );
    let mut processor = SimulatedProcessor::new([7; 32]);
    let lease = Duration(30_000);
    run_utility_payment_batch(&engine, &mut processor, &|| at(2025, 10, 1), lease, 10).unwrap();
    assert!(
        !engine
            .billing_account(BillingAccountId(1))
            .unwrap()
            .covers(at(2025, 10, 1))
    );
    run_annual_distribution_batch(&engine, SettlementUnit(1), &|| at(2026, 1, 1), lease, 10)
        .unwrap();
    assert!(
        engine
            .financial_program(SettlementUnit(1))
            .unwrap()
            .payables()
            .next()
            .unwrap()
            .payment()
            .is_none()
    );
    let repository = engine.accounts(|| at(2026, 2, 1));
    let product = repository.persona_account(ProtocolIdentity(10)).unwrap();
    let product_name = repository
        .product_account(product)
        .unwrap()
        .identity
        .product;
    cs_mail_application::accounts::operations::AccountService::new(&repository, &|| at(2026, 2, 1))
        .execute_command(
            &cs_mail_accounts::control::SignedAccountCommand {
                account: product,
                operational_key: OperationalKeyRef(1),
                expected_revision: 0,
                idempotency_key: IdempotencyKey(2),
                product: product_name,
                command: cs_mail_accounts::control::AccountCommand::SetStatus(
                    cs_mail_accounts::control::AccountStatus::Closed,
                ),
                signature: Vec::new(),
            }
            .sign(&[1; 32])
            .unwrap(),
        )
        .unwrap();
    run_annual_distribution_batch(&engine, SettlementUnit(1), &|| due, lease, 10).unwrap();
    let program = engine.financial_program(SettlementUnit(1)).unwrap();
    let payable = program.payables().next().unwrap();
    let op = payable.pending().next().unwrap();
    assert_eq!(op.amount, Money::from_minor_units(17000));
    assert_eq!(op.destination, [9; 32]);
    assert_eq!(payable.rebate(), Money::from_minor_units(12000));
    assert_eq!(payable.excess(), Money::from_minor_units(5000));
    processor.lose_next_response();
    assert_eq!(
        run_member_payment_batch(&engine, &mut processor, SettlementUnit(1), due, lease, 10)
            .unwrap()
            .retried,
        1
    );
    drop(engine);
    let state = ProtocolState::initial_scoped(
        RelationshipRef::from_u128_for_test(1),
        RequestHistoryRef::from_u128_for_test(1),
        ProtocolIdentity(10),
        ProtocolIdentity(20),
        at(2025, 1, 1),
    );
    let engine = PostgresEngine::connect(&url, "domain", &state, SettlementUnit(1)).unwrap();
    assert_eq!(
        run_member_payment_batch(
            &engine,
            &mut processor,
            SettlementUnit(1),
            at(2026, 3, 2),
            lease,
            10
        )
        .unwrap()
        .completed,
        1
    );
    assert_eq!(processor.operation_count(), 2);
    let statement = engine
        .financial_program(SettlementUnit(1))
        .unwrap()
        .member_statement(MemberId(1));
    assert_eq!(statement[0].outstanding, Money::ZERO);
    assert_eq!(statement[0].status, AllocationStatus::Confirmed);
}
