use cs_mail_finance::*;
use cs_mail_primitives::*;
use ed25519_dalek::SigningKey;
fn bank(version: u64) -> VerifiedBankAccount {
    BankVerification {
        scope: operation(1).scope,
        account: BillingAccountId(1),
        member: MemberId(1),
        person: [8; 32],
        bank_token: [9; 32],
        unit: SettlementUnit(1),
        version,
        signature: Vec::new(),
    }
    .sign(&[7; 32])
    .unwrap()
    .verify(&SigningKey::from_bytes(&[7; 32]).verifying_key().to_bytes())
    .unwrap()
}
fn operation(id: u128) -> PaymentOperation {
    PaymentOperation {
        scope: FinancialScope::new(
            [1; 32],
            ProviderRef(2),
            ProgramRef(3),
            [4; 32],
            ProtocolVersion(2),
        ),
        id: PaymentOperationId(id),
        kind: PaymentKind::Capture,
        amount: Money::from_minor_units(100),
        unit: SettlementUnit(1),
        destination: [9; 32],
    }
}
#[test]
fn queued_charge_is_rechecked_before_first_dispatch() {
    let mut source = FundingSource::verified(&bank(1), 2).unwrap();
    let mut processor = SimulatedProcessor::new([7; 32]);
    source.reserve(&operation(1)).unwrap();
    source.reserve(&operation(2)).unwrap();
    source.authorize_dispatch(PaymentOperationId(1)).unwrap();
    processor.pend_next_submission();
    processor.submit(&operation(1)).unwrap();
    let failed = processor
        .resolve(
            PaymentOperationId(1),
            FinancialEventId(90),
            PaymentOutcome::Failed,
        )
        .unwrap();
    source.record(&failed, &processor.verifying_key()).unwrap();
    assert!(source.restricted());
    assert_eq!(
        source.authorize_dispatch(PaymentOperationId(2)),
        Err(PaymentError::FundingRestricted)
    );
    // Already authorized work remains reconcilable; reservation alone grants no dispatch.
    source.authorize_dispatch(PaymentOperationId(1)).unwrap();
    source.reverify(&bank(2)).unwrap();
    source.authorize_dispatch(PaymentOperationId(2)).unwrap();
    assert!(source.reverify(&bank(2)).is_err());
}
#[test]
fn unresolved_limit_is_not_a_lifetime_attempt_limit() {
    let mut source = FundingSource::verified(&bank(1), 1).unwrap();
    let mut processor = SimulatedProcessor::new([7; 32]);
    for id in 1..=3 {
        source.reserve(&operation(id)).unwrap();
        assert!(source.reserve(&operation(id + 1)).is_err());
        source.authorize_dispatch(PaymentOperationId(id)).unwrap();
        let paid = processor.submit(&operation(id)).unwrap();
        source.record(&paid, &processor.verifying_key()).unwrap();
    }
}
#[test]
fn cancellation_reaches_an_already_pending_capture() {
    let mut processor = SimulatedProcessor::new([7; 32]);
    processor.pend_next_submission();
    let pending = processor.submit(&operation(1)).unwrap();
    assert_eq!(pending.evidence.outcome, PaymentOutcome::Pending);
    let cancellation = CaptureCancellation::new(operation(1)).unwrap();
    let voided = processor.cancel_capture(&cancellation).unwrap();
    assert_eq!(voided.evidence.outcome, PaymentOutcome::Voided);
    assert_eq!(processor.cancel_capture(&cancellation).unwrap(), voided);
    assert_eq!(processor.submit(&operation(1)).unwrap(), voided);
    let mut payout = operation(2);
    payout.kind = PaymentKind::MemberPayout {
        allocation: AllocationId(7),
    };
    assert!(CaptureCancellation::new(payout).is_err());
}
#[test]
fn uncertain_attempt_cannot_be_replaced_and_late_conflicting_evidence_is_rejected() {
    let mut processor = SimulatedProcessor::new([7; 32]);
    let op = operation(1);
    let mut payment = PaymentExecution::new(op.clone()).unwrap();
    processor.lose_next_response();
    assert!(processor.submit(&op).is_err());
    assert!(payment.retry(PaymentOperationId(2)).is_err());
    let paid = processor.lookup(op.id).unwrap().unwrap();
    payment.record(&paid, &processor.verifying_key()).unwrap();
    assert!(payment.retry(PaymentOperationId(2)).is_err());
    let mut stored = serde_json::to_value(payment).unwrap();
    stored["events"] = serde_json::json!({});
    assert!(serde_json::from_value::<PaymentExecution>(stored).is_err());
}
#[test]
fn bank_evidence_is_scoped_signed_and_not_a_numeric_assertion() {
    let mut evidence = bank(1).evidence().clone();
    evidence.account = BillingAccountId(2);
    assert!(
        evidence
            .verify(&SigningKey::from_bytes(&[7; 32]).verifying_key().to_bytes())
            .is_err()
    );
}

#[test]
fn a_failed_receipt_cannot_be_restored_as_a_pending_attempt() {
    let mut processor = SimulatedProcessor::new([7; 32]);
    let op = operation(50);
    let mut payment = PaymentExecution::new(op.clone()).unwrap();
    processor.pend_next_submission();
    processor.submit(&op).unwrap();
    let failed = processor
        .resolve(op.id, FinancialEventId(51), PaymentOutcome::Failed)
        .unwrap();
    payment.record(&failed, &processor.verifying_key()).unwrap();
    let mut stored = serde_json::to_value(&payment).unwrap();
    stored["attempts"][0]["state"] = serde_json::json!("Pending");
    assert!(serde_json::from_value::<PaymentExecution>(stored).is_err());
    assert!(payment.retry(PaymentOperationId(0)).is_err());
    assert_eq!(payment.attempt_count(), 1);
}
