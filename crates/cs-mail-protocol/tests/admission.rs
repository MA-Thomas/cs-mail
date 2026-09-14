use cs_mail_capabilities::{AdmissionAuthentication, BondFreeAdmission};
use cs_mail_content::{
    ContentBinding, ContentCertificateDigest, EncryptedContentRecord, EndpointSecretKey, encrypt,
    message_declaration_digest,
};
use cs_mail_primitives::*;
use cs_mail_protocol::admission::*;
use cs_mail_protocol::{AdmissionBasis, Message, ProtocolError, ProtocolState, RelationshipState};

fn fixture() -> (ProtocolState, BondFreeAdmission, EncryptedContentRecord) {
    let state = ProtocolState::initial_scoped(
        RelationshipRef::from_u128_for_test(1),
        RequestHistoryRef::from_u128_for_test(2),
        ProtocolIdentity(10),
        ProtocolIdentity(20),
        CanonicalTime(0),
    );
    let declarations = MessageDeclarations {
        purpose: DeclaredPurpose::Known(KnownPurpose::Transactional),
        origin: OriginDeclaration {
            mode: OriginMode::HumanInitiated,
            authority: DeclarationAuthority::NativeSender(OperationalKeyRef(1)),
        },
        payload_schema: Some(PayloadSchema {
            id: NamespacedIdentifier::new("com.example", "invoice", 1).unwrap(),
            criticality: ExtensionCriticality::Critical,
        }),
    };
    let admission = BondFreeAdmission {
        wire_version: WireVersion(1),
        sender: state.relationship.key.sender,
        recipient: state.relationship.key.recipient,
        message_id: MessageId(1),
        content_ref: ContentRef(1),
        delivery_intent_ref: DeliveryIntentRef(1),
        declarations: declarations.clone(),
        message_valid_until: MessageValidityUntil(CanonicalTime(50)),
        capability: None,
        evidence: None,
        idempotency_key: IdempotencyKey(1),
        protocol_version: ProtocolVersion(2),
        deployment_domain: [7; 32],
        intended_provider: ProviderRef(30),
        authentication: AdmissionAuthentication::NativeKey(OperationalKeyRef(1)),
    };
    let (sender, _) = EndpointSecretKey::generate(ContentKeyRef(1));
    let (_, recipient) = EndpointSecretKey::generate(ContentKeyRef(2));
    let record = encrypt(
        &sender,
        recipient,
        ContentBinding {
            wire_version: WireVersion(1),
            content_ref: admission.content_ref,
            message_id: admission.message_id,
            sender: admission.sender,
            recipient: admission.recipient,
            protocol_version: admission.protocol_version,
            relationship: state.relationship.key.reference,
            content_scope: ContentScopeRef::from_u128_for_test(1),
            sender_certificate: ContentCertificateDigest([1; 32]),
            declarations,
            message_valid_until: admission.message_valid_until,
            capability: None,
        },
        b"encrypted",
        CanonicalTime(1),
        CanonicalTime(100),
    )
    .unwrap();
    (state, admission, record)
}
fn supported() -> AdmissionPolicy {
    AdmissionPolicy {
        version: Version(1),
        supported_critical_schemas: std::collections::BTreeSet::from([NamespacedIdentifier::new(
            "com.example",
            "invoice",
            1,
        )
        .unwrap()]),
    }
}
fn context(state: &ProtocolState) -> AdmissionContext {
    AdmissionContext {
        relationship: state.relationship.key,
        protocol: ProtocolVersion(2),
        now: CanonicalTime(2),
        authority: DeclarationAuthority::NativeSender(OperationalKeyRef(1)),
        capability: None,
    }
}
#[test]
fn all_four_admission_bases_recheck_current_policy_and_content() {
    let (state, request, record) = fixture();
    for basis in [
        AdmissionBasis::InitialRequest {
            request: RequestId(1),
        },
        AdmissionBasis::RequestFollowup {
            request: RequestId(1),
            policy_version: Version(1),
        },
        AdmissionBasis::AcceptedRelationship {
            version: Version(1).into(),
        },
        AdmissionBasis::ExpressLane {
            lane: LaneId(1),
            version: Version(1),
        },
    ] {
        let message = Message {
            id: request.message_id,
            relationship: state.relationship.key.reference,
            content: request.content_ref,
            delivery: request.delivery_intent_ref,
            declaration: message_declaration_digest(&request.declarations).unwrap(),
            valid_until: request.message_valid_until,
            admitted_at: CanonicalTime(2),
            basis,
        };
        assert_eq!(
            validate_message(&record, &message, context(&state), &supported()),
            Ok(())
        );
        assert_eq!(
            validate_message(
                &record,
                &message,
                context(&state),
                &AdmissionPolicy::default()
            ),
            Err(AdmissionFailure::UnsupportedCriticalExtension)
        );
        let mut wrong = context(&state);
        wrong.capability = Some(LaneId(9));
        assert_eq!(
            validate_message(&record, &message, wrong, &supported()),
            Err(AdmissionFailure::AuthenticationMismatch)
        );
        wrong = context(&state);
        wrong.authority = DeclarationAuthority::LegacyGateway(ProviderRef(30));
        assert_eq!(
            validate_message(&record, &message, wrong, &supported()),
            Err(AdmissionFailure::AuthenticationMismatch)
        );
        wrong = context(&state);
        wrong.now = CanonicalTime(51);
        assert_eq!(
            validate_message(&record, &message, wrong, &supported()),
            Err(AdmissionFailure::MessageValidityClosed)
        );
        wrong = context(&state);
        wrong.now = CanonicalTime(100);
        assert_eq!(
            validate_message(&record, &message, wrong, &supported()),
            Err(AdmissionFailure::ContentExpired)
        );
    }
}
#[test]
fn accepted_permission_precedes_an_attached_capability_and_block_precedes_both() {
    let (mut state, mut request, mut record) = fixture();
    state.relationship.state = RelationshipState::Accepted;
    request.capability = Some(LaneId(999));
    record.binding.capability = request.capability;
    let plan = plan_free_admission(
        &state,
        &request,
        Some(&record),
        None,
        CanonicalTime(2),
        &supported(),
    )
    .unwrap();
    assert!(matches!(
        plan.message.basis,
        AdmissionBasis::AcceptedRelationship { .. }
    ));
    assert!(plan.next_lane.is_none());
    state.relationship.state = RelationshipState::Blocked;
    assert!(matches!(
        plan_free_admission(&state, &request, None, None, CanonicalTime(2), &supported()),
        Err(AdmissionPlanError::Protocol(ProtocolError::ContactBlocked))
    ));
}
#[test]
fn missing_free_permission_never_creates_a_request_or_financial_effect() {
    let (state, request, record) = fixture();
    assert!(
        plan_free_admission(
            &state,
            &request,
            Some(&record),
            None,
            CanonicalTime(2),
            &supported()
        )
        .is_err()
    );
    assert!(state.requests.is_empty());
}
