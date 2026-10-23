//! Conjectures: pending access cannot escape its conversation; rejection cannot
//! become a block or settlement; withdrawals serialize with dispatch; interrupted
//! transactions and retries cannot duplicate or partially commit delivery.
use super::*;
use cs_mail_application::correspondence::external as app;
use cs_mail_content::{EndpointPublicKey, EndpointSecretKey};
use cs_mail_correspondence::ConversationId;
use cs_mail_correspondence::external::delivery::DeliveryState;
use cs_mail_guests::GuestMailbox;
use cs_mail_storage_postgres::external::{Preparation, Repository};
use ed25519_dalek::Signer;

struct External {
    base: Loop,
    mailbox: GuestMailbox,
    relay: EndpointPublicKey,
}
impl External {
    fn new() -> Self {
        let base = Loop::new();
        let (_, key) = EndpointSecretKey::from_secret_bytes(ContentKeyRef(500), &[50; 32]).unwrap();
        base.gateway
            .publish_endpoint_key(
                &cs_mail_security::SignedEndpointKey::sign(
                    base.gateway.scope(),
                    BOB_KEY,
                    cs_mail_security::EndpointKeyPublication {
                        persona: BOB,
                        version: 1,
                        key,
                    },
                    &[21; 32],
                )
                .unwrap(),
            )
            .unwrap();
        Self {
            base,
            mailbox: GuestMailbox::parse("carol@example.test").unwrap(),
            relay: EndpointSecretKey::from_secret_bytes(ContentKeyRef(600), &[60; 32])
                .unwrap()
                .1,
        }
    }
    fn repository(&self) -> Repository {
        let deployment = PostgresDeployment::connect(&self.base.url).unwrap();
        let clock = self.base.clock.clone();
        Repository {
            accounts: deployment.accounts(move || clock.now()),
            deployment: self.base.gateway.record().clone(),
            deriver: cs_mail_privacy::PrivacyDeriver::new([5; 32], [6; 32]).into(),
            relay_key: self.relay,
        }
    }
    fn preparation(&self) -> Preparation {
        self.repository().prepare(BOB, &self.mailbox).unwrap()
    }
    fn command(
        &self,
        conversation: u128,
        operation: u128,
        command: app::Command,
    ) -> app::SignedCommand {
        let account = self
            .base
            .gateway
            .deployment()
            .accounts(|| CanonicalTime(0))
            .persona_account(BOB)
            .unwrap();
        app::SignedCommand {
            scope: self.base.gateway.scope(),
            product: "cs-mail/test".into(),
            account,
            persona: BOB,
            key: BOB_KEY,
            conversation: ConversationId::new(conversation).unwrap(),
            operation: IdempotencyKey(operation),
            command,
            signature: vec![],
        }
        .sign(&[21; 32])
        .unwrap()
    }
    fn start_command(&self, id: u128) -> app::SignedCommand {
        self.command(
            id,
            id,
            app::Command::Start {
                receiving_alias: cs_mail_primitives::AliasId::new(BOB.0).unwrap(),
                mailbox: self.mailbox.clone(),
                disclose_to_external_mail: true,
                message: Box::new(support::external::text(
                    &self.preparation(),
                    ConversationId::new(id).unwrap(),
                    MessageId(id),
                    BOB,
                    self.base.clock.now(),
                )),
            },
        )
    }
    fn start(&self, id: u128) {
        app::Service(&self.repository())
            .handle(&self.start_command(id))
            .unwrap();
    }
    fn grant_lane(&self, id: u128) {
        let p = self.preparation();
        let at = self.base.clock.now();
        let grant = SenderScopedGrant {
            id: LaneId(id),
            subject: LaneSubject::GuestMailbox(p.relationship.sender),
            sender: p.relationship.sender,
            recipient: BOB,
            protocol_version: ProtocolVersion(2),
            deployment_domain: [7; 32],
            intended_provider: ProviderRef(30),
            recipient_operational_key: BOB_KEY,
            purpose: DeclaredPurpose::Unspecified,
            origin: None,
            declaration_authority: DeclarationAuthorityConstraint::GuestGateway,
            lifetime: Duration(1000),
            rate_limit: RateLimit {
                max_messages: 1,
                interval: Duration(1000),
            },
            mode: LaneMode::Expiring,
            issued_at: at,
            not_before: at,
            not_after: at.checked_add(Duration(1000)).unwrap(),
            version: Version(1),
        };
        let signature = ed25519_dalek::SigningKey::from_bytes(&[21; 32])
            .sign(&grant.signing_bytes().unwrap())
            .to_bytes();
        self.base
            .gateway
            .grant_lane(&SignedLaneGrant { grant, signature }, IdempotencyKey(id))
            .unwrap();
        let deployment = self.base.gateway.deployment();
        let account = deployment
            .accounts(move || at)
            .persona_account(BOB)
            .unwrap();
        let command = cs_mail_application::aliases::SignedCommand {
            scope: self.base.gateway.scope(),
            product: "cs-mail/test".into(),
            account,
            owner: BOB,
            key: BOB_KEY,
            operation: IdempotencyKey(id + 100_000),
            command: cs_mail_application::aliases::Command::ScopeSender {
                sender: p.relationship.sender,
                expected: 0,
                receiving: cs_mail_capabilities::routes::ReceivingScope::one(
                    cs_mail_primitives::AliasId::new(BOB.0).unwrap(),
                ),
            },
            signature: vec![],
        }
        .sign(&[21; 32])
        .unwrap();
        let result = self
            .base
            .gateway
            .alias_command(&command, "inbox.test")
            .unwrap();
        assert!(matches!(
            result.receipt.outcome,
            cs_mail_application::aliases::Outcome::Scoped(_)
        ));
    }
    fn reply(
        &self,
        conversation: u128,
        id: u128,
    ) -> cs_mail_smtp_gateway::incoming::AuthenticatedReply {
        support::external::reply(
            &self.preparation(),
            self.repository().scope(),
            ConversationId::new(conversation).unwrap(),
            MessageId(id),
            self.mailbox.clone(),
            self.base.clock.now(),
            None,
        )
    }
    fn read(&self, id: u128) -> (cs_mail_protocol::Relationship, Vec<app::MessageView>) {
        match app::Service(&self.repository())
            .handle(&self.command(id, 900_000, app::Command::Read { after: 0 }))
            .unwrap()
        {
            app::Response::Conversation {
                relationship,
                messages,
                ..
            } => (*relationship, messages),
            app::Response::Outcome(_) => panic!("expected conversation"),
        }
    }
    fn contact(&self, command: ProtocolCommand, operation: u128) {
        let p = self.preparation();
        let signed = self
            .base
            .bob
            .sign(
                SigningScope {
                    deployment_domain: [7; 32],
                    intended_provider: ProviderRef(30),
                    relationship: p.relationship.relationship,
                },
                ProtocolVersion(2),
                IdempotencyKey(operation),
                command,
            )
            .unwrap();
        let outcome = self
            .base
            .gateway
            .submit(p.relationship.sender, BOB, &signed)
            .unwrap();
        assert!(
            !matches!(outcome.outcome, ReceivedOutcome::Refused(_)),
            "{outcome:?}"
        );
    }
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn pending_replies_rejection_and_block_have_distinct_authority() {
    let f = External::new();
    f.start(100);
    let repo = f.repository();
    for n in 101..111 {
        repo.incoming().receive(&f.reply(100, n)).unwrap();
    }
    assert_eq!(f.read(100).1.len(), 11);
    let forged = support::external::reply(
        &f.preparation(),
        f.repository().scope(),
        ConversationId::new(100).unwrap(),
        MessageId(111),
        GuestMailbox::parse("other@example.test").unwrap(),
        f.base.clock.now(),
        None,
    );
    assert!(repo.incoming().receive(&forged).is_err());
    assert!(repo.incoming().receive(&f.reply(999, 112)).is_err());
    let before = f.read(100).0;
    app::Service(&repo)
        .handle(&f.command(100, 200, app::Command::Reject))
        .unwrap();
    assert_eq!(
        f.read(100).0,
        before,
        "rejecting an invitation cannot change relationship or paid history"
    );
    assert!(repo.incoming().receive(&f.reply(100, 113)).is_err());
    f.start(201); // Only the member's explicit new invitation reopens pending access.
    repo.incoming().receive(&f.reply(201, 202)).unwrap();
    f.contact(
        ProtocolCommand::BlockRelationship {
            expected_version: before.version.into(),
        },
        203,
    );
    assert!(repo.incoming().receive(&f.reply(201, 204)).is_err());
    let refused = f.start_command(205);
    let response = app::Service(&repo).handle(&refused).unwrap();
    assert!(
        matches!(&response, app::Response::Outcome(a) if matches!(a.receipt.outcome, app::Outcome::Refused(_)))
    );
    assert_eq!(response, app::Service(&repo).handle(&refused).unwrap());
    assert_eq!(
        postgres::Client::connect(&f.base.url, postgres::NoTls)
            .unwrap()
            .query_one("SELECT count(*) FROM cs_external_conversations", &[])
            .unwrap()
            .get::<_, i64>(0),
        2
    );
    f.contact(
        ProtocolCommand::UnblockRelationship {
            expected_version: f.read(201).0.version.into(),
        },
        206,
    );
    assert_eq!(
        response,
        app::Service(&repo).handle(&refused).unwrap(),
        "unblocking cannot reinterpret an original refusal"
    );
    let mut altered = refused.clone();
    altered.command = app::Command::Reject;
    assert!(
        app::Service(&repo)
            .handle(&altered.sign(&[21; 32]).unwrap())
            .is_err()
    );
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn acceptance_and_lane_reuse_existing_permission_without_reopening_pending_access() {
    let f = External::new();
    f.start(300);
    let initial = f.read(300).0;
    f.contact(
        ProtocolCommand::AcceptRelationship {
            expected_version: initial.version.into(),
        },
        301,
    );
    f.repository()
        .incoming()
        .receive(&f.reply(300, 302))
        .unwrap();
    assert!(
        app::Service(&f.repository())
            .handle(&f.command(300, 309, app::Command::Reject))
            .is_ok_and(|r| matches!(r, app::Response::Outcome(a) if matches!(a.receipt.outcome, app::Outcome::Refused(_)))),
        "reject must not pretend to close standing access"
    );
    let accepted = f.read(300).0;
    f.contact(
        ProtocolCommand::RevokeRelationship {
            expected_version: accepted.version.into(),
        },
        303,
    );
    assert!(
        f.repository()
            .incoming()
            .receive(&f.reply(300, 304))
            .is_err()
    );
    // A mailbox lane uses the existing subject binding, validity and rate enforcement.
    let p = f.preparation();
    let at = f.base.clock.now();
    f.grant_lane(305);
    f.start(310); // Sending again must not turn an existing bounded lane into unlimited pending access.
    assert!(
        f.repository()
            .incoming()
            .receive(&f.reply(310, 311))
            .is_err()
    );
    let reply = support::external::reply(
        &p,
        f.repository().scope(),
        ConversationId::new(300).unwrap(),
        MessageId(306),
        f.mailbox.clone(),
        at,
        Some(LaneId(305)),
    );
    f.repository().incoming().receive(&reply).unwrap();
    f.repository().incoming().receive(&reply).unwrap(); // Exact redelivery consumes no second allowance.
    let second = support::external::reply(
        &p,
        f.repository().scope(),
        ConversationId::new(300).unwrap(),
        MessageId(307),
        f.mailbox.clone(),
        at,
        Some(LaneId(305)),
    );
    assert!(f.repository().incoming().receive(&second).is_err());
    f.base.advance(1001);
    assert!(
        f.repository()
            .incoming()
            .receive(&f.reply(300, 308))
            .is_err()
    );
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn withdrawal_and_dispatch_have_one_serialized_boundary() {
    let f = External::new();
    f.start(400);
    let withdrawal = f.command(400, 401, app::Command::WithdrawDisclosure);
    let left = f.repository();
    let right = f.repository();
    let barrier = Arc::new(Barrier::new(2));
    let b = barrier.clone();
    let worker = std::thread::spawn(move || {
        b.wait();
        left.begin_dispatch(MessageId(400))
    });
    barrier.wait();
    app::Service(&right).handle(&withdrawal).unwrap();
    let dispatch = worker.join().unwrap();
    let state = f.read(400).1[0].delivery.clone().unwrap();
    match state {
        DeliveryState::Cancelled { .. } => {
            assert!(dispatch.is_err() || dispatch.unwrap().is_none());
        }
        DeliveryState::DispatchBegun { .. } => assert!(dispatch.unwrap().is_some()),
        other => panic!("unexpected race state: {other:?}"),
    }
    assert!(right.begin_dispatch(MessageId(400)).is_err());
    f.start(402);
    app::Service(&right)
        .handle(&f.command(402, 403, app::Command::WithdrawDisclosure))
        .unwrap();
    assert!(right.begin_dispatch(MessageId(402)).is_err());
    f.start(404);
    let dispatch = right.begin_dispatch(MessageId(404)).unwrap().unwrap();
    let secret = EndpointSecretKey::from_secret_bytes(ContentKeyRef(600), &[60; 32])
        .unwrap()
        .0;
    assert!(cs_mail_service::external::open_dispatch(&dispatch, &secret).is_ok());
    app::Service(&right)
        .handle(&f.command(404, 405, app::Command::WithdrawDisclosure))
        .unwrap();
    right
        .finish_dispatch(
            MessageId(404),
            Some(
                cs_mail_correspondence::external::delivery::ProviderReference::new(
                    "development:404".into(),
                )
                .unwrap(),
            ),
        )
        .unwrap();
    let completed = f.read(404).1[0].delivery.clone();
    assert!(matches!(completed, Some(DeliveryState::Accepted { .. })));
    f.base.advance(1);
    right
        .finish_dispatch(
            MessageId(404),
            Some(
                cs_mail_correspondence::external::delivery::ProviderReference::new(
                    "development:404".into(),
                )
                .unwrap(),
            ),
        )
        .unwrap();
    assert_eq!(
        f.read(404).1[0].delivery,
        completed,
        "reconciliation preserves the original handoff time"
    );
    assert!(right.finish_dispatch(MessageId(404), None).is_err());
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn failed_atomic_start_and_lost_response_retries_do_not_duplicate_delivery() {
    let f = External::new();
    let signed = f.start_command(500);
    let mut db = postgres::Client::connect(&f.base.url, postgres::NoTls).unwrap();
    db.batch_execute("CREATE FUNCTION refuse_external_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected receipt failure'; END $$; CREATE TRIGGER fail_receipt BEFORE INSERT ON cs_external_receipts FOR EACH ROW EXECUTE FUNCTION refuse_external_receipt();").unwrap();
    assert!(app::Service(&f.repository()).handle(&signed).is_err());
    for table in [
        "cs_conversation_routes",
        "cs_external_conversations",
        "cs_external_messages",
        "cs_external_deliveries",
    ] {
        assert_eq!(
            db.query_one(&format!("SELECT count(*) FROM {table}"), &[])
                .unwrap()
                .get::<_, i64>(0),
            0
        );
    }
    db.batch_execute("DROP TRIGGER fail_receipt ON cs_external_receipts")
        .unwrap();
    db.execute(
        "INSERT INTO cs_conversation_routes(id,route) VALUES('500','native')",
        &[],
    )
    .unwrap();
    assert!(
        app::Service(&f.repository()).handle(&signed).is_err(),
        "a native ID cannot also name an external conversation"
    );
    db.execute("DELETE FROM cs_conversation_routes WHERE id='500'", &[])
        .unwrap();
    let first = app::Service(&f.repository()).handle(&signed).unwrap();
    // Missing direction cannot bypass the received-assignment constraint through SQL NULL.
    assert!(
        db.execute(
            "UPDATE cs_external_messages SET record=record-'direction' WHERE id='500'",
            &[]
        )
        .is_err()
    );
    assert_eq!(
        first,
        app::Service(&f.repository()).handle(&signed).unwrap()
    );
    // An actual host signature binds the recorded command and outcome; a substituted
    // body must not resolve a client's original journal even with that signature.
    let host = cs_mail_service::external::ExternalGateway::new(
        Arc::new(gateway(&f.base.url, &f.base.clock)),
        [60; 32],
    );
    let app::Response::Outcome(acknowledgement) = host.external_command(&signed).unwrap() else {
        panic!("expected acknowledgement");
    };
    acknowledgement
        .verify(&signed, &f.base.gateway.signer().verifying_key_bytes())
        .unwrap();
    let mut substituted = acknowledgement;
    substituted.receipt.outcome = app::Outcome::DisclosureGranted;
    assert!(
        substituted
            .verify(&signed, &f.base.gateway.signer().verifying_key_bytes())
            .is_err()
    );
    assert_eq!(f.read(500).1.len(), 1);
    let mut altered = signed.clone();
    altered.command = app::Command::Reject;
    let altered = altered.sign(&[21; 32]).unwrap();
    assert!(app::Service(&f.repository()).handle(&altered).is_err());
    let reply = f.reply(500, 501);
    f.repository().incoming().receive(&reply).unwrap();
    f.repository().incoming().receive(&reply).unwrap();
    assert_eq!(f.read(500).1.len(), 2);
    let altered = support::external::reply_text(
        &f.preparation(),
        f.repository().scope(),
        ConversationId::new(500).unwrap(),
        MessageId(501),
        f.mailbox.clone(),
        f.base.clock.now(),
        None,
        "altered retry",
    );
    assert!(f.repository().incoming().receive(&altered).is_err());
}

#[path = "external_flow/smtp.rs"]
mod smtp;

#[path = "external_flow/smtp2go.rs"]
mod smtp2go;

#[path = "external_flow/hardening.rs"]
mod hardening;
