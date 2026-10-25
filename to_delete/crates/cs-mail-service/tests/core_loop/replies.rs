//! Agreed claims about replies to member requests, challenged through the member gateway
//! against `PostgreSQL`:
//!
//! 1. Acceptance and the reply lane it grants commit together or not at all. Falsified by
//!    an injected failure leaving the request accepted without the recipient's lane, or
//!    the lane without the acceptance. A request would otherwise consent to nothing.
//! 2. A request cannot be submitted while its sender blocks or has revoked the recipient,
//!    including a block received but not yet applied. Falsified by such a submission
//!    reaching the recipient: it would consent to replies its sender has refused.
//! 3. Once a block or revocation of the recipient commits, no later answer from the
//!    recipient is delivered. Falsified by an answer, in the request's conversation or
//!    under the lane, delivered after the sender withdrew consent.
//! 4. Request content, including a rejected request's, cannot be reused into another
//!    relationship. Falsified by the recipient sharing it with a third member.
//! 5. A retried answer is delivered once, and an altered retry under its operation
//!    identity is refused. Falsified by two deliveries or an accepted substitution.
//! 6. A request cannot reach terms, creation or submission while the other member's
//!    request to its sender awaits that sender's decision, including one received
//!    ahead of it but not yet applied; the refusal moves no money. Falsified by a
//!    quote, charge or submission in the crossed direction.
//! 7. A funded request refused at submission because the other request landed first
//!    returns its full charge when cancelled. Falsified by any retained amount.
//! 8. Accepting while holding an unsubmitted request to the sender grants the reply
//!    lane and refunds that request in full, together or not at all. Falsified by a
//!    lane without the refund, or the reverse.
use super::*;
use cs_mail_application::correspondence::{Command, MessageView, Response, SignedRequest};
use cs_mail_client::correspondence::{EndpointKeys, compose, open, prepare};
use cs_mail_content::{ContentCertificateDigest, EndpointPublicKey};
use cs_mail_correspondence::{CorrespondenceError, MessageSelection, SelectionElement, SharedCopy};
use cs_mail_primitives::AccountId;
use cs_mail_protocol::ProtocolError;

const CAROL: ProtocolIdentity = ProtocolIdentity(30);
const CAROL_KEY: OperationalKeyRef = OperationalKeyRef(31);
/// The key with which Alice decides about Bob writing to her.
const ALICE_INBOX_KEY: OperationalKeyRef = OperationalKeyRef(13);
/// The key with which Bob requests others.
const BOB_SENDER_KEY: OperationalKeyRef = OperationalKeyRef(23);
const ALICE_CONVERSATION: u128 = 500;
const CAROL_CONVERSATION: u128 = 501;

/// A member who requests another: Alice and Carol request Bob, and Bob may request Alice.
struct Party {
    persona: ProtocolIdentity,
    recipient: ProtocolIdentity,
    key: OperationalKeyRef,
    signer: CommandSigner,
    content: u8,
    /// The verified bank account its charges are drawn from.
    payment_method: [u8; 32],
}
fn alice() -> Party {
    Party {
        persona: ALICE,
        recipient: BOB,
        key: ALICE_KEY,
        signer: CommandSigner::from_secret_bytes(ActorRef::Sender(ALICE), ALICE_KEY, &[11; 32]),
        content: 12,
        payment_method: [41; 32],
    }
}
fn carol() -> Party {
    Party {
        persona: CAROL,
        recipient: BOB,
        key: CAROL_KEY,
        signer: CommandSigner::from_secret_bytes(ActorRef::Sender(CAROL), CAROL_KEY, &[31; 32]),
        content: 32,
        payment_method: [43; 32],
    }
}
/// Bob as a sender to Alice, under the sender key he registers for it.
fn bob_to_alice() -> Party {
    Party {
        persona: BOB,
        recipient: ALICE,
        key: BOB_SENDER_KEY,
        signer: CommandSigner::from_secret_bytes(ActorRef::Sender(BOB), BOB_SENDER_KEY, &[23; 32]),
        content: 22,
        payment_method: [42; 32],
    }
}
/// The content key each member's client holds.
fn content_of(persona: ProtocolIdentity) -> u8 {
    match persona {
        ALICE => 12,
        BOB => 22,
        _ => 32,
    }
}
fn alice_inbox() -> CommandSigner {
    CommandSigner::from_secret_bytes(ActorRef::Recipient(ALICE), ALICE_INBOX_KEY, &[13; 32])
}
fn content_key(n: u8) -> (EndpointSecretKey, EndpointPublicKey) {
    EndpointSecretKey::from_secret_bytes(ContentKeyRef(u128::from(n)), &[n; 32]).unwrap()
}
fn written_by(party: &Party) -> MessageDeclarations {
    MessageDeclarations {
        purpose: DeclaredPurpose::Known(KnownPurpose::Personal),
        origin: OriginDeclaration {
            mode: OriginMode::HumanInitiated,
            authority: DeclarationAuthority::NativeSender(party.key),
        },
        payload_schema: None,
    }
}
fn bob_writes() -> MessageDeclarations {
    MessageDeclarations {
        purpose: DeclaredPurpose::Known(KnownPurpose::Personal),
        origin: OriginDeclaration {
            mode: OriginMode::HumanInitiated,
            authority: DeclarationAuthority::NativeSender(BOB_KEY),
        },
        payload_schema: None,
    }
}
fn refused_with(result: &Result<Response, ServiceError>, expected: CorrespondenceError) -> bool {
    matches!(
        result,
        Err(ServiceError::Storage(StorageError::Correspondence(error))) if *error == expected
    )
}

