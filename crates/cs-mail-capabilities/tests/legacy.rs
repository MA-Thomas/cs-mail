use cs_mail_adapters::{
    DmarcAlignment, DmarcPass, DmarcVerificationFuture, DmarcVerifier, DomainIdentity,
    SmtpAuthenticationRequest, VerifiedDomain,
};
use cs_mail_capabilities::{LegacyBondFreeAdmission, verify_legacy_admission};
use cs_mail_primitives::*;
use std::future::Future;
use std::task::{Context, Poll, Waker};

struct Gateway;
impl DmarcVerifier for Gateway {
    fn verify<'a>(&'a self, request: SmtpAuthenticationRequest<'a>) -> DmarcVerificationFuture<'a> {
        Box::pin(async move {
            Ok(VerifiedDomain::passed(
                DomainIdentity::parse_ascii("alerts.example.com").unwrap(),
                Some(DmarcPass {
                    alignment: DmarcAlignment::Relaxed,
                }),
                None,
                request.received_at,
            )
            .unwrap())
        })
    }
}
fn ready<F: Future>(future: F) -> F::Output {
    match Box::pin(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("test verifier must finish immediately"),
    }
}
fn authentication() -> SmtpAuthenticationRequest<'static> {
    SmtpAuthenticationRequest {
        raw_message: b"From: alerts@alerts.example.com\r\n\r\ninvoice",
        remote_ip: "192.0.2.1".parse().unwrap(),
        helo_identity: "alerts.example.com",
        mail_from: Some("alerts@alerts.example.com"),
        receiver_hostname: "mail.recipient.example",
        received_at: CanonicalTime(1),
    }
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
    let proof = ready(verify_legacy_admission(
        &Gateway,
        authentication(),
        &request,
        1,
    ))
    .unwrap();
    assert_eq!(proof.admission().capability, None);
    assert_eq!(
        proof.admission().declarations.origin,
        OriginDeclaration {
            mode: OriginMode::LegacyOrUnspecified,
            authority: DeclarationAuthority::LegacyGateway(ProviderRef(30))
        }
    );
    request.sender = ProtocolIdentity(999);
    assert!(
        ready(verify_legacy_admission(
            &Gateway,
            authentication(),
            &request,
            1
        ))
        .is_err()
    );
    request.sender_domain = DomainIdentity::parse_ascii("another.example").unwrap();
    request.sender = request
        .sender_domain
        .synthetic_protocol_identity(&[7; 32], 1);
    assert!(
        ready(verify_legacy_admission(
            &Gateway,
            authentication(),
            &request,
            1
        ))
        .is_err()
    );
}
