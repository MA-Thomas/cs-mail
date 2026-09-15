//! Explicit paid-service setup for protocol regression fixtures, separate from domain acceptance tests.
use cs_mail_billing::*;
use cs_mail_finance::{BankVerification, PaymentProcessor, SimulatedProcessor};
use cs_mail_primitives::*;
use cs_mail_protocol::ActorRef;
use cs_mail_storage_postgres::{PostgresEngine, StorageError};
use ed25519_dalek::SigningKey;
pub fn arrangement(
    engine: &PostgresEngine,
    policy: &cs_mail_protocol::PolicySnapshot,
) -> Result<(), StorageError> {
    engine.configure_payment_arrangement(
        policy.financial.scope,
        policy.unit,
        SimulatedProcessor::new([7; 32]).verifying_key(),
        SigningKey::from_bytes(&[77; 32]).verifying_key().to_bytes(),
    )
}
pub fn provision(
    engine: &PostgresEngine,
    policy: &cs_mail_protocol::PolicySnapshot,
) -> Result<(), StorageError> {
    arrangement(engine, policy)?;
    engine.initialize_key_registry(&super::registry(), super::test_time(0))?;
    let snapshot = engine.snapshot()?;
    let id = BillingAccountId(1);
    let evidence = BankVerification {
        scope: policy.financial.scope,
        account: id,
        member: MemberId(1),
        person: [1; 32],
        bank_token: [9; 32],
        unit: policy.unit,
        version: 1,
        signature: Vec::new(),
    }
    .sign(&[77; 32])
    .unwrap();
    engine.register_billing_account(
        &evidence,
        &[
            snapshot.state.relationship.key.sender,
            snapshot.state.relationship.key.recipient,
            super::SENDER,
        ],
        &[ActorRef::Sender(super::SENDER)],
        100,
    )?;
    if !engine.billing_account(id)?.contracts().is_empty() {
        return Ok(());
    }
    engine.configure_request_pricing(&cs_mail_protocol::pricing::RequestPricingPolicy {
        version: 1,
        processing_charge: policy.processing_charge,
        default_collateral: policy.collateral,
        collateral_choices: vec![policy.collateral],
    })?;
    let offer = ServiceOffer::new(
        PolicyVersion(1),
        ServicePeriod::annual(1971, 1, 1, LeapDayRule::February28)?,
        calendar_date(1970, 10, 1).unwrap(),
        Money::from_minor_units(12000),
        policy.unit,
    )?;
    engine.publish_service_offer(&offer)?;
    let signed = SignedBillingCommand::sign(
        id,
        OperationalKeyRef(1),
        policy.financial.scope,
        0,
        IdempotencyKey(1),
        BillingCommand::PurchaseService {
            offer: offer.version(),
        },
        &[1; 32],
    )?;
    let op = engine
        .execute_billing_command(&signed, super::test_time(0))?
        .remove(0);
    engine.authorize_utility_dispatch(
        id,
        ServiceContractId(op.id.0),
        op.id,
        super::test_time(0),
    )?;
    let mut processor = SimulatedProcessor::new([7; 32]);
    let receipt = processor.submit(&op).unwrap();
    engine.confirm_utility_payment(
        id,
        ServiceContractId(op.id.0),
        &receipt,
        super::test_time(0),
    )?;
    Ok(())
}
