use cs_mail_finance::*;
use cs_mail_primitives::*;
fn scope() -> FinancialScope {
    FinancialScope::new(
        [1; 32],
        ProviderRef(1),
        ProgramRef(1),
        [2; 32],
        ProtocolVersion(2),
    )
}
fn alternatives() -> [FinancialScope; 5] {
    [
        FinancialScope {
            deployment_domain: [3; 32],
            ..scope()
        },
        FinancialScope {
            operator: ProviderRef(2),
            ..scope()
        },
        FinancialScope {
            program: ProgramRef(2),
            ..scope()
        },
        FinancialScope {
            payment_account: [3; 32],
            ..scope()
        },
        FinancialScope {
            protocol_version: ProtocolVersion(3),
            ..scope()
        },
    ]
}
#[test]
fn financial_evidence_and_operation_ids_are_bound_to_every_scope_dimension() {
    let relationship = RelationshipRef::from_u128_for_test(1);
    let id = request_payment_id(scope(), relationship, RequestId(1), false);
    let operation = PaymentOperation {
        scope: scope(),
        id,
        kind: PaymentKind::Capture,
        amount: Money::from_minor_units(100),
        unit: SettlementUnit(1),
        destination: [9; 32],
    };
    let mut provider = SimulatedProcessor::new([7; 32]);
    let evidence = provider.submit(&operation).unwrap();
    for other in alternatives() {
        assert_ne!(
            request_payment_id(other, relationship, RequestId(1), false),
            id
        );
        let altered = PaymentOperation {
            scope: other,
            ..operation.clone()
        };
        assert_ne!(altered.digest(), operation.digest());
        assert!(
            evidence
                .verify(&provider.verifying_key(), &altered)
                .is_err()
        );
    }
}
#[test]
fn financial_administration_signature_binds_deployment_program_account_and_versions() {
    let key = ed25519_dalek::SigningKey::from_bytes(&[7; 32])
        .verifying_key()
        .to_bytes();
    let command = SignedProgramCommand::sign(
        scope(),
        SettlementUnit(1),
        IdempotencyKey(1),
        0,
        ProgramCommand::Enroll {
            member: MemberId(1),
            identity_digest: [8; 32],
            status: MembershipStatus {
                opted_in: true,
                verified: true,
                suspended: false,
            },
        },
        &[7; 32],
    )
    .unwrap();
    command.verify(&key).unwrap();
    for other in alternatives() {
        let altered = SignedProgramCommand {
            scope: other,
            ..command.clone()
        };
        assert!(altered.verify(&key).is_err());
    }
}
