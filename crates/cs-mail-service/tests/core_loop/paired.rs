//! Claims: the first submitted request cancels opposite preparation atomically,
//! and a pending opposite request refuses new payment work until its issued deadline.
//! Partial cancellation, retained captured money or a needless second capture would
//! falsify the claims and violate the sender's request agreement.
use super::*;
const ALICE_INBOX_KEY: OperationalKeyRef = OperationalKeyRef(13);
const BOB_SENDER_KEY: OperationalKeyRef = OperationalKeyRef(23);
impl Loop {
    fn register_key(
        &self,
        persona: ProtocolIdentity,
        (manager, manager_secret): (OperationalKeyRef, u8),
        (reference, actor, secret): (OperationalKeyRef, ActorRef, u8),
        idempotency: u128,
    ) {
        let clock = self.clock.clone();
        let accounts = self.gateway.deployment().accounts(move || clock.now());
        let account = accounts.persona_account(persona).unwrap();
        let now = || self.clock.now();
        cs_mail_application::accounts::operations::AccountService::new(&accounts, &now)
            .execute_command(
                &cs_mail_accounts::control::SignedAccountCommand {
                    account,
                    operational_key: manager,
                    expected_revision: accounts.account_control(account).unwrap().revision(),
                    idempotency_key: IdempotencyKey(idempotency),
                    product: "cs-mail/test".into(),
                    command: cs_mail_accounts::control::AccountCommand::RegisterKey(
                        cs_mail_accounts::control::NewOperationalKey::prove(
                            account,
                            "cs-mail/test",
                            reference,
                            actor,
                            &[secret; 32],
                        )
                        .unwrap(),
                    ),
                    signature: vec![],
                }
                .sign(&[manager_secret; 32])
                .unwrap(),
            )
            .unwrap();
    }

