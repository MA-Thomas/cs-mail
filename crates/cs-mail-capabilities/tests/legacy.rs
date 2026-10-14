use cs_mail_adapters::{DmarcAlignment, DmarcPass, DomainIdentity, VerifiedDomain};
use cs_mail_capabilities::{LegacyBondFreeAdmission, verify_legacy_admission};
use cs_mail_primitives::*;

fn evidence() -> VerifiedDomain {
    VerifiedDomain::passed(
        DomainIdentity::parse_ascii("alerts.example.com").unwrap(),
        Some(DmarcPass {
            alignment: DmarcAlignment::Relaxed,
        }),
        None,
        CanonicalTime(1),
    )
    .unwrap()
}
#[test]
fn verified_legacy_identity_and_origin_do_not_require_a_lane() {
    let domain = DomainIdentity::parse_ascii("example.com").unwrap();
    let mut request = LegacyBondFreeAdmission {
        sender: domain.synthetic_protocol_identity(&[7; 32], 1),
        recipient: ProtocolIdentity(2),
        message_id: MessageId(1),
        content_ref: ContentRef(1),
        delivery_intent_ref: DeliveryIntentRef(1),
        purpose: DeclaredPurpose::Known(KnownPurpose::Transactional),
        payload_schema: None,
        message_valid_until: MessageValidityUntil(CanonicalTime(100)),
        capability: None,
        idempotency_key: IdempotencyKey(1),
        protocol_version: ProtocolVersion(2),
        deployment_domain: [7; 32],
        intended_provider: ProviderRef(30),
        sender_domain: domain,
    };
    let proof = verify_legacy_admission(evidence(), CanonicalTime(1), &request, 1).unwrap();
    assert_eq!(proof.admission().capability, None);
    assert_eq!(
        proof.admission().declarations.origin,
        OriginDeclaration {
            mode: OriginMode::LegacyOrUnspecified,
            authority: DeclarationAuthority::LegacyGateway(ProviderRef(30))
        }
    );
    request.sender = ProtocolIdentity(999);
    assert!(verify_legacy_admission(evidence(), CanonicalTime(1), &request, 1).is_err());
    request.sender_domain = DomainIdentity::parse_ascii("another.example").unwrap();
    request.sender = request
        .sender_domain
        .synthetic_protocol_identity(&[7; 32], 1);
    assert!(verify_legacy_admission(evidence(), CanonicalTime(1), &request, 1).is_err());
}