impl Loop {
    /// Carol enrolls as a sender with annual service.
    fn enroll_carol(&self) {
        let deployment = PostgresDeployment::connect(&self.url).unwrap();
        let at = self.clock.now();
        enroll(
            &deployment,
            3,
            CAROL,
            ActorRef::Sender(CAROL),
            CAROL_KEY,
            at,
        );
        support::purchase_service(
            &deployment,
            scope(),
            UNIT,
            BillingAccountId(3),
            CAROL_KEY,
            at,
        )
        .unwrap();
    }
    fn account(&self, persona: ProtocolIdentity) -> AccountId {
        let clock = self.clock.clone();
        self.gateway
            .deployment()
            .accounts(move || clock.now())
            .persona_account(persona)
            .unwrap()
    }
    /// `persona` registers another key, authorized by its existing `manager` key.
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
    /// Alice registers the key with which she decides about those writing to her.
    fn alice_registers_inbox_key(&self) {
        self.register_key(
            ALICE,
            (ALICE_KEY, 11),
            (ALICE_INBOX_KEY, ActorRef::Recipient(ALICE), 13),
            9_001,
        );
    }
    /// Alice and Bob can each request the other: Alice publishes a request class and
    /// Bob registers a sender key.
    fn requests_both_ways(&self) {
        self.alice_registers_inbox_key();
        self.register_key(
            BOB,
            (BOB_KEY, 21),
            (BOB_SENDER_KEY, ActorRef::Sender(BOB), 23),
            9_002,
        );
        self.gateway
            .publish_classes(
                &alice_inbox()
                    .sign_request_classes(
                        self.gateway.scope(),
                        cs_mail_protocol::pricing::RecipientRequestClasses::new(
                            ALICE,
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
    fn signed_in(
        &self,
        signer: &CommandSigner,
        (sender, recipient): (ProtocolIdentity, ProtocolIdentity),
        key: u128,
        command: ProtocolCommand,
    ) -> SignedCommandBytes {
        let handle = self.gateway.address(sender, recipient).unwrap().public();
        signer
            .sign(
                SigningScope {
                    deployment_domain: [7; 32],
                    intended_provider: ProviderRef(30),
                    relationship: handle.relationship,
                },
                ProtocolVersion(2),
                IdempotencyKey(key),
                command,
            )
            .unwrap()
    }
    fn submit_in(
        &self,
        pair: (ProtocolIdentity, ProtocolIdentity),
        signed: &SignedCommandBytes,
    ) -> ServiceOutcome {
        self.advance(1);
        self.gateway.submit(pair.0, pair.1, signed).unwrap()
    }
    fn snapshot_of(
        &self,
        sender: ProtocolIdentity,
        recipient: ProtocolIdentity,
    ) -> SettlementSnapshot {
        let address = self.gateway.address(sender, recipient).unwrap();
        self.gateway
            .deployment()
            .existing_relationship(address.aggregate_key())
            .unwrap()
            .unwrap()
            .snapshot()
            .unwrap()
    }
    /// Bob writing to Alice: the direction a request's consent opens.
    fn reverse(&self) -> SettlementSnapshot {
        self.snapshot_of(BOB, ALICE)
    }
    /// Alice decides about Bob writing to her.
    fn alice_decides(&self, key: u128, command: impl FnOnce(Version) -> ProtocolCommand) {
        let version = self.reverse().state.relationship.version;
        let outcome = self.submit_in(
            (BOB, ALICE),
            &self.signed_in(&alice_inbox(), (BOB, ALICE), key, command(version.into())),
        );
        assert!(
            matches!(outcome.outcome, ReceivedOutcome::Protocol(_)),
            "{:?}",
            outcome.outcome
        );
    }
    /// Alice's block of Bob, durably received but not yet applied.
    fn alice_block_received(&self, key: u128) {
        let version = self.reverse().state.relationship.version;
        let signed = self.signed_in(
            &alice_inbox(),
            (BOB, ALICE),
            key,
            ProtocolCommand::BlockRelationship {
                expected_version: version.into(),
            },
        );
        self.received_only((BOB, ALICE), &signed);
    }
    /// `signed`, durably received in `pair`'s relationship but not yet applied.
    fn received_only(
        &self,
        (sender, recipient): (ProtocolIdentity, ProtocolIdentity),
        signed: &SignedCommandBytes,
    ) {
        let address = self.gateway.address(sender, recipient).unwrap();
        self.advance(1);
        let clock = self.clock.clone();
        self.gateway
            .deployment()
            .existing_relationship(address.aggregate_key())
            .unwrap()
            .unwrap()
            .receive_signed(
                signed,
                [7; 32],
                move || clock.now(),
                self.gateway.policy().unwrap(),
            )
            .unwrap();
    }
    /// `party` asks for terms of request `id`.
    fn terms_for(&self, party: &Party, id: u128) -> ServiceOutcome {
        let pair = (party.persona, party.recipient);
        self.submit_in(
            pair,
            &self.signed_in(
                &party.signer,
                pair,
                id * 100 + 1,
                ProtocolCommand::IssueRequestTerms {
                    class_id: RequestClassId(1),
                    quote_id: QuoteId(id * 100 + 1),
                    declaration_digest: Some(
                        message_declaration_digest(&written_by(party)).unwrap(),
                    ),
                },
            ),
        )
    }
    /// `party` creates request `id` under `terms`.
    fn create_for(
        &self,
        party: &Party,
        id: u128,
        terms: Box<cs_mail_protocol::RequestTerms>,
    ) -> ServiceOutcome {
        let pair = (party.persona, party.recipient);
        self.submit_in(
            pair,
            &self.signed_in(
                &party.signer,
                pair,
                id * 100 + 2,
                ProtocolCommand::CreateRequest {
                    payment_method: party.payment_method,
                    request_id: RequestId(id),
                    message_id: MessageId(id),
                    terms,
                },
            ),
        )
    }
    /// `party` issues, creates, funds and stages request `id`, whose message begins
    /// `conversation`. Returns the submission, unsigned.
    fn prepared_request(&mut self, party: &Party, id: u128, conversation: u128) -> ProtocolCommand {
        let issued = self.terms_for(party, id);
        let ReceivedOutcome::Protocol(issued) = issued.outcome else {
            panic!("terms refused: {:?}", issued.outcome)
        };
        let Some(TermsOutcome::ChargeRequired(terms)) = issued.transition.terms_outcome else {
            panic!("expected a charge")
        };
        let created = self.create_for(party, id, terms);
        assert!(!refused(&created), "{:?}", created.outcome);
        self.work();
        let valid_until = self.stage_request_message(party, id, conversation);
        let version = self
            .snapshot_of(party.persona, party.recipient)
            .state
            .requests[&RequestId(id)]
            .version;
        ProtocolCommand::SubmitRequestToRecipient {
            request_id: RequestId(id),
            expected_request_version: version.into(),
            content_ref: ContentRef(id),
            delivery_intent_ref: DeliveryIntentRef(id),
            declaration_digest: message_declaration_digest(&written_by(party)).unwrap(),
            message_valid_until: valid_until,
        }
    }
    /// Stages request `id`'s message, beginning `conversation`, as the party's client does.
    fn stage_request_message(
        &self,
        party: &Party,
        id: u128,
        conversation: u128,
    ) -> MessageValidityUntil {
        let handle = self
            .gateway
            .address(party.persona, party.recipient)
            .unwrap()
            .public();
        let now = self.clock.now();
        let valid_until = MessageValidityUntil(CanonicalTime(now.0 + 3_600_000));
        let (secret, public) = content_key(party.content);
        let certificate = party
            .signer
            .sign_content_key_certificate(ContentKeyCertificate {
                wire_version: WireVersion(1),
                protocol_version: ProtocolVersion(2),
                deployment_domain: [7; 32],
                intended_provider: ProviderRef(30),
                relationship: handle.relationship,
                owner: party.persona,
                operational_key: party.key,
                content_key_version: ContentKeyVersion(1),
                key: public,
                valid_from: now,
                valid_until: valid_until.0,
            })
            .unwrap();
        let package = prepare(
            &EndpointKeys {
                owner: party.persona,
                secret: &secret,
                public,
            },
            &compose(vec![DocumentPart::Text("Hello".into())]).unwrap(),
            ConversationId::new(conversation).unwrap(),
            content_key(content_of(party.recipient)).1,
            self.gateway.custody_public_key().unwrap(),
            ContentBinding {
                wire_version: WireVersion(1),
                content_ref: ContentRef(id),
                message_id: MessageId(id),
                sender: party.persona,
                recipient: party.recipient,
                protocol_version: ProtocolVersion(2),
                relationship: handle.relationship,
                content_scope: handle.content_scope,
                sender_certificate: content_key_certificate_digest(&certificate.certificate),
                declarations: written_by(party),
                message_valid_until: valid_until,
                capability: None,
            },
            (now, valid_until.0),
        )
        .unwrap();
        self.gateway
            .upload(
                party.persona,
                party.recipient,
                &RequestDocument {
                    request: RequestId(id),
                    content_ref: ContentRef(id),
                    package,
                },
                &certificate,
            )
            .unwrap();
        valid_until
    }
    fn submit_request(
        &self,
        party: &Party,
        key: u128,
        submission: ProtocolCommand,
    ) -> ServiceOutcome {
        let pair = (party.persona, party.recipient);
        self.submit_in(pair, &self.signed_in(&party.signer, pair, key, submission))
    }
    /// `party`'s request `id`, submitted and awaiting its recipient's decision.
    fn requested(&mut self, party: &Party, id: u128, conversation: u128) {
        let submission = self.prepared_request(party, id, conversation);
        let outcome = self.submit_request(party, id * 100 + 3, submission);
        assert!(!refused(&outcome), "{:?}", outcome.outcome);
        assert!(matches!(
            self.snapshot_of(party.persona, party.recipient)
                .state
                .requests[&RequestId(id)]
                .lifecycle,
            RequestLifecycle::AwaitingRecipientDecision(_)
        ));
    }
    /// Bob decides about `sender` writing to him.
    fn bob_decides(
        &self,
        sender: ProtocolIdentity,
        key: u128,
        command: impl FnOnce(Version) -> ProtocolCommand,
    ) {
        let version = self.snapshot_of(sender, BOB).state.relationship.version;
        let outcome = self.submit_in(
            (sender, BOB),
            &self.signed_in(&self.bob, (sender, BOB), key, command(version.into())),
        );
        assert!(!refused(&outcome), "{:?}", outcome.outcome);
    }
    fn correspondence_as(
        &self,
        persona: ProtocolIdentity,
        (key, secret): (OperationalKeyRef, u8),
        operation: u128,
        command: Command,
    ) -> SignedRequest {
        SignedRequest {
            product: "cs-mail/test".into(),
            account: self.account(persona),
            persona,
            key,
            operation: IdempotencyKey(operation),
            command,
            signature: vec![],
        }
        .sign(&[secret; 32])
        .unwrap()
    }
    /// Bob's signed message to `to` in `conversation`, presenting `lane` when given.
    fn bob_message(
        &self,
        to: ProtocolIdentity,
        conversation: u128,
        id: u128,
        parts: Vec<DocumentPart>,
        lane: Option<LaneId>,
    ) -> SignedRequest {
        let handle = self.gateway.address(BOB, to).unwrap().public();
        let now = self.clock.now();
        let valid_until = MessageValidityUntil(CanonicalTime(now.0 + 3_600_000));
        let (secret, public) = content_key(22);
        let other = if to == ALICE { 12 } else { 32 };
        let package = prepare(
            &EndpointKeys {
                owner: BOB,
                secret: &secret,
                public,
            },
            &compose(parts).unwrap(),
            ConversationId::new(conversation).unwrap(),
            content_key(other).1,
            self.gateway.custody_public_key().unwrap(),
            ContentBinding {
                wire_version: WireVersion(1),
                content_ref: ContentRef(id),
                message_id: MessageId(id),
                sender: BOB,
                recipient: to,
                protocol_version: ProtocolVersion(2),
                relationship: handle.relationship,
                content_scope: handle.content_scope,
                sender_certificate: ContentCertificateDigest([0; 32]),
                declarations: bob_writes(),
                message_valid_until: valid_until,
                capability: lane,
            },
            (now, valid_until.0),
        )
        .unwrap();
        self.correspondence_as(BOB, (BOB_KEY, 21), id, Command::Send(Box::new(package)))
    }
    fn bob_says(
        &self,
        to: ProtocolIdentity,
        conversation: u128,
        id: u128,
        text: &str,
        lane: Option<LaneId>,
    ) -> Result<Response, ServiceError> {
        self.advance(1);
        self.gateway.correspondence(&self.bob_message(
            to,
            conversation,
            id,
            vec![DocumentPart::Text(text.into())],
            lane,
        ))
    }
    /// The messages Bob wrote that reached Alice's mailbox.
    fn delivered_to_alice(&self) -> Vec<MessageId> {
        let operation = 7_000_000 + u128::from(self.clock.now().0);
        let Response::Mailbox { entries, .. } = self
            .gateway
            .correspondence(&self.correspondence_as(
                ALICE,
                (ALICE_KEY, 11),
                operation,
                Command::Mailbox {
                    after: 0,
                    limit: 100,
                },
            ))
            .unwrap()
        else {
            panic!("expected a mailbox")
        };
        entries
            .into_iter()
            .filter(|(_, record)| record.author == BOB)
            .map(|(_, record)| record.manifest.message())
            .collect()
    }
    fn reply_lane(&self) -> Option<cs_mail_capabilities::Lane> {
        self.reverse().lane
    }
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn acceptance_and_its_reply_lane_commit_together_or_not_at_all() {
    let fail_reply_lane = |f: &Loop| {
        let reverse = f.gateway.address(BOB, ALICE).unwrap();
        f.sql(&format!(
            "CREATE FUNCTION cs_injected_failure() RETURNS trigger LANGUAGE plpgsql AS \
             $$ BEGIN RAISE EXCEPTION 'injected failure'; END $$; \
             CREATE TRIGGER cs_injected_failure BEFORE INSERT ON cs_capability_lanes \
             FOR EACH ROW WHEN (NEW.aggregate_key = '{}') \
             EXECUTE FUNCTION cs_injected_failure();",
            reverse.aggregate_key()
        ));
    };
    let heal = |f: &Loop| {
        f.sql("DROP TRIGGER cs_injected_failure ON cs_capability_lanes; DROP FUNCTION cs_injected_failure();");
    };
    let unaccepted = |f: &Loop| {
        let snapshot = f.snapshot();
        assert_eq!(
            snapshot.state.relationship.state,
            RelationshipState::Unknown
        );
        assert!(matches!(
            snapshot.state.requests[&RequestId(1)].lifecycle,
            RequestLifecycle::AwaitingRecipientDecision(_)
        ));
        assert!(
            snapshot.payments[&RequestId(1)]
                .refund()
                .operation()
                .is_none()
        );
        let reverse = f.reverse();
        assert_eq!(reverse.state.relationship.state, RelationshipState::Unknown);
        assert!(reverse.lane.is_none());
    };
    let replies_allowed = |f: &Loop| {
        let lane = f.reply_lane().expect("acceptance without its reply lane");
        let grant = lane.grant();
        assert!(matches!(
            f.reverse().state.relationship.state,
            RelationshipState::ExpressLane(_)
        ));
        assert_eq!((grant.sender, grant.recipient), (BOB, ALICE));
        assert_eq!(
            grant.purpose,
            DeclaredPurpose::Known(KnownPurpose::Personal)
        );
        // The operator's published reply lane terms.
        assert_eq!(grant.lifetime, Duration(86_400_000));
        assert_eq!(grant.rate_limit.max_messages, 20);
        assert_eq!(grant.rate_limit.interval, Duration(3_600_000));
        assert!(lane.is_current(f.clock.now()));
    };

    // Standing permission.
    let mut f = Loop::new();
    f.submitted();
    fail_reply_lane(&f);
    assert!(f.send(&f.accept(10)).is_err());
    unaccepted(&f);
    heal(&f);
    f.work();
    assert_eq!(
        f.snapshot().state.relationship.state,
        RelationshipState::Accepted
    );
    replies_allowed(&f);

    // An express lane.
    let mut f = Loop::new();
    f.submitted();
    fail_reply_lane(&f);
    f.advance(1);
    assert!(
        f.gateway
            .grant_lane(&f.lane(86_400_000), IdempotencyKey(20))
            .is_err()
    );
    unaccepted(&f);
    heal(&f);
    f.work();
    assert!(f.snapshot().lane.is_some());
    replies_allowed(&f);
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn a_request_cannot_be_submitted_while_its_sender_blocks_the_recipient() {
    let mut f = Loop::new();
    f.alice_registers_inbox_key();
    let submission = f.prepared_request(&alice(), 1, ALICE_CONVERSATION);
    // The block is received before the submission but not yet applied.
    f.alice_block_received(9_101);
    let outcome = f.submit_request(&alice(), 103, submission.clone());
    assert!(
        matches!(
            &outcome.outcome,
            ReceivedOutcome::Refused(cs_mail_storage_postgres::Refusal::Protocol(
                ProtocolError::ReplyDirectionClosed
            ))
        ),
        "{:?}",
        outcome.outcome
    );
    assert_eq!(
        f.reverse().state.relationship.state,
        RelationshipState::Blocked
    );
    assert!(matches!(
        f.snapshot().state.requests[&RequestId(1)].lifecycle,
        RequestLifecycle::PreparingSubmission(_)
    ));
    assert!(f.delivered_to_alice().is_empty());
    assert!(f.snapshot().messages.is_empty());

    // Unblocked, the same request can be submitted under a new identity.
    f.alice_decides(9_102, |version| ProtocolCommand::UnblockRelationship {
        expected_version: version,
    });
    let ProtocolCommand::SubmitRequestToRecipient {
        content_ref,
        delivery_intent_ref,
        declaration_digest,
        message_valid_until,
        ..
    } = submission
    else {
        panic!("expected a submission")
    };
    let version = f.snapshot().state.requests[&RequestId(1)].version;
    let retried = f.submit_request(
        &alice(),
        9_103,
        ProtocolCommand::SubmitRequestToRecipient {
            request_id: RequestId(1),
            expected_request_version: version.into(),
            content_ref,
            delivery_intent_ref,
            declaration_digest,
            message_valid_until,
        },
    );
    assert!(!refused(&retried), "{:?}", retried.outcome);
    assert_eq!(f.delivered_to_alice(), Vec::<MessageId>::new());
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn a_block_ends_answers_in_the_request_conversation_before_any_later_delivery() {
    let mut f = Loop::new();
    f.alice_registers_inbox_key();
    f.submitted();
    // Bob may answer while the request is pending and after rejecting it.
    assert_eq!(
        f.bob_says(ALICE, ALICE_CONVERSATION, 801, "Who are you?", None)
            .unwrap(),
        Response::Committed(MessageId(801))
    );
    f.bob_decides(ALICE, 11, |version| ProtocolCommand::RejectRelationship {
        expected_version: version,
    });
    assert_eq!(
        f.bob_says(ALICE, ALICE_CONVERSATION, 802, "No, thank you.", None)
            .unwrap(),
        Response::Committed(MessageId(802))
    );
    // Only in the request's own conversation: a new one needs standing permission or a lane.
    assert!(refused_with(
        &f.gateway.correspondence(&f.correspondence_as(
            BOB,
            (BOB_KEY, 21),
            803,
            Command::Create {
                id: ConversationId::new(503).unwrap(),
                other: ALICE,
            },
        )),
        CorrespondenceError::Unauthorized
    ));
    // Alice's block is received and not yet applied when Bob answers again.
    f.alice_block_received(9_201);
    assert!(refused_with(
        &f.bob_says(ALICE, ALICE_CONVERSATION, 804, "Are you sure?", None),
        CorrespondenceError::Unauthorized
    ));
    assert_eq!(
        f.reverse().state.relationship.state,
        RelationshipState::Blocked
    );
    assert_eq!(f.delivered_to_alice(), vec![MessageId(801), MessageId(802)]);
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn revoking_the_recipient_ends_its_answers_and_closes_new_requests() {
    let mut f = Loop::new();
    f.alice_registers_inbox_key();
    f.submitted();
    // Bob accepts with a short express lane; acceptance opens his reply lane.
    f.advance(1);
    f.gateway
        .grant_lane(&f.lane(10_000), IdempotencyKey(20))
        .unwrap();
    let lane = f.reply_lane().unwrap().grant().id;
    f.advance(1);
    f.gateway
        .correspondence(&f.correspondence_as(
            BOB,
            (BOB_KEY, 21),
            812,
            Command::Create {
                id: ConversationId::new(502).unwrap(),
                other: ALICE,
            },
        ))
        .unwrap();
    assert_eq!(
        f.bob_says(ALICE, 502, 813, "Thanks for writing", Some(lane))
            .unwrap(),
        Response::Committed(MessageId(813))
    );
    // Alice upgrades Bob to standing permission, then revokes it.
    f.alice_decides(9_301, |version| ProtocolCommand::AcceptRelationship {
        expected_version: version,
    });
    assert_eq!(
        f.reverse().state.relationship.state,
        RelationshipState::Accepted
    );
    assert_eq!(
        f.bob_says(ALICE, 502, 814, "Standing now", None).unwrap(),
        Response::Committed(MessageId(814))
    );
    f.alice_decides(9_302, |version| ProtocolCommand::RevokeRelationship {
        expected_version: version,
    });
    for (conversation, id, lane) in [
        (502, 815, None),
        (502, 816, Some(lane)),
        (ALICE_CONVERSATION, 817, None),
    ] {
        let answer = f.bob_says(ALICE, conversation, id, "Still there?", lane);
        assert!(
            refused_with(&answer, CorrespondenceError::Unauthorized),
            "{conversation} {lane:?}: {answer:?}"
        );
    }
    assert_eq!(f.delivered_to_alice(), vec![MessageId(813), MessageId(814)]);

    // Bob's lane to Alice lapses; her next request would consent to replies she revoked.
    f.advance(20_000);
    let submission = f.prepared_request(&alice(), 2, 504);
    let outcome = f.submit_request(&alice(), 203, submission);
    assert!(
        matches!(
            &outcome.outcome,
            ReceivedOutcome::Refused(cs_mail_storage_postgres::Refusal::Protocol(
                ProtocolError::ReplyDirectionClosed
            ))
        ),
        "{:?}",
        outcome.outcome
    );
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn request_content_even_when_rejected_stays_within_its_relationship() {
    let mut f = Loop::new();
    f.enroll_carol();
    f.requested(&alice(), 1, ALICE_CONVERSATION);
    f.bob_decides(ALICE, 11, |version| ProtocolCommand::RejectRelationship {
        expected_version: version,
    });
    f.requested(&carol(), 2, CAROL_CONVERSATION);
    f.bob_decides(CAROL, 12, |version| ProtocolCommand::AcceptRelationship {
        expected_version: version,
    });
    // Bob reads Alice's rejected request and excerpts it.
    let Response::Message(view) = f
        .gateway
        .correspondence(&f.correspondence_as(
            BOB,
            (BOB_KEY, 21),
            820,
            Command::Fetch {
                message: MessageId(1),
            },
        ))
        .unwrap()
    else {
        panic!("Bob cannot read the request message")
    };
    let view: MessageView = *view;
    let (secret, public) = content_key(22);
    let document = open(
        &EndpointKeys {
            owner: BOB,
            secret: &secret,
            public,
        },
        &view,
        content_key(12).1,
    )
    .unwrap();
    let excerpt = document
        .quote(
            &view.record.manifest,
            MessageSelection::new(
                MessageId(1),
                view.record.manifest.version(),
                vec![SelectionElement::TextRange {
                    block: 0,
                    start: 0,
                    end: 5,
                }],
            )
            .unwrap(),
        )
        .unwrap();
    let share = |to, conversation, id| {
        f.advance(1);
        f.gateway.correspondence(&f.bob_message(
            to,
            conversation,
            id,
            vec![
                DocumentPart::Text("Look at this".into()),
                DocumentPart::Share(SharedCopy(excerpt.clone())),
            ],
            None,
        ))
    };
    // Within the originating relationship the excerpt may be shared back...
    assert_eq!(
        share(ALICE, ALICE_CONVERSATION, 821).unwrap(),
        Response::Committed(MessageId(821))
    );
    // ...but not with Carol, whom Bob may otherwise answer.
    assert!(refused_with(
        &share(CAROL, CAROL_CONVERSATION, 822),
        CorrespondenceError::PolicyRestricted
    ));
    assert_eq!(
        f.bob_says(CAROL, CAROL_CONVERSATION, 823, "Welcome", None)
            .unwrap(),
        Response::Committed(MessageId(823))
    );
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn a_retried_answer_is_delivered_once_and_an_altered_retry_is_refused() {
    let mut f = Loop::new();
    f.submitted();
    let answer = f.bob_message(
        ALICE,
        ALICE_CONVERSATION,
        830,
        vec![DocumentPart::Text("Tell me more".into())],
        None,
    );
    f.advance(1);
    assert_eq!(
        f.gateway.correspondence(&answer).unwrap(),
        Response::Committed(MessageId(830))
    );
    // The answer is lost; the same bytes arrive again, also through another connection.
    f.advance(1);
    assert_eq!(
        f.gateway.correspondence(&answer).unwrap(),
        Response::Committed(MessageId(830))
    );
    assert_eq!(
        gateway(&f.url, &f.clock).correspondence(&answer).unwrap(),
        Response::Committed(MessageId(830))
    );
    // Other content under the answer's operation identity.
    let mut altered = f.bob_message(
        ALICE,
        ALICE_CONVERSATION,
        831,
        vec![DocumentPart::Text("Something else".into())],
        None,
    );
    altered.operation = answer.operation;
    let altered = altered.sign(&[21; 32]).unwrap();
    assert!(refused_with(
        &f.gateway.correspondence(&altered),
        CorrespondenceError::Conflict
    ));
    assert_eq!(f.delivered_to_alice(), vec![MessageId(830)]);
}

fn counterpart_pending(outcome: &ServiceOutcome, request: u128) -> bool {
    matches!(
        &outcome.outcome,
        ReceivedOutcome::Refused(cs_mail_storage_postgres::Refusal::Protocol(
            ProtocolError::CounterpartRequestPending { request: r }
        )) if *r == RequestId(request)
    )
}
fn full_charge() -> Money {
    Money::from_minor_units(support::FIXTURE_PROCESSING_CHARGE + support::FIXTURE_COLLATERAL)
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn crossed_requests_are_refused_before_any_money_moves() {
    let mut f = Loop::new();
    f.requests_both_ways();
    // Bob holds terms for a request to Alice before her request reaches him.
    let issued = f.terms_for(&bob_to_alice(), 2);
    let ReceivedOutcome::Protocol(issued) = issued.outcome else {
        panic!("terms refused: {:?}", issued.outcome)
    };
    let Some(TermsOutcome::ChargeRequired(terms)) = issued.transition.terms_outcome else {
        panic!("expected a charge")
    };
    // Alice's submission is received ahead of Bob's next steps but not yet applied.
    let submission = f.prepared_request(&alice(), 1, ALICE_CONVERSATION);
    f.received_only(
        (ALICE, BOB),
        &f.signed_in(&alice().signer, (ALICE, BOB), 103, submission),
    );
    let captures = f.processor.captures.len();
    assert!(counterpart_pending(
        &f.create_for(&bob_to_alice(), 2, terms),
        1
    ));
    assert!(matches!(
        f.snapshot().state.requests[&RequestId(1)].lifecycle,
        RequestLifecycle::AwaitingRecipientDecision(_)
    ));
    assert!(counterpart_pending(&f.terms_for(&bob_to_alice(), 3), 1));
    f.work();
    let crossed = f.snapshot_of(BOB, ALICE);
    assert!(crossed.state.requests.is_empty());
    assert!(crossed.payments.is_empty());
    assert_eq!(f.processor.captures.len(), captures);

    // Once Bob has decided Alice's request, he may request her.
    f.bob_decides(ALICE, 11, |version| ProtocolCommand::RejectRelationship {
        expected_version: version,
    });
    let outcome = f.terms_for(&bob_to_alice(), 4);
    assert!(
        matches!(&outcome.outcome, ReceivedOutcome::Protocol(o)
            if matches!(o.transition.terms_outcome, Some(TermsOutcome::ChargeRequired(_)))),
        "{:?}",
        outcome.outcome
    );
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn a_funded_request_overtaken_by_the_other_returns_its_full_charge() {
    let mut f = Loop::new();
    f.requests_both_ways();
    let bobs = f.prepared_request(&bob_to_alice(), 2, 600);
    // Bob's request is funded but unsubmitted, so Alice's own request still goes through.
    f.requested(&alice(), 1, ALICE_CONVERSATION);
    assert!(counterpart_pending(
        &f.submit_request(&bob_to_alice(), 203, bobs),
        1
    ));
    let crossed = f.snapshot_of(BOB, ALICE);
    assert!(
        crossed.state.requests[&RequestId(2)]
            .lifecycle
            .is_preparing_submission()
    );
    assert!(crossed.messages.is_empty());
    let version = crossed.state.requests[&RequestId(2)].version;
    let cancelled = f.submit_in(
        (BOB, ALICE),
        &f.signed_in(
            &bob_to_alice().signer,
            (BOB, ALICE),
            204,
            ProtocolCommand::CancelRequestSubmission {
                request_id: RequestId(2),
                expected_request_version: version.into(),
                reason: cs_mail_protocol::CancellationReason::SenderRequested,
            },
        ),
    );
    assert!(!refused(&cancelled), "{:?}", cancelled.outcome);
    let refund = f.snapshot_of(BOB, ALICE).payments[&RequestId(2)]
        .refund()
        .operation()
        .expect("cancellation without its refund")
        .clone();
    assert_eq!(refund.amount, full_charge());
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn acceptance_cancels_an_unsubmitted_request_back_with_its_lane_or_not_at_all() {
    let mut f = Loop::new();
    f.requests_both_ways();
    let _unsubmitted = f.prepared_request(&bob_to_alice(), 2, 600);
    f.requested(&alice(), 1, ALICE_CONVERSATION);
    let reverse = f.gateway.address(BOB, ALICE).unwrap();
    f.sql(&format!(
        "CREATE FUNCTION cs_injected_failure() RETURNS trigger LANGUAGE plpgsql AS \
         $$ BEGIN RAISE EXCEPTION 'injected failure'; END $$; \
         CREATE TRIGGER cs_injected_failure BEFORE INSERT ON cs_capability_lanes \
         FOR EACH ROW WHEN (NEW.aggregate_key = '{}') \
         EXECUTE FUNCTION cs_injected_failure();",
        reverse.aggregate_key()
    ));
    assert!(f.send(&f.accept(10)).is_err());
    assert_eq!(
        f.snapshot().state.relationship.state,
        RelationshipState::Unknown
    );
    let crossed = f.reverse();
    assert!(crossed.lane.is_none());
    assert!(
        crossed.state.requests[&RequestId(2)]
            .lifecycle
            .is_preparing_submission()
    );
    assert!(
        crossed.payments[&RequestId(2)]
            .refund()
            .operation()
            .is_none()
    );

    f.sql("DROP TRIGGER cs_injected_failure ON cs_capability_lanes; DROP FUNCTION cs_injected_failure();");
    f.work();
    assert_eq!(
        f.snapshot().state.relationship.state,
        RelationshipState::Accepted
    );
    let crossed = f.reverse();
    assert!(
        crossed
            .lane
            .as_ref()
            .is_some_and(|lane| lane.is_current(f.clock.now()))
    );
    assert!(matches!(
        crossed.state.requests[&RequestId(2)].lifecycle,
        RequestLifecycle::Cancelled { .. }
    ));
    assert_eq!(
        crossed.payments[&RequestId(2)]
            .refund()
            .operation()
            .expect("lane without the refund")
            .amount,
        full_charge()
    );
}
