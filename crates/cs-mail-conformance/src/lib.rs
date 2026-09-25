//! Executable cross-crate conformance checks for normative protocol boundaries.
//!
//! Domain-specific versions cannot be compared without an explicit conversion:
//!
//! ```compile_fail
//! use cs_mail_primitives::{RequestHistoryVersion, RelationshipVersion, Version};
//!
//! let relationship = RelationshipVersion::from(Version(3));
//! let attempt = RequestHistoryVersion::from(Version(3));
//! assert_eq!(relationship, attempt);
//! ```

#[cfg(test)]
mod tests {
    use cs_mail_application::{EngineError, InMemoryEngine};
    use cs_mail_content::{ContentKeyCertificate, EndpointSecretKey};
    use cs_mail_finance::PaymentProcessor;
    use cs_mail_primitives::{
        CanonicalTime, ContentKeyRef, ContentKeyVersion, ContentRef, DeliveryIntentRef, Duration,
        IdempotencyKey, JournalPosition, MessageDeclarationDigest, MessageId, MessageValidityUntil,
        Money, OperationalKeyRef, PolicyVersion, PrincipalRef, PrivacyProfileVersion,
        ProtocolIdentity, ProtocolVersion, ProviderRef, QuoteId, ReceiptRef, RelationshipRef,
        RequestHistoryRef, RequestId, RetentionPolicyVersion, SettlementUnit, Version, WireVersion,
    };
    use cs_mail_protocol::{
        ActorRef, KernelCommand, PolicySnapshot, ProtocolCommand, ProtocolError, ProtocolState,
        TermsOutcome,
    };
    use cs_mail_security::{
        CommandDigest, CommandSigner, KeyRegistry, OutcomeDigest, ProviderSigner, ReceiptKind,
        ReceiptPayload, SecurityError, SigningScope,
    };

    const SENDER: ProtocolIdentity = ProtocolIdentity(10);
    const RECIPIENT: ProtocolIdentity = ProtocolIdentity(20);
    const PROVIDER: ProviderRef = ProviderRef(30);

    fn policy() -> PolicySnapshot {
        PolicySnapshot {
            selected_class: Some(
                cs_mail_protocol::pricing::SelectedRequestClass::new(
                    cs_mail_primitives::RequestClassId(1),
                    "Test class".into(),
                )
                .unwrap(),
            ),
            pricing_policy_version: cs_mail_primitives::PolicyVersion(1),
            protocol_version: ProtocolVersion(2),
            policy_version: PolicyVersion(1),
            privacy_profile_version: PrivacyProfileVersion(1),
            retention_policy_version: RetentionPolicyVersion(1),
            recipient_provider: PROVIDER,
            unit: SettlementUnit(1),
            processing_charge: Money::from_minor_units(2),
            collateral: Money::from_minor_units(8),
            submission_window: Duration(10),
            decision_window: Duration(20),
            quote_lifetime: Duration(20),
            backoff: vec![Duration(0), Duration(5)],
            financial: cs_mail_finance::FinancialTerms {
                scope: cs_mail_finance::FinancialScope::new(
                    [7; 32],
                    cs_mail_primitives::ProviderRef(30),
                    cs_mail_primitives::ProgramRef(1),
                    [9; 32],
                    cs_mail_primitives::ProtocolVersion(2),
                ),
                policy_version: PolicyVersion(1),
                corporate_basis_points: 300,
                maturity_delay: Duration(10),
            },
            payment_provider_key: cs_mail_finance::SimulatedProcessor::new([7; 32]).verifying_key(),
            expiry_cooldown: Duration(30),
            rejection_cooldown: Duration(90),
        }
    }

    fn sender(command: ProtocolCommand, key: u128) -> KernelCommand<ProtocolCommand> {
        sender_as(SENDER, command, key)
    }

    fn sender_as(
        identity: ProtocolIdentity,
        command: ProtocolCommand,
        key: u128,
    ) -> KernelCommand<ProtocolCommand> {
        KernelCommand::new(
            command,
            ActorRef::Sender(identity),
            OperationalKeyRef(1),
            IdempotencyKey(key),
        )
    }

