pub mod enrollment;
pub mod smtp;
// Explicit paid-service setup for protocol regression fixtures, separate from domain acceptance tests.
use cs_mail_billing::*;
use cs_mail_finance::{BankVerification, PaymentProcessor, SimulatedProcessor};
use cs_mail_primitives::*;
use cs_mail_storage_postgres::{PostgresEngine, StorageError};
use ed25519_dalek::SigningKey;
/// Processing component `C` of the regression fixtures' pricing policy.
pub const FIXTURE_PROCESSING_CHARGE: u64 = 2;
/// Collateral `S` of the fixtures' single published class.
pub const FIXTURE_COLLATERAL: u64 = 8;
/// The regression fixtures' operator pricing policy: `C = 2`, collateral bounds 1..=1000.
/// # Errors
/// Never fails for these constants; returns the constructor's error type.
pub fn fixture_pricing(
    unit: SettlementUnit,
) -> Result<cs_mail_protocol::pricing::RequestPricingPolicy, cs_mail_protocol::ProtocolError> {
    cs_mail_protocol::pricing::RequestPricingPolicy::new(
        PolicyVersion(1),
        unit,
        Money::from_minor_units(FIXTURE_PROCESSING_CHARGE),
        Money::from_minor_units(1),
        Money::from_minor_units(1000),
    )
}
/// Secret of the fixtures' single operator key, `OperatorKeyRef(1)`.
pub const FIXTURE_OPERATOR_SECRET: [u8; 32] = [90; 32];
fn public(secret: u8) -> [u8; 32] {
    SigningKey::from_bytes(&[secret; 32])
        .verifying_key()
        .to_bytes()
}
/// The regression fixtures' deployment: provider key 3, scheduler key 4, operator key 1,
/// payment processor `[7; 32]`, bank authority `[77; 32]` and the test identity issuer
/// with decision key `[80; 32]`, for the given scope and settlement unit.
/// # Errors
/// Rejects an inconsistent scope.
pub fn fixture_deployment(
    scope: cs_mail_finance::FinancialScope,
    unit: SettlementUnit,
) -> Result<cs_mail_operations::DeploymentBootstrap, StorageError> {
    Ok(cs_mail_operations::DeploymentBootstrap::new(
        cs_mail_operations::BootstrapRecord {
            provider: scope.operator,
            deployment_domain: scope.deployment_domain,
            financial_scope: scope,
            unit,
            provider_key: cs_mail_operations::ProviderKey {
                reference: OperationalKeyRef(3),
                verifying_key: public(3),
            },
            scheduler_key: cs_mail_operations::ProviderKey {
                reference: OperationalKeyRef(4),
                verifying_key: public(4),
            },
            operators: cs_mail_operations::OperatorKeys::new(
                [(
                    cs_mail_operations::OperatorKeyRef(1),
                    public(FIXTURE_OPERATOR_SECRET[0]),
                )]
                .into(),
            )?,
            payment: cs_mail_operations::PaymentTrust {
                processor_key: SimulatedProcessor::new([7; 32]).verifying_key(),
                bank_authority: public(77),
            },
            identity: cs_mail_operations::IdentityTrust {
                issuer: "https://identity.example.test".into(),
                product: "cs-mail/test".into(),
                decision_key: public(80),
            },
        },
    )?)
}
/// Bootstraps the fixture deployment, or confirms it (an identical repeat changes nothing).
/// # Errors
/// Refuses a differently bootstrapped deployment; persistence failures.
pub fn bootstrap(
    deployment: &cs_mail_storage_postgres::PostgresDeployment,
    scope: cs_mail_finance::FinancialScope,
    unit: SettlementUnit,
) -> Result<(), StorageError> {
    cs_mail_application::operations::bootstrap(
        deployment,
        &fixture_deployment(scope, unit)?,
        || CanonicalTime(0),
    )?;
    Ok(())
}
/// Executes one operator command signed by the fixture operator. The command identity is
/// derived from the command, so repeating a command is an exact replay.
/// # Errors
/// The operator service's refusals and persistence failures.
pub fn operator(
    deployment: &cs_mail_storage_postgres::PostgresDeployment,
    command: cs_mail_operations::OperatorCommand,
    at: CanonicalTime,
) -> Result<cs_mail_operations::OperatorOutcome, StorageError> {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(serde_json::to_vec(&command)?);
    let identity = u128::from_be_bytes(digest[..16].try_into().unwrap_or([0; 16]));
    let domain = deployment
        .deployment_record()?
        .ok_or(cs_mail_operations::OperationsError::NotBootstrapped)?
        .record()
        .deployment_domain;
    let signed = cs_mail_operations::SignedOperatorCommand::sign(
        domain,
        cs_mail_operations::OperatorKeyRef(1),
        IdempotencyKey(identity),
        command,
        &FIXTURE_OPERATOR_SECRET,
    )?;
    cs_mail_application::operations::OperatorService::new(deployment).execute(&signed, || at)
}
pub fn provision(
    engine: &PostgresEngine,
    policy: &cs_mail_protocol::PolicySnapshot,
    mut all_keys: cs_mail_security::KeyRegistry,
    sender: ProtocolIdentity,
    at: CanonicalTime,
) -> Result<(), StorageError> {
    bootstrap(engine.deployment(), policy.financial.scope, policy.unit)?;
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
        let account = match engine
            .deployment()
            .accounts(move || at)
            .persona_account(persona)
        {
            Ok(account) => account,
            Err(StorageError::Identity(identity_contract::Error::Invalid)) => {
                enroll_persona(engine, policy, persona, record, sender, at)?
            }
            Err(error) => return Err(error),
        };
        let billing = engine
            .deployment()
            .accounts(move || at)
            .product_account(account)?
            .billing;
        purchase_service(
            engine.deployment(),
            policy.financial.scope,
            policy.unit,
            billing,
            record.reference,
            at,
        )?;
    }
    engine.initialize_key_registry(&provider_keys, at)?;
    engine.configure_ingress(
        policy.financial.scope.deployment_domain,
        policy,
        &cs_mail_protocol::admission::AdmissionPolicy::default(),
    )?;
    let pricing =
        cs_mail_application::request_pricing::RequestPricingService::new(engine.deployment());
    if engine.deployment().request_pricing_policy()?.is_none() {
        operator(
            engine.deployment(),
            cs_mail_operations::OperatorCommand::PublishRequestPricing(fixture_pricing(
                policy.unit,
            )?),
            at,
        )?;
    }
    if engine.deployment().request_classes(recipient)?.is_none() {
        let record = all_keys
            .records()
            .find(|r| r.actor == cs_mail_protocol::ActorRef::Recipient(recipient))
            .unwrap();
        let classes = cs_mail_protocol::pricing::RecipientRequestClasses::new(
            recipient,
            1,
            vec![cs_mail_protocol::pricing::RequestClass::new(
                RequestClassId(1),
                "Test class".into(),
                Money::from_minor_units(FIXTURE_COLLATERAL),
            )?],
        )?;
        let signer = cs_mail_security::CommandSigner::from_secret_bytes(
            record.actor,
            record.reference,
            &[u8::try_from(record.reference.0).unwrap(); 32],
        );
        let scope = cs_mail_security::RecipientSigningScope {
            deployment_domain: policy.financial.scope.deployment_domain,
            provider: policy.recipient_provider,
        };
        pricing.publish_classes(&signer.sign_request_classes(scope, classes)?, scope, || at)?;
    }
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
        bank_token: [u8::try_from(number + 8).unwrap(); 32],
        unit: policy.unit,
        version: 1,
        signature: vec![],
    }
    .sign(&[77; 32])
    .unwrap();
    enrollment::enroll(
        engine.deployment(),
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
/// Purchases the fixtures' 1971 service year for a billing account whose key secret is
/// its reference repeated, and confirms its collection through the simulator.
/// # Errors
/// Billing refusals and persistence failures.
pub fn purchase_service(
    deployment: &cs_mail_storage_postgres::PostgresDeployment,
    scope: cs_mail_finance::FinancialScope,
    unit: SettlementUnit,
    id: BillingAccountId,
    reference: OperationalKeyRef,
    at: CanonicalTime,
) -> Result<(), StorageError> {
    if !deployment.billing_account(id)?.contracts().is_empty() {
        return Ok(());
    }
    let offer = ServiceOffer::new(
        PolicyVersion(1),
        ServicePeriod::annual(1971, 1, 1, LeapDayRule::February28)?,
        calendar_date(1970, 10, 1).unwrap(),
        Money::from_minor_units(12000),
        unit,
    )?;
    operator(
        deployment,
        cs_mail_operations::OperatorCommand::PublishServiceOffer(offer.clone()),
        at,
    )?;
    let signed = SignedBillingCommand::sign(
        id,
        reference,
        scope,
        0,
        IdempotencyKey(1),
        BillingCommand::PurchaseService {
            offer: offer.version(),
        },
        &[u8::try_from(reference.0).unwrap(); 32],
    )?;
    let op = cs_mail_application::billing::operations::BillingService::new(deployment, &|| at)
        .execute_command(&signed)?
        .remove(0);
    cs_mail_application::billing::operations::BillingService::new(deployment, &|| at)
        .authorize_dispatch(id, ServiceContractId(op.id.0), op.id)?;
    let mut processor = SimulatedProcessor::new([7; 32]);
    let receipt = processor.submit(&op).unwrap();
    cs_mail_application::billing::operations::BillingService::new(deployment, &|| at)
        .confirm_payment(id, ServiceContractId(op.id.0), &receipt)?;
    Ok(())
}

pub mod external;

pub mod smtp2go;