    fn both_directions(&self) {
        self.register_key(
            ALICE,
            (ALICE_KEY, 11),
            (ALICE_INBOX_KEY, ActorRef::Recipient(ALICE), 13),
            9001,
        );
        self.register_key(
            BOB,
            (BOB_KEY, 21),
            (BOB_SENDER_KEY, ActorRef::Sender(BOB), 23),
            9002,
        );
        let signer = CommandSigner::from_secret_bytes(
            ActorRef::Recipient(ALICE),
            ALICE_INBOX_KEY,
            &[13; 32],
        );
        self.gateway
            .publish_classes(
                &signer
                    .sign_request_classes(
                        self.gateway.scope(),
                        cs_mail_protocol::pricing::RecipientRequestClasses::new(
                            ALICE,
                            cs_mail_primitives::AliasId::new((ALICE).0).unwrap(),
                            1,
                            vec![
                                cs_mail_protocol::pricing::RequestClass::new(
                                    RequestClassId(1),
                                    "Introductions".into(),
                                    Money::from_minor_units(support::FIXTURE_COLLATERAL),
                                )
                                .unwrap(),
                            ],
                        )
                        .unwrap(),
                    )
                    .unwrap(),
            )
            .unwrap();
    }
    fn reverse_command(&self, operation: u128, command: ProtocolCommand) -> SignedCommandBytes {
        self.sign_reverse(
            &CommandSigner::from_secret_bytes(ActorRef::Sender(BOB), BOB_SENDER_KEY, &[23; 32]),
            operation,
            command,
        )
    }
    fn sign_reverse(
        &self,
        signer: &CommandSigner,
        operation: u128,
        command: ProtocolCommand,
    ) -> SignedCommandBytes {
        let handle = self.gateway.address(BOB, ALICE).unwrap().public();
        signer
            .sign(
                SigningScope {
                    deployment_domain: [7; 32],
                    intended_provider: ProviderRef(30),
                    relationship: handle.relationship,
                },
                ProtocolVersion(2),
                IdempotencyKey(operation),
                command,
            )
            .unwrap()
    }
    fn reverse_submit(&self, command: &SignedCommandBytes) -> ServiceOutcome {
        self.advance(1);
        self.gateway.submit(BOB, ALICE, command).unwrap()
    }
    fn reverse_snapshot(&self) -> SettlementSnapshot {
        let handle = self.gateway.address(BOB, ALICE).unwrap();
        self.gateway
            .deployment()
            .existing_relationship(handle.aggregate_key())
            .unwrap()
            .unwrap()
            .snapshot()
            .unwrap()
    }
    fn reverse_terms(&self, operation: u128) -> ServiceOutcome {
        self.reverse_submit(&self.reverse_command(
            operation,
            ProtocolCommand::IssueRequestTerms {
                return_route: cs_mail_protocol::RequestReturnRoute::Native(
                    cs_mail_primitives::AliasId::new(BOB.0).unwrap(),
                ),
                alias: cs_mail_primitives::AliasId::new(ALICE.0).unwrap(),
                class_id: RequestClassId(1),
                quote_id: QuoteId(operation),
                declaration_digest: None,
            },
        ))
    }
    fn reverse_upload(&self) -> MessageValidityUntil {
        let handle = self.gateway.address(BOB, ALICE).unwrap().public();
        let now = self.clock.now();
        let valid_until = MessageValidityUntil(CanonicalTime(now.0 + 3_600_000));
        let signer =
            CommandSigner::from_secret_bytes(ActorRef::Sender(BOB), BOB_SENDER_KEY, &[23; 32]);
        let (secret, public) =
            EndpointSecretKey::from_secret_bytes(ContentKeyRef(22), &[22; 32]).unwrap();
        let (_, recipient) =
            EndpointSecretKey::from_secret_bytes(ContentKeyRef(12), &[12; 32]).unwrap();
        let certificate = signer
            .sign_content_key_certificate(ContentKeyCertificate {
                wire_version: WireVersion(1),
                protocol_version: ProtocolVersion(2),
                deployment_domain: [7; 32],
                intended_provider: ProviderRef(30),
                relationship: handle.relationship,
                owner: BOB,
                operational_key: BOB_SENDER_KEY,
                content_key_version: ContentKeyVersion(1),
                key: public,
                valid_from: now,
                valid_until: valid_until.0,
            })
            .unwrap();
        let mut declarations = declarations();
        declarations.origin.authority = DeclarationAuthority::NativeSender(BOB_SENDER_KEY);
        let document = cs_mail_client::correspondence::compose(vec![
            cs_mail_correspondence::DocumentPart::Text("Hello Alice".into()),
        ])
        .map(|doc| {
            doc.with_subject(
                cs_mail_correspondence::ConversationSubject::new(
                    "Fixture conversation",
                    cs_mail_primitives::CanonicalTime(0),
                    0,
                    [19; 32],
                )
                .unwrap(),
            )
        })
        .unwrap();
        let package = cs_mail_client::correspondence::prepare(
            &cs_mail_client::correspondence::EndpointKeys {
                owner: BOB,
                secret: &secret,
                public,
            },
            &document,
            cs_mail_correspondence::ConversationId::new(1009).unwrap(),
            recipient,
            self.gateway.custody_key().unwrap(),
            ContentBinding {
                wire_version: WireVersion(1),
                content_ref: ContentRef(9),
                message_id: MessageId(9),
                sender: BOB,
                recipient: ALICE,
                protocol_version: ProtocolVersion(2),
                relationship: handle.relationship,
                content_scope: handle.content_scope,
                sender_certificate: content_key_certificate_digest(&certificate.certificate),
                declarations,
                message_valid_until: valid_until,
                capability: None,
            },
            (now, valid_until.0),
        )
        .unwrap();
        let document = cs_mail_application::requests::documents::RequestDocument::new(
            RequestId(9),
            ContentRef(9),
            package,
            cs_mail_primitives::AliasId::new(BOB.0).unwrap(),
        )
        .unwrap();
        let packet = cs_mail_application::requests::documents::SignedRequestDocument {
            scope: SigningScope {
                deployment_domain: [7; 32],
                intended_provider: ProviderRef(30),
                relationship: handle.relationship,
            },
            key: BOB_SENDER_KEY,
            document,
            certificate,
            signature: Vec::new(),
        }
        .sign(&[23; 32])
        .unwrap();
        self.gateway
            .upload_request_document(BOB, ALICE, &packet)
            .unwrap();
        valid_until
    }
    fn create_reverse(
        &self,
        request: RequestId,
        message: MessageId,
        quote: u128,
        operation: u128,
    ) -> SignedCommandBytes {
        let issued = self.reverse_terms(quote);
        let ReceivedOutcome::Protocol(issued) = issued.outcome else {
            panic!("quote refused")
        };
        let Some(TermsOutcome::ChargeRequired(terms)) = issued.transition.terms_outcome else {
            panic!("expected charge")
        };
        let command = self.reverse_command(
            operation,
            ProtocolCommand::CreateRequest {
                payment_method: [42; 32],
                request_id: request,
                message_id: message,
                terms,
            },
        );
        assert!(!refused(&self.reverse_submit(&command)));
        command
    }
}
#[test]
#[ignore = "requires isolated PostgreSQL"]
fn first_submission_cancels_opposite_funding_atomically_and_exact_replay_is_historical() {
    for fund in [false, true] {
        let mut f = Loop::new();
        f.both_directions();
        let terms = f.terms(1);
        assert!(!refused(&f.send(&f.create(2, &terms, 41)).unwrap()));
        f.work();
        let issued = f.reverse_terms(90);
        let ReceivedOutcome::Protocol(issued) = issued.outcome else {
            panic!("reverse quote refused")
        };
        let Some(TermsOutcome::ChargeRequired(terms)) = issued.transition.terms_outcome else {
            panic!("expected charge")
        };
        let create = f.reverse_command(
            91,
            ProtocolCommand::CreateRequest {
                payment_method: [42; 32],
                request_id: RequestId(9),
                message_id: MessageId(9),
                terms,
            },
        );
        assert!(!refused(&f.reverse_submit(&create)));
        if fund {
            f.work();
        } else {
            // The processor captures Bob's funding, but its answer is lost before
            // recording. Cancellation must reconcile this original operation.
            let address = f.gateway.address(BOB, ALICE).unwrap();
            let engine = f
                .gateway
                .deployment()
                .existing_relationship(address.aggregate_key())
                .unwrap()
                .unwrap();
            let operation = f.reverse_snapshot().payments[&RequestId(9)]
                .capture()
                .clone();
            engine
                .authorize_request_dispatch(RequestId(9), operation.id)
                .unwrap();
            f.processor.submit(&operation).unwrap();
        }
        let valid = f.upload(1);
        let submission = f.submit(3, 1, valid);
        let forward = f.snapshot();
        let reverse = f.reverse_snapshot();
        f.sql("CREATE FUNCTION fail_pair() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event#>'{kind,RequestPreparationSuperseded}' IS NOT NULL THEN RAISE EXCEPTION 'injected paired failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_pair BEFORE INSERT ON cs_protocol_events FOR EACH ROW EXECUTE FUNCTION fail_pair();");
        assert!(f.send(&submission).is_err());
        assert_eq!(f.snapshot(), forward);
        assert_eq!(f.reverse_snapshot(), reverse);
        f.sql("DROP TRIGGER fail_pair ON cs_protocol_events; DROP FUNCTION fail_pair();");
        let address = f.gateway.address(ALICE, BOB).unwrap();
        let engine = f
            .gateway
            .deployment()
            .existing_relationship(address.aggregate_key())
            .unwrap()
            .unwrap();
        engine.process_next_received().unwrap();
        let result = f.send(&submission).unwrap();
        assert!(matches!(result.outcome, ReceivedOutcome::Protocol(result) if result.replayed));
        let reverse = f.reverse_snapshot();
        assert!(matches!(
            reverse.state.requests[&RequestId(9)].lifecycle,
            RequestLifecycle::Cancelled { .. }
        ));
        let money = reverse.payments[&RequestId(9)].view();
        if fund {
            assert!(
                matches!(money.refund,RefundView::Owed(amount) if amount==Money::from_minor_units(10))
            );
        } else {
            assert!(money.awaiting_processor);
        }
        let winner = f.snapshot();
        let r = winner.state.requests[&RequestId(1)]
            .lifecycle
            .submission()
            .unwrap();
        let cancelled = reverse.state.requests[&RequestId(9)].lifecycle;
        let RequestLifecycle::Cancelled { event, .. } = cancelled else {
            unreachable!()
        };
        assert_eq!(Some(event), winner.history.last_event);
        assert!(r.at <= f.clock.now());
        f.work();
        f.work();
        assert!(
            matches!(f.reverse_snapshot().payments[&RequestId(9)].view().refund,RefundView::Confirmed(amount) if amount==Money::from_minor_units(10))
        );
        assert_eq!(f.processor.captures.len(), 2);
        let before = f.reverse_snapshot();
        f.reverse_submit(&create);
        assert_eq!(f.reverse_snapshot(), before);
    }
}
#[test]
#[ignore = "requires isolated PostgreSQL"]
fn opposite_pending_refuses_before_payment_but_unrecorded_expiry_does_not() {
    let mut f = Loop::new();
    f.both_directions();
    let issued = f.reverse_terms(89);
    let ReceivedOutcome::Protocol(issued) = issued.outcome else {
        panic!("quote refused")
    };
    let Some(TermsOutcome::ChargeRequired(terms)) = issued.transition.terms_outcome else {
        panic!("expected charge")
    };
    let create = f.reverse_command(
        91,
        ProtocolCommand::CreateRequest {
            payment_method: [42; 32],
            request_id: RequestId(9),
            message_id: MessageId(9),
            terms,
        },
    );
    f.submitted();
    let captures = f.processor.captures.len();
    assert!(matches!(
        f.reverse_terms(90).outcome,
        ReceivedOutcome::Refused(cs_mail_application::receipts::Refusal::Protocol(
            cs_mail_protocol::ProtocolError::OppositeRequestPending {
                request: RequestId(1)
            }
        ))
    ));
    assert!(matches!(
        f.reverse_submit(&create).outcome,
        ReceivedOutcome::Refused(cs_mail_application::receipts::Refusal::Protocol(
            cs_mail_protocol::ProtocolError::OppositeRequestPending {
                request: RequestId(1)
            }
        ))
    ));
    f.work();
    assert_eq!(f.processor.captures.len(), captures);
    f.advance(600_001);
    assert!(matches!(
        f.snapshot().state.requests[&RequestId(1)].lifecycle,
        RequestLifecycle::AwaitingRecipientDecision(_)
    ));
    assert!(!refused(&f.reverse_terms(92)));
}