    #[test]
    fn signed_commands_cannot_cross_relationship_targets() {
        let signer = CommandSigner::from_secret_bytes(
            ActorRef::Sender(SENDER),
            OperationalKeyRef(1),
            &[7; 32],
        );
        let mut registry = KeyRegistry::default();
        registry
            .register(
                OperationalKeyRef(1),
                ActorRef::Sender(SENDER),
                signer.verifying_key_bytes(),
                CanonicalTime(0),
            )
            .unwrap();
        let relationship = RelationshipRef::from_u128_for_test(1);
        let signed_command = signer
            .sign(
                SigningScope {
                    deployment_domain: [9; 32],
                    intended_provider: PROVIDER,
                    relationship,
                },
                ProtocolVersion(2),
                IdempotencyKey(1),
                ProtocolCommand::IssueRequestTerms {
                    class_id: cs_mail_primitives::RequestClassId(1),
                    quote_id: QuoteId(1),
                    declaration_digest: None,
                },
            )
            .unwrap();
        assert_eq!(
            registry.verify(
                &signed_command,
                CanonicalTime(1),
                ProtocolVersion(2),
                SigningScope {
                    deployment_domain: [9; 32],
                    intended_provider: PROVIDER,
                    relationship: RelationshipRef::from_u128_for_test(2),
                },
            ),
            Err(SecurityError::SigningScopeMismatch)
        );
    }

