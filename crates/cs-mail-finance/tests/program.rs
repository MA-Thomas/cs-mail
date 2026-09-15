use cs_mail_finance::*;
use cs_mail_ledger::Account;
use cs_mail_primitives::*;
fn eligibility() -> EligibilityPolicy {
    EligibilityPolicy {
        version: PolicyVersion(1),
        minimum_tenure: Duration(30 * DAY_MILLIS),
        minimum_active_days: 2,
    }
}
fn schedule(q: u8) -> AnnualDistributionSchedule {
    AnnualDistributionSchedule::utc(
        1969 + u16::from(q),
        eligibility(),
        cs_mail_finance::DistributionTerms::new(
            cs_mail_primitives::PolicyVersion(1),
            cs_mail_primitives::Money::from_minor_units(12000),
            cs_mail_primitives::calendar_date((1969 + u16::from(q)) + 1, 1, 1).unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn amount(n: u64) -> Money {
    Money::from_minor_units(n)
}
fn status() -> MembershipStatus {
    MembershipStatus {
        opted_in: true,
        verified: true,
        suspended: false,
    }
}
fn funding(id: u128, n: u64, rate: u16, version: u64) -> Forfeiture {
    Forfeiture {
        id: PaymentOperationId(id),
        amount: amount(n),
        unit: SettlementUnit(1),
        forfeited_at: CanonicalTime(1),
        requires_review: false,
        terms: FinancialTerms {
            scope: cs_mail_finance::FinancialScope::new(
                [7; 32],
                cs_mail_primitives::ProviderRef(30),
                cs_mail_primitives::ProgramRef(1),
                [9; 32],
                cs_mail_primitives::ProtocolVersion(2),
            ),
            policy_version: PolicyVersion(version),
            corporate_basis_points: rate,
            maturity_delay: Duration(10),
        },
    }
}
fn populated() -> FinancialProgram {
    let mut p = FinancialProgram::new(
        cs_mail_finance::FinancialScope::new(
            [7; 32],
            cs_mail_primitives::ProviderRef(30),
            cs_mail_primitives::ProgramRef(1),
            [9; 32],
            cs_mail_primitives::ProtocolVersion(2),
        ),
        SettlementUnit(1),
    );
    p.publish_annual_distribution(schedule(1), CanonicalTime(0))
        .unwrap();
    p.publish_annual_distribution(schedule(2), CanonicalTime(0))
        .unwrap();
    for i in 1..=3 {
        p.enroll(
            MemberId(i),
            [u8::try_from(i).unwrap(); 32],
            status(),
            CanonicalTime(0),
        )
        .unwrap();
    }
    for i in 1..=3 {
        p.record_activity(
            MemberId(i),
            IntentionalActivity::Read,
            CanonicalTime(DAY_MILLIS),
        )
        .unwrap();
    }
    for i in 1..=3 {
        p.record_activity(
            MemberId(i),
            IntentionalActivity::Decision,
            CanonicalTime(2 * DAY_MILLIS),
        )
        .unwrap();
    }
    p
}
#[test]
fn exact_calendar_annual_allocations_and_cutoff_activity() {
    assert_eq!(schedule(1).cutoff, CanonicalTime(365 * DAY_MILLIS));
    assert_eq!(
        AnnualDistributionSchedule::utc(
            1972,
            eligibility(),
            cs_mail_finance::DistributionTerms::new(
                cs_mail_primitives::PolicyVersion(1),
                cs_mail_primitives::Money::from_minor_units(12000),
                cs_mail_primitives::calendar_date((1972) + 1, 1, 1).unwrap()
            )
            .unwrap()
        )
        .unwrap()
        .cutoff
        .0 - AnnualDistributionSchedule::utc(
            1972,
            eligibility(),
            cs_mail_finance::DistributionTerms::new(
                cs_mail_primitives::PolicyVersion(1),
                cs_mail_primitives::Money::from_minor_units(12000),
                cs_mail_primitives::calendar_date((1972) + 1, 1, 1).unwrap()
            )
            .unwrap()
        )
        .unwrap()
        .start
        .0,
        366 * DAY_MILLIS
    );
    let mut p = populated();
    p.record_forfeiture(funding(1, 10000, 300, 1), CanonicalTime(3 * DAY_MILLIS))
        .unwrap();
    p.clear_maturity(
        PaymentOperationId(1),
        FinancialEventId(1),
        CanonicalTime(3 * DAY_MILLIS),
    )
    .unwrap();
    let cutoff = schedule(1).cutoff;
    // A new status at the boundary belongs to the new year, not the closing year.
    p.set_membership(
        MemberId(1),
        MembershipStatus {
            opted_in: false,
            ..status()
        },
        cutoff,
    )
    .unwrap();
    let q = p
        .finalize_annual_distribution(schedule(1).id, cutoff)
        .unwrap();
    assert_eq!(q.members, vec![MemberId(1), MemberId(2), MemberId(3)]);
    assert_eq!(q.corporate_share, amount(300));
    assert_eq!(q.each, amount(3233));
    assert_eq!(q.remainder, amount(1));
    assert_eq!(p.payables().count(), 3);
    let before = p.clone();
    assert_eq!(
        p.finalize_annual_distribution(schedule(1).id, CanonicalTime(cutoff.0 + 1))
            .unwrap(),
        q
    );
    assert_eq!(p, before);
    let q2 = p
        .finalize_annual_distribution(schedule(2).id, schedule(2).cutoff)
        .unwrap();
    assert!(q2.members.is_empty());
    assert_eq!(q2.corporate_share, Money::ZERO);
    assert_eq!(q2.remainder, amount(1));
    assert_eq!(p.payables().count(), 3);
    assert_eq!(p.ledger().total_value(), Ok(Money::ZERO));
}
#[test]
fn rate_cohort_rounding_and_each_source_assessed_once() {
    let mut p = populated();
    let at = CanonicalTime(3 * DAY_MILLIS);
    for (id, n, rate, v) in [(1, 20, 300, 1), (2, 20, 300, 1), (3, 20, 500, 2)] {
        p.record_forfeiture(funding(id, n, rate, v), at).unwrap();
        p.clear_maturity(PaymentOperationId(id), FinancialEventId(id), at)
            .unwrap();
    }
    let q = p
        .finalize_annual_distribution(schedule(1).id, schedule(1).cutoff)
        .unwrap();
    assert_eq!(q.corporate_share, amount(2));
    assert_eq!(q.member_contribution, amount(58));
    assert_eq!(q.each, amount(19));
    assert_eq!(q.remainder, amount(1));
    let q2 = p
        .finalize_annual_distribution(schedule(2).id, schedule(2).cutoff)
        .unwrap();
    assert_eq!(q2.corporate_share, Money::ZERO);
    assert_eq!(p.ledger().balance(Account::CorporatePoolRevenue), amount(2));
}
#[test]
fn immature_held_or_cutoff_boundary_sources_stay_pending() {
    let mut p = FinancialProgram::new(
        cs_mail_finance::FinancialScope::new(
            [7; 32],
            cs_mail_primitives::ProviderRef(30),
            cs_mail_primitives::ProgramRef(1),
            [9; 32],
            cs_mail_primitives::ProtocolVersion(2),
        ),
        SettlementUnit(1),
    );
    p.publish_annual_distribution(schedule(1), CanonicalTime(0))
        .unwrap();
    p.record_forfeiture(funding(1, 100, 300, 1), CanonicalTime(1))
        .unwrap();
    assert_eq!(
        p.clear_maturity(
            PaymentOperationId(1),
            FinancialEventId(1),
            CanonicalTime(10)
        ),
        Err(ProgramError::TooEarly)
    );
    p.set_hold(PaymentOperationId(1), true, CanonicalTime(11))
        .unwrap();
    assert!(
        p.clear_maturity(
            PaymentOperationId(1),
            FinancialEventId(1),
            CanonicalTime(12)
        )
        .is_err()
    );
    p.record_forfeiture(funding(2, 100, 300, 1), CanonicalTime(12))
        .unwrap();
    p.clear_maturity(
        PaymentOperationId(2),
        FinancialEventId(2),
        schedule(1).cutoff,
    )
    .unwrap();
    let q = p
        .finalize_annual_distribution(schedule(1).id, schedule(1).cutoff)
        .unwrap();
    assert!(q.funding.is_empty());
    assert_eq!(
        p.ledger()
            .balance(Account::PendingForfeiture(PaymentOperationId(1))),
        amount(100)
    );
    assert_eq!(
        p.ledger()
            .balance(Account::PendingForfeiture(PaymentOperationId(2))),
        amount(100)
    );
}
#[test]
fn duplicate_identity_and_fabricated_calendar_are_rejected() {
    let mut p = populated();
    let before = p.clone();
    assert!(
        p.enroll(
            MemberId(4),
            [1; 32],
            status(),
            CanonicalTime(3 * DAY_MILLIS)
        )
        .is_err()
    );
    assert_eq!(p, before);
    let mut invalid = schedule(3);
    invalid.cutoff.0 += 1;
    assert!(
        p.publish_annual_distribution(invalid, CanonicalTime(3 * DAY_MILLIS))
            .is_err()
    );
    assert!(
        p.record_activity(MemberId(1), IntentionalActivity::Read, CanonicalTime(1))
            .is_err()
    );
}

#[test]
fn persistence_round_trip_preserves_large_ids_and_signed_balances() {
    let mut p = populated();
    let at = CanonicalTime(3 * DAY_MILLIS);
    p.record_forfeiture(funding(u128::MAX, 10000, 300, 1), at)
        .unwrap();
    p.clear_maturity(PaymentOperationId(u128::MAX), FinancialEventId(1), at)
        .unwrap();
    p.finalize_annual_distribution(schedule(1).id, schedule(1).cutoff)
        .unwrap();
    let json = serde_json::to_vec(&p).unwrap();
    let restored: FinancialProgram = serde_json::from_slice(&json).unwrap();
    assert_eq!(restored, p);
}
#[test]
fn no_members_carries_net_funds_without_assessing_them_again() {
    let mut p = FinancialProgram::new(
        cs_mail_finance::FinancialScope::new(
            [7; 32],
            cs_mail_primitives::ProviderRef(30),
            cs_mail_primitives::ProgramRef(1),
            [9; 32],
            cs_mail_primitives::ProtocolVersion(2),
        ),
        SettlementUnit(1),
    );
    p.publish_annual_distribution(schedule(1), CanonicalTime(0))
        .unwrap();
    p.publish_annual_distribution(schedule(2), CanonicalTime(0))
        .unwrap();
    p.record_forfeiture(funding(1, 100, 300, 1), CanonicalTime(1))
        .unwrap();
    p.clear_maturity(
        PaymentOperationId(1),
        FinancialEventId(1),
        CanonicalTime(11),
    )
    .unwrap();
    let q = p
        .finalize_annual_distribution(schedule(1).id, schedule(1).cutoff)
        .unwrap();
    assert_eq!(q.remainder, amount(97));
    assert!(p.payables().next().is_none());
    let q2 = p
        .finalize_annual_distribution(schedule(2).id, schedule(2).cutoff)
        .unwrap();
    assert_eq!(q2.remainder, amount(97));
    assert_eq!(q2.corporate_share, Money::ZERO);
}

#[test]
fn authorized_corrections_preserve_the_distribution_and_restricted_carryforward() {
    let mut p = populated();
    let at = CanonicalTime(3 * DAY_MILLIS);
    p.record_forfeiture(funding(1, 100, 300, 1), at).unwrap();
    p.clear_maturity(PaymentOperationId(1), FinancialEventId(1), at)
        .unwrap();
    let distribution = p
        .finalize_annual_distribution(schedule(1).id, schedule(1).cutoff)
        .unwrap();
    let carry = p.ledger().balance(Account::RestrictedMemberFunds);
    p.compensate_member(
        FinancialEventId(901),
        MemberId(1),
        distribution.schedule.id,
        amount(5),
        "Confirmed eligibility correction",
        schedule(1).cutoff,
    )
    .unwrap();
    assert_eq!(
        p.distribution(distribution.schedule.id),
        Some(&distribution)
    );
    assert_eq!(p.ledger().balance(Account::RestrictedMemberFunds), carry);
    assert_eq!(
        p.ledger().signed_balance(Account::CorporateLossClearing),
        -5
    );
    assert!(
        p.member_statement(MemberId(1))
            .iter()
            .any(|e| e.correction == Some(FinancialEventId(901)) && e.amount == amount(5))
    );
    let before = p.clone();
    assert!(
        p.compensate_member(
            FinancialEventId(901),
            MemberId(1),
            distribution.schedule.id,
            amount(50),
            "Conflicting replay",
            schedule(1).cutoff
        )
        .is_err()
    );
    assert_eq!(before, p);
}
#[test]
fn administration_signature_binds_unit_command_and_expected_revision() {
    let secret = [11; 32];
    let key = SimulatedProcessor::new(secret).verifying_key();
    let signed = SignedProgramCommand::sign(
        cs_mail_finance::FinancialScope::new(
            [7; 32],
            cs_mail_primitives::ProviderRef(30),
            cs_mail_primitives::ProgramRef(1),
            [9; 32],
            cs_mail_primitives::ProtocolVersion(2),
        ),
        SettlementUnit(1),
        IdempotencyKey(u128::MAX),
        3,
        ProgramCommand::FinalizeAnnualDistribution(schedule(1).id),
        &secret,
    )
    .unwrap();
    signed.verify(&key).unwrap();
    let mut changed = signed.clone();
    changed.unit = SettlementUnit(2);
    assert!(changed.verify(&key).is_err());
    let encoded = serde_json::to_value(&signed).unwrap();
    let restored: SignedProgramCommand = serde_json::from_value(encoded).unwrap();
    assert_eq!(signed, restored);
}

#[test]
fn reversal_holds_unassessed_funds_and_preserves_finalized_obligations() {
    let mut p = populated();
    let at = CanonicalTime(3 * DAY_MILLIS);
    for id in 1..=2 {
        p.record_forfeiture(funding(id, 100, 300, 1), at).unwrap();
        p.clear_maturity(PaymentOperationId(id), FinancialEventId(id), at)
            .unwrap();
    }
    p.note_reversal(PaymentOperationId(1), at).unwrap();
    let q = p
        .finalize_annual_distribution(schedule(1).id, schedule(1).cutoff)
        .unwrap();
    assert_eq!(q.newly_eligible, amount(100));
    assert_eq!(
        p.ledger()
            .balance(Account::PendingForfeiture(PaymentOperationId(1))),
        amount(100)
    );
    let before = p.ledger();
    p.note_reversal(PaymentOperationId(2), schedule(1).cutoff)
        .unwrap();
    assert_eq!(p.ledger(), before);
    assert_eq!(p.distribution(schedule(1).id), Some(&q));
}

#[test]
fn a_selected_storage_owner_cannot_finalize_or_enroll_members() {
    let p = populated();
    let mut point =
        FinancialProgram::restore_owner(p.metadata(), ProgramOwner::Metadata, p.ledger()).unwrap();
    assert_eq!(
        point.finalize_annual_distribution(schedule(1).id, schedule(1).cutoff),
        Err(ProgramError::IncompleteSnapshot)
    );
    assert_eq!(
        point.enroll(MemberId(99), [99; 32], status(), schedule(1).cutoff),
        Err(ProgramError::IncompleteSnapshot)
    );
}