// Claim: submission cannot consent to replies in a direction its sender terminated.
// A submitted message under the sender's block/revocation would falsify that boundary.
#[test]
#[ignore = "requires isolated PostgreSQL"]
fn sender_termination_refuses_submission_without_delivering_or_silently_restoring_permission() {
    for revoke in [false, true] {
        let mut f = Loop::new();
        f.both_directions();
        let terms = f.terms(1);
        f.send(&f.create(2, &terms, 41)).unwrap();
        f.work();
        let valid = f.upload(1);
        let submission = f.submit(3, 1, valid);
        let alice = CommandSigner::from_secret_bytes(
            ActorRef::Recipient(ALICE),
            ALICE_INBOX_KEY,
            &[13; 32],
        );
        if revoke {
            let accept = f.sign_reverse(
                &alice,
                95,
                ProtocolCommand::AcceptRelationship {
                    expected_version: Version(0),
                },
            );
            assert!(!refused(&f.reverse_submit(&accept)));
        }
        let control = f.sign_reverse(
            &alice,
            96,
            if revoke {
                ProtocolCommand::RevokeRelationship {
                    expected_version: Version(1),
                }
            } else {
                ProtocolCommand::BlockRelationship {
                    expected_version: Version(0),
                }
            },
        );
        assert!(!refused(&f.reverse_submit(&control)));
        let reverse = f.reverse_snapshot();
        let forward = f.snapshot();
        assert!(matches!(
            f.send(&submission).unwrap().outcome,
            ReceivedOutcome::Refused(cs_mail_application::receipts::Refusal::Protocol(
                cs_mail_protocol::ProtocolError::SenderReplyRestricted(reason)
            )) if reason == if revoke { cs_mail_protocol::SenderReplyRestriction::Revoked } else { cs_mail_protocol::SenderReplyRestriction::Blocked }
        ));
        assert_eq!(f.reverse_snapshot(), reverse);
        assert_eq!(f.snapshot(), forward);
        assert!(f.snapshot().messages.is_empty());
    }
}

