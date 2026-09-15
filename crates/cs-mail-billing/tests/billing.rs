use cs_mail_billing::*;
use cs_mail_finance::*;
use cs_mail_primitives::*;
use ed25519_dalek::SigningKey;
fn account() -> BillingAccount {
    let evidence = BankVerification {
        scope: FinancialScope::new(
            [1; 32],
            ProviderRef(2),
            ProgramRef(3),
            [4; 32],
            ProtocolVersion(2),
        ),
        account: BillingAccountId(1),
        member: MemberId(1),
        person: [5; 32],
        bank_token: [9; 32],
        unit: SettlementUnit(1),
        version: 1,
        signature: Vec::new(),
    }
    .sign(&[7; 32])
    .unwrap();
    BillingAccount::new(
        evidence
            .verify(&SigningKey::from_bytes(&[7; 32]).verifying_key().to_bytes())
            .unwrap(),
    )
}
fn offer() -> ServiceOffer {
    ServiceOffer::new(
        PolicyVersion(1),
        ServicePeriod::annual(2026, 7, 1, LeapDayRule::February28).unwrap(),
        calendar_date(2026, 4, 1).unwrap(),
        Money::from_minor_units(12000),
        SettlementUnit(1),
    )
    .unwrap()
}
#[test]
fn advance_collection_does_not_start_service_early() {
    let mut account = account();
    let terms = offer();
    let mut processor = SimulatedProcessor::new([7; 32]);
    let id = account.purchase(&terms, processor.verifying_key()).unwrap();
    let receipt = processor
        .submit(account.contracts()[&id].collection().current())
        .unwrap();
    account
        .record_collection(id, &receipt, terms.collect_at())
        .unwrap();
    assert!(!account.covers(terms.collect_at()));
    assert!(!account.covers(CanonicalTime(terms.period().start().0 - 1)));
    assert!(account.covers(terms.period().start()));
    assert!(!account.covers(terms.period().end()));
    let old = account.clone();
    assert_eq!(
        account.purchase(&terms, processor.verifying_key()).unwrap(),
        id
    );
    assert_eq!(account, old);
    let restored: BillingAccount =
        serde_json::from_slice(&serde_json::to_vec(&account).unwrap()).unwrap();
    assert_eq!(restored, account);
}
#[test]
fn late_retry_preserves_the_purchased_period_and_price() {
    let mut account = account();
    let terms = offer();
    let mut processor = SimulatedProcessor::new([7; 32]);
    let id = account.purchase(&terms, processor.verifying_key()).unwrap();
    assert!(account.retry_collection(id).is_err());
    processor.pend_next_submission();
    let op = account.contracts()[&id].collection().current().clone();
    processor.submit(&op).unwrap();
    let failed = processor
        .resolve(op.id, FinancialEventId(90), PaymentOutcome::Failed)
        .unwrap();
    account
        .record_collection(id, &failed, terms.collect_at())
        .unwrap();
    let retry = account.retry_collection(id).unwrap();
    assert_ne!(retry.id, op.id);
    assert_eq!(retry.amount, op.amount);
    assert_eq!(account.contracts()[&id].offer(), &terms);
    let paid = processor.submit(&retry).unwrap();
    let late = calendar_date(2026, 8, 1).unwrap();
    account.record_collection(id, &paid, late).unwrap();
    assert!(!account.covers(terms.period().start()));
    assert!(account.covers(late));
    assert!(!account.covers(terms.period().end()));
    assert!(account.retry_collection(id).is_err());
}
#[test]
fn invalid_periods_overlap_and_corrupt_funding_are_rejected() {
    assert!(ServicePeriod::annual(2026, 4, 31, LeapDayRule::February28).is_err());
    let leap = ServicePeriod::annual(2024, 2, 29, LeapDayRule::March1).unwrap();
    assert_eq!(leap.end(), calendar_date(2025, 3, 1).unwrap());
    let terms = offer();
    assert!(
        ServiceOffer::new(
            PolicyVersion(1),
            terms.period(),
            terms.period().start(),
            terms.price(),
            terms.unit()
        )
        .is_err()
    );
    let mut account = account();
    let id = account.purchase(&terms, [7; 32]).unwrap();
    let overlap = ServiceOffer::new(
        PolicyVersion(2),
        ServicePeriod::annual(2026, 9, 1, LeapDayRule::February28).unwrap(),
        terms.collect_at(),
        terms.price(),
        terms.unit(),
    )
    .unwrap();
    assert!(account.purchase(&overlap, [7; 32]).is_err());
    let mut stored = serde_json::to_value(&account).unwrap();
    stored["contracts"][id.0.to_string()]["settlement"] =
        serde_json::json!({"Funded":{"at":terms.collect_at().0}});
    assert!(serde_json::from_value::<BillingAccount>(stored).is_err());
}
