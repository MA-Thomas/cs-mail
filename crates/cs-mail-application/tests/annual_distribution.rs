//! Product examples: the allocation is one bank payment, independent of renewal.
use cs_mail_application::billing::prepare_distribution;
use cs_mail_billing::*;
use cs_mail_finance::*;
use cs_mail_primitives::*;
use ed25519_dalek::SigningKey;
fn scope() -> FinancialScope {
    FinancialScope::new(
        [1; 32],
        ProviderRef(2),
        ProgramRef(3),
        [4; 32],
        ProtocolVersion(2),
    )
}
fn bank(id: u128) -> VerifiedBankAccount {
    BankVerification {
        scope: scope(),
        account: BillingAccountId(id),
        member: MemberId(id),
        person: [u8::try_from(id).unwrap(); 32],
        bank_token: [9; 32],
        unit: SettlementUnit(1),
        version: 1,
        signature: Vec::new(),
    }
    .sign(&[7; 32])
    .unwrap()
    .verify(&SigningKey::from_bytes(&[7; 32]).verifying_key().to_bytes())
    .unwrap()
}
fn allocated(amount: u64) -> FinancialProgram {
    let mut p = FinancialProgram::new(scope(), SettlementUnit(1));
    let cutoff = calendar_date(2026, 1, 1).unwrap();
    let start = calendar_date(2025, 1, 1).unwrap();
    let schedule = AnnualDistributionSchedule::utc(
        2025,
        EligibilityPolicy {
            version: PolicyVersion(1),
            minimum_tenure: Duration(0),
            minimum_active_days: 1,
        },
        DistributionTerms::new(PolicyVersion(1), Money::from_minor_units(12000), cutoff).unwrap(),
    )
    .unwrap();
    p.publish_annual_distribution(schedule, start).unwrap();
    p.enroll(
        MemberId(1),
        [1; 32],
        MembershipStatus {
            opted_in: true,
            verified: true,
            suspended: false,
        },
        start,
    )
    .unwrap();
    p.record_activity(MemberId(1), IntentionalActivity::Read, start)
        .unwrap();
    if amount != 0 {
        p.record_forfeiture(
            Forfeiture {
                id: PaymentOperationId(8),
                amount: Money::from_minor_units(amount),
                unit: SettlementUnit(1),
                forfeited_at: start,
                requires_review: false,
                terms: FinancialTerms {
                    scope: scope(),
                    policy_version: PolicyVersion(1),
                    corporate_basis_points: 0,
                    maturity_delay: Duration(0),
                },
            },
            start,
        )
        .unwrap();
        p.clear_maturity(PaymentOperationId(8), FinancialEventId(9), start)
            .unwrap();
    }
    p.finalize_annual_distribution(AnnualDistributionId(2025), cutoff)
        .unwrap();
    p
}
#[test]
fn below_equal_and_above_utility_charge_each_produce_one_payment() {
    for (total, rebate, excess) in [
        (8000, 8000, 0),
        (12000, 12000, 0),
        (17000, 12000, 5000),
        (u64::MAX, 12000, u64::MAX - 12000),
    ] {
        let mut program = allocated(total);
        let account = BillingAccount::new(bank(1));
        let id = program.payables().next().unwrap().id();
        let at = calendar_date(2026, 1, 1).unwrap();
        let before = program.clone();
        assert!(prepare_distribution(&account, &mut program, id, CanonicalTime(at.0 - 1)).is_err());
        assert_eq!(program, before);
        let op = prepare_distribution(&account, &mut program, id, at)
            .unwrap()
            .unwrap();
        assert_eq!(op.amount, Money::from_minor_units(total));
        assert!(matches!(op.kind, PaymentKind::MemberPayout { .. }));
        assert_eq!(op.destination, account.bank().evidence().bank_token);
        let statement = &program.member_statement(MemberId(1))[0];
        assert_eq!(statement.rebate_amount, Money::from_minor_units(rebate));
        assert_eq!(statement.excess_amount, Money::from_minor_units(excess));
        let mut processor = SimulatedProcessor::new([7; 32]);
        processor.lose_next_response();
        assert_eq!(processor.submit(&op), Err(PaymentError::Unavailable));
        assert_eq!(
            prepare_distribution(&account, &mut program, id, at).unwrap(),
            Some(op.clone())
        );
        let receipt = processor.lookup(op.id).unwrap().unwrap();
        program
            .confirm_payout(id, &receipt, &processor.verifying_key(), at)
            .unwrap();
        let paid = program.clone();
        program
            .confirm_payout(id, &receipt, &processor.verifying_key(), at)
            .unwrap();
        assert_eq!(program, paid);
        assert_eq!(
            program.member_statement(MemberId(1))[0].outstanding,
            Money::ZERO
        );
        assert_eq!(processor.operation_count(), 1);
        assert!(
            prepare_distribution(&account, &mut program, id, at)
                .unwrap()
                .is_none()
        );
        assert_eq!(program.ledger().total_value(), Ok(Money::ZERO));
    }
}
#[test]
fn closure_and_pending_future_service_do_not_change_the_distribution() {
    let mut program = allocated(17000);
    let mut account = BillingAccount::new(bank(1));
    let mut processor = SimulatedProcessor::new([7; 32]);
    let current = ServiceOffer::new(
        PolicyVersion(1),
        ServicePeriod::annual(2026, 1, 1, LeapDayRule::February28).unwrap(),
        calendar_date(2025, 10, 1).unwrap(),
        Money::from_minor_units(12000),
        SettlementUnit(1),
    )
    .unwrap();
    let contract = account
        .purchase(&current, processor.verifying_key())
        .unwrap();
    let receipt = processor
        .submit(account.contracts()[&contract].collection().current())
        .unwrap();
    account
        .record_collection(contract, &receipt, current.collect_at())
        .unwrap();
    let next = ServiceOffer::new(
        PolicyVersion(2),
        ServicePeriod::annual(2027, 1, 1, LeapDayRule::February28).unwrap(),
        calendar_date(2026, 10, 1).unwrap(),
        Money::from_minor_units(15000),
        SettlementUnit(1),
    )
    .unwrap();
    account.purchase(&next, processor.verifying_key()).unwrap();
    account.close().unwrap();
    let id = program.payables().next().unwrap().id();
    let op = prepare_distribution(
        &account,
        &mut program,
        id,
        calendar_date(2026, 1, 1).unwrap(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(op.amount, Money::from_minor_units(17000));
    assert_eq!(
        program.member_statement(MemberId(1))[0].rebate_amount,
        Money::from_minor_units(12000)
    );
}
#[test]
fn wrong_beneficiary_and_tampered_payable_are_rejected() {
    let mut program = allocated(17000);
    let id = program.payables().next().unwrap().id();
    assert!(
        prepare_distribution(
            &BillingAccount::new(bank(2)),
            &mut program,
            id,
            calendar_date(2026, 1, 1).unwrap()
        )
        .is_err()
    );
    let mut account = BillingAccount::new(bank(1));
    account.close().unwrap();
    prepare_distribution(
        &account,
        &mut program,
        id,
        calendar_date(2026, 1, 1).unwrap(),
    )
    .unwrap();
    let mut json = serde_json::to_value(program.payables().next().unwrap()).unwrap();
    json["amount"] = serde_json::to_value(Money::from_minor_units(1)).unwrap();
    assert!(serde_json::from_value::<MemberPayable>(json).is_err());
}
#[test]
fn zero_allocation_does_not_create_a_bank_operation() {
    let program = allocated(0);
    assert_eq!(program.payables().count(), 0);
    assert_eq!(
        program
            .distribution(AnnualDistributionId(2025))
            .unwrap()
            .each,
        Money::ZERO
    );
}
