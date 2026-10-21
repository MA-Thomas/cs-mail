//! Agreed counterexamples: partial acceptance/reverse permission, a revived answer
//! after withdrawal, duplicate send on replay, unauthorized reuse and ciphertext-only
//! persistence. These are real signed commands against `PostgreSQL`, not adapter mocks.
use super::*;
use cs_mail_application::correspondence::{
    Command, CorrespondenceService, MessageView, PreparedMessage, Response, SignedRequest,
};
use cs_mail_client::correspondence::{EndpointKeys, compose, open, prepare, readable_text};
use cs_mail_correspondence::{ConversationId, CorrespondenceError, DocumentPart};
use cs_mail_test_support::fixture_native_send;

fn signed(f: &Loop, actor: usize, operation: u128, command: Command) -> SignedRequest {
    let (persona, key, seed) = if actor == 0 {
        (ALICE, ALICE_KEY, 11)
    } else {
        (BOB, BOB_KEY, 21)
    };
    let at = f.clock.now();
    let account = f
        .gateway
        .deployment()
        .accounts(move || at)
        .persona_account(persona)
        .unwrap();
    SignedRequest {
        product: "cs-mail/test".into(),
        account,
        persona,
        key,
        operation: IdempotencyKey(operation),
        command,
        signature: Vec::new(),
    }
    .sign(&[seed; 32])
    .unwrap()
}
fn call(f: &Loop, request: &SignedRequest) -> Result<Response, StorageError> {
    let clock = f.clock.clone();
    let repository = f.gateway.deployment().accounts(move || clock.now());
    CorrespondenceService::new(&repository).handle(request)
}
fn package(
    f: &Loop,
    actor: usize,
    id: u128,
    conversation: u128,
    parts: Vec<DocumentPart>,
) -> PreparedMessage {
    package_with_capability(f, actor, id, conversation, parts, None)
}
fn package_with_capability(
    f: &Loop,
    actor: usize,
    id: u128,
    conversation: u128,
    parts: Vec<DocumentPart>,
    capability: Option<LaneId>,
) -> PreparedMessage {
    let (author, other, key, seed, content, recipient) = if actor == 0 {
        (
            ALICE,
            BOB,
            ALICE_KEY,
            12,
            ContentKeyRef(12),
            ContentKeyRef(22),
        )
    } else {
        (
            BOB,
            ALICE,
            BOB_KEY,
            22,
            ContentKeyRef(22),
            ContentKeyRef(12),
        )
    };
    let (secret, public) = EndpointSecretKey::from_secret_bytes(content, &[seed; 32]).unwrap();
    let other_seed = if actor == 0 { 22 } else { 12 };
    let recipient = EndpointSecretKey::from_secret_bytes(recipient, &[other_seed; 32])
        .unwrap()
        .1;
    let handle = f.gateway.address(author, other).unwrap().public();
    let at = f.clock.now();
    let until = CanonicalTime(at.0 + 1_000_000);
    prepare(
        &EndpointKeys {
            owner: author,
            secret: &secret,
            public,
        },
        &compose(parts)
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
            .unwrap(),
        ConversationId::new(conversation).unwrap(),
        recipient,
        f.gateway.custody_key().unwrap(),
        ContentBinding {
            wire_version: WireVersion(1),
            content_ref: ContentRef(id),
            message_id: MessageId(id),
            sender: author,
            recipient: other,
            protocol_version: ProtocolVersion(2),
            relationship: handle.relationship,
            content_scope: handle.content_scope,
            sender_certificate: cs_mail_content::ContentCertificateDigest([0; 32]),
            declarations: MessageDeclarations {
                purpose: DeclaredPurpose::Known(KnownPurpose::Personal),
                origin: OriginDeclaration {
                    mode: OriginMode::HumanInitiated,
                    authority: DeclarationAuthority::NativeSender(key),
                },
                payload_schema: None,
            },
            message_valid_until: MessageValidityUntil(until),
            capability,
        },
        (at, until),
    )
    .unwrap()
}
fn view(f: &Loop, actor: usize, id: u128, operation: u128) -> MessageView {
    let Response::Message(view) = call(
        f,
        &signed(
            f,
            actor,
            operation,
            Command::Fetch {
                message: MessageId(id),
            },
        ),
    )
    .unwrap() else {
        panic!("copy unavailable")
    };
    *view
}
fn reverse(f: &Loop) -> SettlementSnapshot {
    let address = f.gateway.address(BOB, ALICE).unwrap();
    f.gateway
        .deployment()
        .existing_relationship(address.aggregate_key())
        .unwrap()
        .unwrap()
        .snapshot()
        .unwrap()
}
fn register_alice_inbox(f: &Loop) {
    let at = f.clock.now();
    let repository = f.gateway.deployment().accounts(move || at);
    let account = repository.persona_account(ALICE).unwrap();
    cs_mail_application::accounts::operations::AccountService::new(&repository, &|| at)
        .execute_command(
            &cs_mail_accounts::control::SignedAccountCommand {
                account,
                operational_key: ALICE_KEY,
                expected_revision: repository.account_control(account).unwrap().revision(),
                idempotency_key: IdempotencyKey(9900),
                product: "cs-mail/test".into(),
                command: cs_mail_accounts::control::AccountCommand::RegisterKey(
                    cs_mail_accounts::control::NewOperationalKey::prove(
                        account,
                        "cs-mail/test",
                        OperationalKeyRef(13),
                        ActorRef::Recipient(ALICE),
                        &[13; 32],
                    )
                    .unwrap(),
                ),
                signature: Vec::new(),
            }
            .sign(&[11; 32])
            .unwrap(),
        )
        .unwrap();
}
fn control(f: &Loop, operation: u128, command: ProtocolCommand) -> ServiceOutcome {
    let handle = f.gateway.address(BOB, ALICE).unwrap().public();
    let signer = CommandSigner::from_secret_bytes(
        ActorRef::Recipient(ALICE),
        OperationalKeyRef(13),
        &[13; 32],
    );
    let request = signer
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
        .unwrap();
    f.advance(1);
    f.gateway.submit(BOB, ALICE, &request).unwrap()
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
#[allow(clippy::too_many_lines)] // Keep decision, delivery, retry and recovery observations together.
fn submitted_documents_and_answers_survive_rejection_expiry_and_exact_replay() {
    for disposition in 0..3 {
        let mut f = Loop::new();
        f.submitted();
        if disposition == 1 {
            f.send(&f.sign(
                &f.bob,
                91,
                ProtocolCommand::RejectRelationship {
                    expected_version: f.snapshot().state.relationship.version.into(),
                },
            ))
            .unwrap();
        }
        if disposition == 2 {
            f.advance(600_001);
            f.work();
        }
        // Continuity depends on delivered conversation evidence, never on a broad reverse-lane agreement.
        let source = view(
            &f,
            1,
            f.snapshot().state.requests[&RequestId(1)].initial_message.0,
            9900 + disposition,
        );
        let empty = cs_mail_correspondence::Conversation::begun_by_request(
            source.record.manifest.conversation(),
            cs_mail_correspondence::CorrespondenceRef::new(ALICE, BOB).unwrap(),
        );
        let current = reverse(&f).state.relationship;
        let restored = cs_mail_application::correspondence::restore_received_conversation(
            &empty,
            &source.record,
            Some(&current),
            None,
        )
        .unwrap();
        assert_eq!(
            restored.reply_source(BOB, current.reply_epoch),
            Some(source.record.manifest.message())
        );
        let mut blocked = current.clone();
        blocked.state = cs_mail_protocol::RelationshipState::Blocked;
        assert_eq!(
            cs_mail_application::correspondence::restore_received_conversation(
                &empty,
                &source.record,
                Some(&blocked),
                None
            )
            .unwrap(),
            empty
        );
        let before = f.snapshot();
        let payload = package(
            &f,
            1,
            200,
            1001,
            vec![DocumentPart::Text("answer-canary-private".into())],
        );
        let request = signed(&f, 1, 200, fixture_native_send(Box::new(payload)));
        // The transport may reorder object fields; signatures bind the typed command.
        let mut tampered = request.clone();
        tampered.command = Command::Fetch {
            message: MessageId(1),
        };
        assert!(matches!(
            call(&f, &tampered),
            Err(StorageError::Correspondence(
                CorrespondenceError::Unauthorized
            ))
        ));
        let reordered: SignedRequest =
            serde_json::from_value(serde_json::to_value(&request).unwrap()).unwrap();
        assert_eq!(
            call(&f, &reordered).unwrap(),
            Response::Committed(MessageId(200))
        );
        assert_eq!(
            call(&f, &request).unwrap(),
            Response::Committed(MessageId(200))
        );
        assert_eq!(f.snapshot(), before); // answering makes no request or money decision
        let actual = view(&f, 0, 200, 201);
        assert!(matches!(
            actual.record.admission,
            cs_mail_protocol::AdmissionBasis::ConversationReply { .. }
        ));
        let (secret, public) =
            EndpointSecretKey::from_secret_bytes(ContentKeyRef(12), &[12; 32]).unwrap();
        assert_eq!(
            readable_text(
                &open(
                    &EndpointKeys {
                        owner: ALICE,
                        secret: &secret,
                        public
                    },
                    &actual,
                    actual.record.sender_key
                )
                .unwrap()
            ),
            "answer-canary-private"
        );
        let changed = signed(
            &f,
            1,
            200,
            fixture_native_send(Box::new(package(
                &f,
                1,
                200,
                1001,
                vec![DocumentPart::Text("altered".into())],
            ))),
        );
        assert!(matches!(
            call(&f, &changed),
            Err(StorageError::Correspondence(CorrespondenceError::Conflict))
        ));
        let mut db = postgres::Client::connect(&f.url, postgres::NoTls).unwrap();
        assert_eq!(
            db.query_one(
                "SELECT count(*) FROM cs_correspondence_copies WHERE message='200'",
                &[]
            )
            .unwrap()
            .get::<_, i64>(0),
            2
        );
        assert_eq!(
            db.query_one(
                "SELECT count(*) FROM cs_correspondence_recovery WHERE message='1'",
                &[]
            )
            .unwrap()
            .get::<_, i64>(0),
            2
        );
        let persisted = db
            .query_one(
                "SELECT string_agg(record::text,'') FROM cs_correspondence_messages",
                &[],
            )
            .unwrap()
            .get::<_, Option<String>>(0)
            .unwrap();
        assert!(!persisted.contains("answer-canary-private"));
    }
}
#[test]
#[ignore = "requires isolated PostgreSQL"]
fn acceptance_and_frozen_reply_lane_roll_back_together() {
    let mut f = Loop::new();
    f.submitted();
    let before = f.snapshot();
    let opposite = reverse(&f);
    let accept = f.accept(100);
    f.sql("CREATE FUNCTION fail_reply() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected reply failure'; END $$; CREATE TRIGGER fail_reply BEFORE INSERT ON cs_capability_events FOR EACH ROW EXECUTE FUNCTION fail_reply();");
    assert!(f.send(&accept).is_err());
    assert_eq!(f.snapshot(), before);
    assert_eq!(reverse(&f), opposite);
    f.sql("DROP TRIGGER fail_reply ON cs_capability_events;DROP FUNCTION fail_reply();");
    let address = f.gateway.address(ALICE, BOB).unwrap();
    f.gateway
        .deployment()
        .existing_relationship(address.aggregate_key())
        .unwrap()
        .unwrap()
        .process_next_received()
        .unwrap();
    f.send(&accept).unwrap();
    let lane = reverse(&f).lane.unwrap();
    assert!(matches!(
        lane.authority(),
        cs_mail_capabilities::reply::LaneAuthority::RequestAcceptance {
            request: RequestId(1),
            ..
        }
    ));
    assert_eq!(
        lane.grant().lifetime,
        before.state.requests[&RequestId(1)]
            .terms
            .reply_lane
            .agreed()
            .unwrap()
            .lifetime()
    );
    assert_eq!(
        lane.grant().rate_limit,
        before.state.requests[&RequestId(1)]
            .terms
            .reply_lane
            .agreed()
            .unwrap()
            .rate_limit()
    );
    assert!(
        matches!(f.snapshot().payments[&RequestId(1)].view().refund,RefundView::Owed(amount) if amount==Money::from_minor_units(10))
    );
    let new = signed(
        &f,
        1,
        300,
        Command::Create {
            subject: cs_mail_correspondence::ConversationSubject::new(
                "Fixture conversation",
                cs_mail_primitives::CanonicalTime(0),
                0,
                [19; 32],
            )
            .unwrap()
            .commitment(),
            id: ConversationId::new(2000).unwrap(),
            other: ALICE,
        },
    );
    call(&f, &new).unwrap();
    let packet = package_with_capability(
        &f,
        1,
        301,
        2000,
        vec![DocumentPart::Text("another conversation".into())],
        Some(lane.grant().id),
    );
    assert_eq!(
        call(
            &f,
            &signed(&f, 1, 301, fixture_native_send(Box::new(packet)))
        )
        .unwrap(),
        Response::Committed(MessageId(301))
    );
}
#[test]
#[ignore = "requires isolated PostgreSQL"]
fn revoking_scoped_answers_and_unblocking_never_revives_the_old_offer() {
    for revoke in [false, true] {
        let mut f = Loop::new();
        f.submitted();
        register_alice_inbox(&f);
        let waiting = signed(
            &f,
            1,
            400,
            fixture_native_send(Box::new(package(
                &f,
                1,
                400,
                1001,
                vec![DocumentPart::Text("waiting".into())],
            ))),
        );
        let version = reverse(&f).state.relationship.version.into();
        let action = if revoke {
            ProtocolCommand::RevokeRelationship {
                expected_version: version,
            }
        } else {
            ProtocolCommand::BlockRelationship {
                expected_version: version,
            }
        };
        assert!(!refused(&control(&f, 401, action)));
        assert!(matches!(
            call(&f, &waiting),
            Ok(Response::Refused {
                reason: CorrespondenceError::Unauthorized,
                ..
            })
        ));
        if !revoke {
            let version = reverse(&f).state.relationship.version.into();
            control(
                &f,
                402,
                ProtocolCommand::UnblockRelationship {
                    expected_version: version,
                },
            );
        }
        assert!(reverse(&f).state.relationship.reply_epoch.0 > 0);
        assert!(matches!(
            call(&f, &waiting),
            Ok(Response::Refused {
                reason: CorrespondenceError::Unauthorized,
                ..
            })
        ));
        assert!(
            call(
                &f,
                &signed(
                    &f,
                    0,
                    403,
                    Command::Fetch {
                        message: MessageId(1)
                    }
                )
            )
            .is_ok()
        );
        f.send(&f.accept(404)).unwrap();
        assert!(reverse(&f).lane.is_none());
    }
}

// Claim: acceptance cannot replace an explicit custom reverse lane or refill its allowance.
#[test]
#[ignore = "requires isolated PostgreSQL"]
fn acceptance_preserves_custom_reverse_lane_and_consumed_allowance() {
    let mut f = Loop::new();
    f.submitted();
    register_alice_inbox(&f);
    let mut grant = f.lane(10_000_000).grant;
    let handle = f.gateway.address(BOB, ALICE).unwrap().public();
    grant.sender = BOB;
    grant.recipient = ALICE;
    grant.id = LaneId(777);
    grant.subject = LaneSubject::Native(cs_mail_privacy::ScopedHandle(
        *handle.relationship.as_bytes(),
    ));
    grant.recipient_operational_key = OperationalKeyRef(13);
    let signature = ed25519_dalek::Signer::sign(
        &ed25519_dalek::SigningKey::from_bytes(&[13; 32]),
        &grant.signing_bytes().unwrap(),
    )
    .to_bytes();
    f.gateway
        .grant_lane(&SignedLaneGrant { grant, signature }, IdempotencyKey(700))
        .unwrap();
    let packet = package_with_capability(
        &f,
        1,
        701,
        1001,
        vec![DocumentPart::Text("custom lane answer".into())],
        Some(LaneId(777)),
    );
    call(
        &f,
        &signed(&f, 1, 701, fixture_native_send(Box::new(packet))),
    )
    .unwrap();
    let before = reverse(&f).lane.unwrap();
    assert_eq!(before.messages_in_rate_window(), 1);
    f.send(&f.accept(702)).unwrap();
    assert_eq!(reverse(&f).lane.unwrap(), before);
}

// Claim: a rejected request remains restricted; neither participant can relax it alone.
#[test]
#[ignore = "requires isolated PostgreSQL"]
fn request_conversation_reuse_requires_both_participants_even_after_rejection() {
    use cs_mail_correspondence::{CorrespondenceRef, PolicyAction};
    let mut f = Loop::new();
    f.submitted();
    let version = f.snapshot().state.relationship.version.into();
    f.send(&f.sign(
        &f.bob,
        800,
        ProtocolCommand::RejectRelationship {
            expected_version: version,
        },
    ))
    .unwrap();
    let id = ConversationId::new(1001).unwrap();
    let Response::Conversation(found) =
        call(&f, &signed(&f, 1, 801, Command::Policy { id })).unwrap()
    else {
        panic!()
    };
    let elsewhere = CorrespondenceRef::new(BOB, ProtocolIdentity(99)).unwrap();
    assert!(!found.permits(elsewhere));
    assert!(matches!(
        call(
            &f,
            &signed(
                &f,
                1,
                802,
                Command::ChangePolicy {
                    id,
                    revision: 0,
                    action: PolicyAction::ApproveRelaxation
                }
            )
        ),
        Ok(Response::Refused { .. })
    ));
    call(
        &f,
        &signed(
            &f,
            1,
            803,
            Command::ChangePolicy {
                id,
                revision: 0,
                action: PolicyAction::ProposeRelaxation,
            },
        ),
    )
    .unwrap();
    assert!(matches!(
        call(
            &f,
            &signed(
                &f,
                1,
                804,
                Command::ChangePolicy {
                    id,
                    revision: 1,
                    action: PolicyAction::ApproveRelaxation
                }
            )
        ),
        Ok(Response::Refused { .. })
    ));
    call(
        &f,
        &signed(
            &f,
            0,
            805,
            Command::ChangePolicy {
                id,
                revision: 1,
                action: PolicyAction::ApproveRelaxation,
            },
        ),
    )
    .unwrap();
    let Response::Conversation(found) =
        call(&f, &signed(&f, 1, 806, Command::Policy { id })).unwrap()
    else {
        panic!()
    };
    assert!(found.permits(elsewhere));
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn every_received_conversation_keeps_replies_after_broad_reverse_lane_expiry() {
    // Conjecture: an ordinary received conversation remains answerable after a broad lane expires,
    // without permitting its recipient to start another conversation. Missing reply or successful new
    // initiation would contradict the received-conversation scope confirmed by the owner.
    let mut f = Loop::new();
    f.submitted();
    f.send(&f.sign(
        &f.bob,
        9500,
        ProtocolCommand::AcceptRelationship {
            expected_version: f.snapshot().state.relationship.version.into(),
        },
    ))
    .unwrap();
    let lifetime = f.snapshot().state.requests[&RequestId(1)]
        .terms
        .reply_lane
        .agreed()
        .unwrap()
        .lifetime()
        .0;
    f.advance(lifetime + 1);
    f.work();
    call(
        &f,
        &signed(
            &f,
            0,
            9501,
            Command::Create {
                subject: cs_mail_correspondence::ConversationSubject::new(
                    "Fixture conversation",
                    cs_mail_primitives::CanonicalTime(0),
                    0,
                    [19; 32],
                )
                .unwrap()
                .commitment(),
                id: ConversationId::new(9501).unwrap(),
                other: BOB,
            },
        ),
    )
    .unwrap();
    let outgoing = package(
        &f,
        0,
        9502,
        9501,
        vec![DocumentPart::Text(
            "ordinary invitation after lane expiry".into(),
        )],
    );
    assert_eq!(
        call(
            &f,
            &signed(&f, 0, 9502, fixture_native_send(Box::new(outgoing)))
        )
        .unwrap(),
        Response::Committed(MessageId(9502))
    );
    let reply = package(
        &f,
        1,
        9503,
        9501,
        vec![DocumentPart::Text("reply without a broad lane".into())],
    );
    assert_eq!(
        call(
            &f,
            &signed(&f, 1, 9503, fixture_native_send(Box::new(reply)))
        )
        .unwrap(),
        Response::Committed(MessageId(9503))
    );
    assert!(matches!(
        call(
            &f,
            &signed(
                &f,
                1,
                9504,
                Command::Create {
                    subject: cs_mail_correspondence::ConversationSubject::new(
                        "Fixture conversation",
                        cs_mail_primitives::CanonicalTime(0),
                        0,
                        [19; 32]
                    )
                    .unwrap()
                    .commitment(),
                    id: ConversationId::new(9504).unwrap(),
                    other: ALICE
                }
            )
        )
        .unwrap(),
        Response::Refused {
            reason: CorrespondenceError::Unauthorized,
            ..
        }
    ));
}
