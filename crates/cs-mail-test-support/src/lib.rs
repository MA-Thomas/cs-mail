pub mod enrollment;
// Explicit paid-service setup for protocol regression fixtures, separate from domain acceptance tests.
use cs_mail_billing::*;
use cs_mail_finance::{BankVerification, PaymentProcessor, SimulatedProcessor};
use cs_mail_primitives::*;
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
    mut all_keys: cs_mail_security::KeyRegistry,
    sender: ProtocolIdentity,
    at: CanonicalTime,
) -> Result<(), StorageError> {
    arrangement(engine, policy)?;
    let recipient = engine.snapshot()?.state.relationship.key.recipient;
    if !all_keys
        .records()
        .any(|r| matches!(r.actor, cs_mail_protocol::ActorRef::Recipient(id) if id == recipient))
    {
        all_keys.register(
            OperationalKeyRef(102),
            cs_mail_protocol::ActorRef::Recipient(recipient),
            SigningKey::from_bytes(&[102; 32])
                .verifying_key()
                .to_bytes(),
            at,
        )?;
    }
    let mut provider_keys = cs_mail_security::KeyRegistry::default();
    for record in all_keys.records() {
        let (cs_mail_protocol::ActorRef::Sender(persona)
        | cs_mail_protocol::ActorRef::Recipient(persona)) = record.actor
        else {
            provider_keys.register(record.reference, record.actor, record.verifying_key, at)?;
            continue;
        };
        let account = match engine.accounts(move || at).persona_account(persona) {
            Ok(account) => account,
            Err(StorageError::Identity(identity_contract::Error::Invalid)) => {
                enroll_persona(engine, policy, persona, record, sender, at)?
            }
            Err(error) => return Err(error),
        };
        let billing = engine
            .accounts(move || at)
            .product_account(account)?
            .billing;
        purchase_service(engine, policy, billing, record.reference, at)?;
    }
    engine.initialize_key_registry(&provider_keys, at)?;
    Ok(())
}
fn enroll_persona(
    engine: &PostgresEngine,
    policy: &cs_mail_protocol::PolicySnapshot,
    persona: ProtocolIdentity,
    record: &cs_mail_security::OperationalKeyRecord,
    sender: ProtocolIdentity,
    at: CanonicalTime,
) -> Result<AccountId, StorageError> {
    let number = if persona == sender { 1 } else { persona.0 };
    let bank = BankVerification {
        scope: policy.financial.scope,
        account: BillingAccountId(number),
        member: MemberId(number),
        person: [u8::try_from(number).unwrap(); 32],
        bank_token: [u8::try_from(number + 8).unwrap(); 32],
        unit: policy.unit,
        version: 1,
        signature: vec![],
    }
    .sign(&[77; 32])
    .unwrap();
    enrollment::enroll(
        engine,
        cs_mail_accounts::EnrollmentInput {
            bank,
            persona,
            actor: record.actor,
            key_ref: record.reference,
            initial_key: record.verifying_key,
            maximum_unresolved: 100,
        },
        at,
    )
}
fn purchase_service(
    engine: &PostgresEngine,
    policy: &cs_mail_protocol::PolicySnapshot,
    id: BillingAccountId,
    reference: OperationalKeyRef,
    at: CanonicalTime,
) -> Result<(), StorageError> {
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
        reference,
        policy.financial.scope,
        0,
        IdempotencyKey(1),
        BillingCommand::PurchaseService {
            offer: offer.version(),
        },
        &[u8::try_from(reference.0).unwrap(); 32],
    )?;
    let op = cs_mail_application::billing::operations::BillingService::new(engine, &|| at)
        .execute_command(&signed)?
        .remove(0);
    cs_mail_application::billing::operations::BillingService::new(engine, &|| at)
        .authorize_dispatch(id, ServiceContractId(op.id.0), op.id)?;
    let mut processor = SimulatedProcessor::new([7; 32]);
    let receipt = processor.submit(&op).unwrap();
    cs_mail_application::billing::operations::BillingService::new(engine, &|| at).confirm_payment(
        id,
        ServiceContractId(op.id.0),
        &receipt,
    )?;
    Ok(())
}