// Claim: independently connected competing submissions cannot both win; the losing
// preparation is cancelled under the same causal commit and its original money remains owed.
#[test]
#[ignore = "requires isolated PostgreSQL"]
fn concurrent_submissions_leave_one_winner_and_one_fully_refundable_preparation() {
    let mut f = Loop::new();
    f.both_directions();
    f.create_reverse(RequestId(9), MessageId(9), 90, 91);
    let terms = f.terms(1);
    f.send(&f.create(2, &terms, 41)).unwrap();
    f.work();
    let forward_valid = f.upload(1);
    let reverse_valid = f.reverse_upload();
    let forward = f.submit(3, 1, forward_valid);
    let mut declarations = declarations();
    declarations.origin.authority = DeclarationAuthority::NativeSender(BOB_SENDER_KEY);
    let reverse = f.reverse_command(
        92,
        ProtocolCommand::SubmitRequestToRecipient {
            request_id: RequestId(9),
            expected_request_version: f.reverse_snapshot().state.requests[&RequestId(9)]
                .version
                .into(),
            content_ref: ContentRef(9),
            delivery_intent_ref: DeliveryIntentRef(9),
            declaration_digest: message_declaration_digest(&declarations).unwrap(),
            message_valid_until: reverse_valid,
        },
    );
    let first = gateway(&f.url, &f.clock);
    let second = gateway(&f.url, &f.clock);
    f.advance(1);
    let barrier = Arc::new(Barrier::new(2));
    let started = barrier.clone();
    let original = forward.clone();
    let left = std::thread::spawn(move || {
        started.wait();
        first.submit(ALICE, BOB, &original)
    });
    let right = std::thread::spawn(move || {
        barrier.wait();
        second.submit(BOB, ALICE, &reverse)
    });
    for result in [left.join().unwrap(), right.join().unwrap()] {
        if let Err(error) = result {
            // A gateway assembling its ingress while the other command is queued
            // may require that queue to drain. Other storage failures are not evidence
            // of a valid losing submission and must fail this experiment.
            assert!(matches!(
                error,
                ServiceError::Storage(StorageError::PendingCommands)
            ));
        }
    }
    let address = f.gateway.address(ALICE, BOB).unwrap();
    let engine = f
        .gateway
        .deployment()
        .existing_relationship(address.aggregate_key())
        .unwrap()
        .unwrap();
    while engine.process_next_received().unwrap() {}
    let forward = f.snapshot();
    let reverse = f.reverse_snapshot();
    let submitted = forward.state.requests[&RequestId(1)]
        .lifecycle
        .is_awaiting_recipient_decision();
    assert_ne!(
        submitted,
        reverse.state.requests[&RequestId(9)]
            .lifecycle
            .is_awaiting_recipient_decision()
    );
    let (winner, loser, id) = if submitted {
        (&forward, &reverse, RequestId(9))
    } else {
        (&reverse, &forward, RequestId(1))
    };
    let RequestLifecycle::Cancelled { event, .. } = loser.state.requests[&id].lifecycle else {
        panic!("loser not cancelled")
    };
    assert_eq!(Some(event), winner.history.last_event);
    assert!(
        matches!(loser.payments[&id].view().refund,RefundView::Owed(amount) if amount==Money::from_minor_units(10))
    );
    assert_eq!(winner.messages.len(), 1);
    assert!(loser.messages.is_empty());
    f.work();
    f.work();
    assert_eq!(f.processor.captures.len(), 2);
    let loser = if submitted {
        f.reverse_snapshot()
    } else {
        f.snapshot()
    };
    assert!(
        matches!(loser.payments[&id].view().refund,RefundView::Confirmed(amount) if amount==Money::from_minor_units(10))
    );
}

// Claim: this cutover cannot reinterpret a populated pre-cutover database.
// Successful migration or erased records would falsify the fresh-environment boundary.
#[test]
#[ignore = "requires isolated PostgreSQL"]
fn paired_cutover_refuses_populated_previous_schema_and_preserves_its_records() {
    let f = Loop::new();
    let before = f.gateway.deployment().deployment_record().unwrap();
    f.sql("DELETE FROM cs_schema_migrations WHERE version>=33;");
    assert!(PostgresDeployment::connect(&f.url).is_err());
    assert_eq!(f.gateway.deployment().deployment_record().unwrap(), before);
    assert_eq!(f.gateway.deployment().schema_version().unwrap(), 32);
    assert!(matches!(
        PostgresDeployment::connect_existing(&f.url),
        Err(StorageError::SchemaNotReady)
    ));
}
