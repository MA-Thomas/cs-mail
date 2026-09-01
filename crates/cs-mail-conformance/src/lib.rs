//! Executable cross-crate conformance checks for normative protocol boundaries.
//!
//! Domain-specific versions cannot be compared without an explicit conversion:
//!
//! ```compile_fail
//! use cs_mail_primitives::{AttemptVersion, RelationshipVersion, Version};
//!
//! let relationship = RelationshipVersion::from(Version(3));
//! let attempt = AttemptVersion::from(Version(3));
//! assert_eq!(relationship, attempt);
//! ```

#[cfg(test)]
mod tests {
    use cs_mail_application::{EngineError, InMemoryEngine};
    use cs_mail_content::{ContentKeyCertificate, EndpointSecretKey};
    use cs_mail_primitives::{
        AttemptId, AttemptSubjectRef, BondId, CanonicalTime, ContentKeyRef, ContentKeyVersion,
        ContentRef, DeliveryIntentRef, Duration, IdempotencyKey, JournalPosition, LedgerAccountRef,
        MessageDeclarationDigest, MessageId, MessageValidityUntil, Money, OperationalKeyRef,
        PersistenceReserveId, PolicyVersion, PrincipalRef, PrivacyProfileVersion, ProtocolIdentity,
        ProtocolVersion, ProviderRef, QuoteId, ReceiptRef, RelationshipRef, RetentionPolicyVersion,
        SettlementUnit, Version, WireVersion,
    };
    use cs_mail_protocol::{
        ActorRef, Authorized, PolicySnapshot, ProtocolCommand, ProtocolError, ProtocolState,
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
            protocol_version: ProtocolVersion(1),
            policy_version: PolicyVersion(1),
            privacy_profile_version: PrivacyProfileVersion(1),
            retention_policy_version: RetentionPolicyVersion(1),
            recipient_provider: PROVIDER,
            unit: SettlementUnit(1),
            processing_charge: Money::from_minor_units(2),
            collateral: Money::from_minor_units(8),
            admission_window: Duration(10),
            decision_window: Duration(20),
            quote_lifetime: Duration(20),
            persistence_duration: Duration(100),
            backoff: vec![Duration(0), Duration(5)],
            persistence: vec![Money::ZERO, Money::from_minor_units(5)],
        }
    }

    fn sender(command: ProtocolCommand, key: u128) -> Authorized<ProtocolCommand> {
        sender_as(SENDER, command, key)
    }

    fn sender_as(
        identity: ProtocolIdentity,
        command: ProtocolCommand,
        key: u128,
    ) -> Authorized<ProtocolCommand> {
        Authorized::assume_verified(
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
                ProtocolVersion(1),
                IdempotencyKey(1),
                ProtocolCommand::IssueContactTerms {
                    quote_id: QuoteId(1),
                    declaration_digest: None,
                },
            )
            .unwrap();
        assert_eq!(
            registry.verify(
                &signed_command,
                CanonicalTime(1),
                ProtocolVersion(1),
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
        let engine =
            InMemoryEngine::new(state, SettlementUnit(1), Money::from_minor_units(1_000)).unwrap();
        let issued = engine
            .execute(
                sender(
                    ProtocolCommand::IssueContactTerms {
                        quote_id: QuoteId(1),
                        declaration_digest: None,
                    },
                    1,
                ),
                CanonicalTime(1),
                policy(),
            )
            .unwrap();
        let Some(TermsOutcome::BondRequired(terms)) = issued.manifest.terms_outcome else {
            panic!("unknown relationship must receive bond terms");
        };
        let mut changed = policy();
        changed.policy_version = PolicyVersion(2);
        changed.processing_charge = Money::from_minor_units(200);
        changed.collateral = Money::from_minor_units(800);
        changed.admission_window = Duration(2);
        engine
            .execute(
                sender(
                    ProtocolCommand::ReserveAttempt {
                        bond_id: BondId(1),
                        reserve_id: PersistenceReserveId(1),
                        attempt_id: AttemptId(1),
                        message_id: MessageId(1),
                        terms,
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
    fn attempt_history_survives_public_identity_rotation() {
        let old_sender = ProtocolIdentity(10);
        let new_sender = ProtocolIdentity(11);
        let attempt_subject = AttemptSubjectRef::from_u128_for_test(99);
        let sender_account = LedgerAccountRef::from_u128_for_test(99);
        let old_relationship = RelationshipRef::from_u128_for_test(1);
        let new_relationship = RelationshipRef::from_u128_for_test(2);
        let first_state = ProtocolState::initial_scoped(
            old_relationship,
            attempt_subject,
            sender_account,
            old_sender,
            RECIPIENT,
            CanonicalTime(0),
        );
        let first = InMemoryEngine::new(
            first_state,
            SettlementUnit(1),
            Money::from_minor_units(1_000),
        )
        .unwrap();
        let issued = first
            .execute(
                sender_as(
                    old_sender,
                    ProtocolCommand::IssueContactTerms {
                        quote_id: QuoteId(1),
                        declaration_digest: None,
                    },
                    1,
                ),
                CanonicalTime(1),
                policy(),
            )
            .unwrap();
        let Some(TermsOutcome::BondRequired(terms)) = issued.manifest.terms_outcome else {
            panic!("unknown relationship must receive bond terms");
        };
        first
            .execute(
                sender_as(
                    old_sender,
                    ProtocolCommand::ReserveAttempt {
                        bond_id: BondId(1),
                        reserve_id: PersistenceReserveId(1),
                        attempt_id: AttemptId(1),
                        message_id: MessageId(1),
                        terms,
                    },
                    2,
                ),
                CanonicalTime(2),
                policy(),
            )
            .unwrap();
        first
            .execute(
                sender_as(
                    old_sender,
                    ProtocolCommand::AdmitAttempt {
                        bond_id: BondId(1),
                        expected_bond_version: Version(0),
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
        assert_eq!(first_snapshot.state.attempt.level, 1);
        assert_eq!(
            first_snapshot.state.attempt.earliest_next_admission,
            CanonicalTime(8)
        );

        let mut rotated_state = ProtocolState::initial_scoped(
            new_relationship,
            attempt_subject,
            sender_account,
            new_sender,
            RECIPIENT,
            CanonicalTime(4),
        );
        rotated_state.attempt = first_snapshot.state.attempt;
        let rotated = InMemoryEngine::new(
            rotated_state,
            SettlementUnit(1),
            Money::from_minor_units(1_000),
        )
        .unwrap();
        let issued = rotated
            .execute(
                sender_as(
                    new_sender,
                    ProtocolCommand::IssueContactTerms {
                        quote_id: QuoteId(2),
                        declaration_digest: None,
                    },
                    4,
                ),
                CanonicalTime(4),
                policy(),
            )
            .unwrap();
        let Some(TermsOutcome::BondRequired(terms)) = issued.manifest.terms_outcome else {
            panic!("rotated identity must receive bonded terms");
        };
        assert_eq!(terms.relationship, new_relationship);
        assert_eq!(terms.sender, new_sender);
        assert_eq!(terms.attempt_subject, attempt_subject);
        assert_eq!(terms.attempt_level, 1);
        assert_eq!(terms.persistence, Money::from_minor_units(5));
        assert_eq!(terms.eligibility_time, CanonicalTime(8));
        rotated
            .execute(
                sender_as(
                    new_sender,
                    ProtocolCommand::ReserveAttempt {
                        bond_id: BondId(2),
                        reserve_id: PersistenceReserveId(2),
                        attempt_id: AttemptId(2),
                        message_id: MessageId(2),
                        terms,
                    },
                    5,
                ),
                CanonicalTime(5),
                policy(),
            )
            .unwrap();
        let admission = sender_as(
            new_sender,
            ProtocolCommand::AdmitAttempt {
                bond_id: BondId(2),
                expected_bond_version: Version(0),
                content_ref: ContentRef(2),
                delivery_intent_ref: DeliveryIntentRef(2),
                declaration_digest: MessageDeclarationDigest([0; 32]),
                message_valid_until: MessageValidityUntil(CanonicalTime(100)),
            },
            6,
        );
        assert_eq!(
            rotated.execute(admission.clone(), CanonicalTime(7), policy()),
            Err(EngineError::Protocol(ProtocolError::BackoffActive {
                next_eligible: CanonicalTime(8),
            }))
        );
        rotated
            .execute(admission, CanonicalTime(8), policy())
            .unwrap();
        assert_eq!(rotated.snapshot().unwrap().state.attempt.level, 2);
    }

    #[test]
    fn signed_receipt_commits_command_position_and_outcome() {
        let signer = ProviderSigner::from_secret_bytes(PROVIDER, OperationalKeyRef(9), &[8; 32]);
        let payload = ReceiptPayload {
            receipt_id: ReceiptRef(1),
            kind: ReceiptKind::ReservationCommitted,
            relationship: RelationshipRef::from_u128_for_test(1),
            command_digest: CommandDigest([2; 32]),
            journal_position: JournalPosition(3),
            received_at: CanonicalTime(4),
            outcome_digest: OutcomeDigest([5; 32]),
            provider: PROVIDER,
            protocol_version: ProtocolVersion(1),
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
                protocol_version: ProtocolVersion(1),
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
                ProtocolVersion(1),
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
                ProtocolVersion(1),
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