    #[test]
    fn issued_quote_remains_fixed_when_current_policy_changes() {
        let state = ProtocolState::initial(PrincipalRef(1), SENDER, RECIPIENT, CanonicalTime(0));
        let engine = InMemoryEngine::new(
            state,
            SettlementUnit(1),
            cs_mail_finance::FinancialScope::new(
                [7; 32],
                cs_mail_primitives::ProviderRef(30),
                cs_mail_primitives::ProgramRef(1),
                [9; 32],
                cs_mail_primitives::ProtocolVersion(2),
            ),
        )
        .unwrap();
        let issued = engine
            .execute(
                sender(
                    ProtocolCommand::IssueRequestTerms {
                        class_id: cs_mail_primitives::RequestClassId(1),
                        quote_id: QuoteId(1),
                        declaration_digest: None,
                    },
                    1,
                ),
                CanonicalTime(1),
                policy(),
            )
            .unwrap();
        let Some(TermsOutcome::ChargeRequired(terms)) = issued.transition.terms_outcome else {
            panic!("unknown relationship must receive bond terms");
        };
        let mut changed = policy();
        changed.policy_version = PolicyVersion(2);
        changed.processing_charge = Money::from_minor_units(200);
        changed.collateral = Money::from_minor_units(800);
        changed.submission_window = Duration(2);
        engine
            .execute(
                sender(
                    ProtocolCommand::CreateRequest {
                        request_id: RequestId(1),

                        message_id: MessageId(1),
                        terms,
                        payment_method: [9; 32],
                    },
                    2,
                ),
                CanonicalTime(2),
                changed,
            )
            .unwrap();
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn request_history_survives_public_identity_rotation() {
        let old_sender = ProtocolIdentity(10);
        let new_sender = ProtocolIdentity(11);
        let request_history = RequestHistoryRef::from_u128_for_test(99);
        let old_relationship = RelationshipRef::from_u128_for_test(1);
        let new_relationship = RelationshipRef::from_u128_for_test(2);
        let first_state = ProtocolState::initial_scoped(
            old_relationship,
            request_history,
            old_sender,
            RECIPIENT,
            CanonicalTime(0),
        );
        let store = cs_mail_application::InMemoryStore::new(cs_mail_finance::FinancialScope::new(
            [7; 32],
            cs_mail_primitives::ProviderRef(30),
            cs_mail_primitives::ProgramRef(1),
            [9; 32],
            cs_mail_primitives::ProtocolVersion(2),
        ));
        let first = store.register(first_state, SettlementUnit(1)).unwrap();
        let issued = first
            .execute(
                sender_as(
                    old_sender,
                    ProtocolCommand::IssueRequestTerms {
                        class_id: cs_mail_primitives::RequestClassId(1),
                        quote_id: QuoteId(1),
                        declaration_digest: None,
                    },
                    1,
                ),
                CanonicalTime(1),
                policy(),
            )
            .unwrap();
        let Some(TermsOutcome::ChargeRequired(terms)) = issued.transition.terms_outcome else {
            panic!("unknown relationship must receive bond terms");
        };
        first
            .execute(
                sender_as(
                    old_sender,
                    ProtocolCommand::CreateRequest {
                        request_id: RequestId(1),

                        message_id: MessageId(1),
                        terms,
                        payment_method: [9; 32],
                    },
                    2,
                ),
                CanonicalTime(2),
                policy(),
            )
            .unwrap();
        let mut provider = cs_mail_finance::SimulatedProcessor::new([7; 32]);
        let op = first.snapshot().unwrap().payments[&RequestId(1)]
            .capture()
            .clone();
        let receipt = provider.submit(&op).unwrap();
        first
            .execute(
                KernelCommand::new(
                    ProtocolCommand::RecordPayment {
                        request_id: RequestId(1),
                        receipt,
                    },
                    ActorRef::Provider(PROVIDER),
                    OperationalKeyRef(9),
                    IdempotencyKey(900),
                ),
                CanonicalTime(2),
                policy(),
            )
            .unwrap();
        first
            .execute(
                sender_as(
                    old_sender,
                    ProtocolCommand::SubmitRequestToRecipient {
                        request_id: RequestId(1),
                        expected_request_version: Version(0),
                        content_ref: ContentRef(1),
                        delivery_intent_ref: DeliveryIntentRef(1),
                        declaration_digest: MessageDeclarationDigest([0; 32]),
                        message_valid_until: MessageValidityUntil(CanonicalTime(100)),
                    },
                    3,
                ),
                CanonicalTime(3),
                policy(),
            )
            .unwrap();
        let first_snapshot = first.snapshot().unwrap();
        assert_eq!(first_snapshot.history.level, 1);
        assert_eq!(
            first_snapshot.history.earliest_next_submission,
            CanonicalTime(8)
        );

        let rotated_state = ProtocolState::initial_scoped(
            new_relationship,
            request_history,
            new_sender,
            RECIPIENT,
            CanonicalTime(4),
        );
        let rotated = store.register(rotated_state, SettlementUnit(1)).unwrap();
        let denied = rotated.execute(
            sender_as(
                new_sender,
                ProtocolCommand::IssueRequestTerms {
                    class_id: cs_mail_primitives::RequestClassId(1),
                    quote_id: QuoteId(2),
                    declaration_digest: None,
                },
                4,
            ),
            CanonicalTime(4),
            policy(),
        );
        assert_eq!(
            denied,
            Err(EngineError::Protocol(ProtocolError::BackoffActive {
                next_eligible: CanonicalTime(8)
            }))
        );
        let issued = rotated
            .execute(
                sender_as(
                    new_sender,
                    ProtocolCommand::IssueRequestTerms {
                        class_id: cs_mail_primitives::RequestClassId(1),
                        quote_id: QuoteId(2),
                        declaration_digest: None,
                    },
                    5,
                ),
                CanonicalTime(8),
                policy(),
            )
            .unwrap();
        let Some(TermsOutcome::ChargeRequired(terms)) = issued.transition.terms_outcome else {
            panic!("eligible new identity needs terms")
        };
        assert_eq!(terms.relationship, new_relationship);
        assert_eq!(terms.sender, new_sender);
        assert_eq!(terms.request_history, request_history);
        assert_eq!(terms.request_level, 1);
        rotated
            .execute(
                sender_as(
                    new_sender,
                    ProtocolCommand::CreateRequest {
                        request_id: RequestId(2),

                        message_id: MessageId(2),
                        payment_method: [9; 32],
                        terms,
                    },
                    6,
                ),
                CanonicalTime(8),
                policy(),
            )
            .unwrap();
        let op = rotated.snapshot().unwrap().payments[&RequestId(2)]
            .capture()
            .clone();
        let receipt = provider.submit(&op).unwrap();
        rotated
            .execute(
                KernelCommand::new(
                    ProtocolCommand::RecordPayment {
                        request_id: RequestId(2),
                        receipt,
                    },
                    ActorRef::Provider(PROVIDER),
                    OperationalKeyRef(9),
                    IdempotencyKey(901),
                ),
                CanonicalTime(8),
                policy(),
            )
            .unwrap();
        rotated
            .execute(
                sender_as(
                    new_sender,
                    ProtocolCommand::SubmitRequestToRecipient {
                        request_id: RequestId(2),
                        expected_request_version: Version(0),
                        content_ref: ContentRef(2),
                        delivery_intent_ref: DeliveryIntentRef(2),
                        declaration_digest: MessageDeclarationDigest([0; 32]),
                        message_valid_until: MessageValidityUntil(CanonicalTime(100)),
                    },
                    7,
                ),
                CanonicalTime(8),
                policy(),
            )
            .unwrap();
        assert_eq!(rotated.snapshot().unwrap().history.level, 2);
    }

    #[test]
    fn signed_receipt_commits_command_position_and_outcome() {
        let signer = ProviderSigner::from_secret_bytes(PROVIDER, OperationalKeyRef(9), &[8; 32]);
        let payload = ReceiptPayload {
            deployment_domain: [7; 32],
            receipt_id: ReceiptRef(1),
            kind: ReceiptKind::ReservationCommitted,
            relationship: RelationshipRef::from_u128_for_test(1),
            command_digest: CommandDigest([2; 32]),
            journal_position: JournalPosition(3),
            received_at: CanonicalTime(4),
            outcome_digest: OutcomeDigest([5; 32]),
            provider: PROVIDER,
            protocol_version: ProtocolVersion(2),
        };
        let mut receipt = signer.sign_receipt(payload).unwrap();
        receipt.verify(&signer.verifying_key_bytes()).unwrap();
        receipt.payload.outcome_digest = OutcomeDigest([6; 32]);
        assert_eq!(
            receipt.verify(&signer.verifying_key_bytes()),
            Err(SecurityError::InvalidSignature)
        );
    }

    #[test]
    fn content_key_certificate_is_operationally_bound() {
        let signer = CommandSigner::from_secret_bytes(
            ActorRef::Sender(SENDER),
            OperationalKeyRef(1),
            &[7; 32],
        );
        let mut registry = KeyRegistry::default();
        registry
            .register(
                OperationalKeyRef(1),
                ActorRef::Sender(SENDER),
                signer.verifying_key_bytes(),
                CanonicalTime(0),
            )
            .unwrap();
        let (_, content_key) = EndpointSecretKey::generate(ContentKeyRef(1));
        let relationship = RelationshipRef::from_u128_for_test(1);
        let certificate = signer
            .sign_content_key_certificate(ContentKeyCertificate {
                wire_version: WireVersion(1),
                protocol_version: ProtocolVersion(2),
                deployment_domain: [9; 32],
                intended_provider: PROVIDER,
                relationship,
                owner: SENDER,
                operational_key: OperationalKeyRef(1),
                content_key_version: ContentKeyVersion(1),
                key: content_key,
                valid_from: CanonicalTime(1),
                valid_until: CanonicalTime(10),
            })
            .unwrap();
        registry
            .verify_content_key_certificate(
                &certificate,
                CanonicalTime(2),
                ProtocolVersion(2),
                SigningScope {
                    deployment_domain: [9; 32],
                    intended_provider: PROVIDER,
                    relationship,
                },
            )
            .unwrap();
        assert_eq!(
            registry.verify_content_key_certificate(
                &certificate,
                CanonicalTime(2),
                ProtocolVersion(2),
                SigningScope {
                    deployment_domain: [9; 32],
                    intended_provider: PROVIDER,
                    relationship: RelationshipRef::from_u128_for_test(2),
                },
            ),
            Err(SecurityError::SigningScopeMismatch)
        );
    }
}
