//! Approved local conjectures: paid acceptance opens exactly one reply route without
//! outbound consent; interrupted transactional delivery cannot send twice or use
//! another message's provider evidence. Violations leak content or mislead guests.
use super::*;
use cs_mail_application::{correspondence::external as app, notices::NoticeRecord};
use cs_mail_correspondence::{ConversationId, external::delivery::DeliveryState};
use cs_mail_primitives::MessageId;
use cs_mail_service::guests::Smtp2goOutbox;
use cs_mail_smtp_gateway::smtp2go::{Feedback, Smtp2go, Webhook};
use support::smtp2go::{Carrier, Reply, accepted};

fn repository(f: &Fixture) -> cs_mail_storage_postgres::external::Repository {
    let clock = f.clock.clone();
    cs_mail_storage_postgres::external::Repository {
        accounts: PostgresDeployment::connect(&f.url)
            .unwrap()
            .accounts(move || clock.now()),
        deployment: f.members.record().clone(),
        deriver: cs_mail_privacy::PrivacyDeriver::new([5; 32], [6; 32]),
        relay_key: cs_mail_content::EndpointSecretKey::from_secret_bytes(
            ContentKeyRef(600),
            &[60; 32],
        )
        .unwrap()
        .1,
    }
}
fn gateway(f: &Fixture, carrier: &Carrier) -> GuestGateway<Clock> {
    GuestGateway::new(
        f.members.clone(),
        [4; 32],
        Arc::new(Smtp2goOutbox {
            carrier: Smtp2go::new(&carrier.endpoint, "api-local-test-credential").unwrap(),
            dkim: support::smtp::dkim("example.test"),
        }),
        Arc::new(cards()),
        cs_mail_smtp_gateway::outbound::RequestSite::new("https://requests.example.test").unwrap(),
    )
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn paid_acceptance_opens_one_reply_route_without_disclosing_member_content() {
    let mut f = Fixture::new();
    let session = f.login("carol@example.test");
    f.send(
        &session,
        "4242424242424242",
        "paid request body stays encrypted",
    );
    f.decide("carol@example.test", true);
    let mut db = f.client();
    db.batch_execute("CREATE FUNCTION refuse_paid_route() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected route failure'; END $$; CREATE TRIGGER fail_paid_route BEFORE INSERT ON cs_guest_request_conversations FOR EACH ROW EXECUTE FUNCTION refuse_paid_route();").unwrap();
    assert!(f.guests.send_notices().is_err());
    assert_eq!(
        db.query_one("SELECT count(*) FROM cs_external_conversations", &[])
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    assert_eq!(
        f.mail
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.thread.is_some())
            .count(),
        0
    );
    db.batch_execute("DROP TRIGGER fail_paid_route ON cs_guest_request_conversations")
        .unwrap();
    f.guests.send_notices().unwrap();
    assert_eq!(f.guests.send_notices().unwrap(), 0);
    let mail = f
        .mail
        .0
        .lock()
        .unwrap()
        .iter()
        .find(|m| m.thread.is_some())
        .unwrap()
        .clone();
    assert!(!mail.body.contains("paid request body"));
    let route = mail.thread.unwrap().reply_to;
    let id = u128::from_str_radix(
        route
            .strip_prefix("reply+")
            .unwrap()
            .split('@')
            .next()
            .unwrap(),
        16,
    )
    .unwrap();
    let record: cs_mail_correspondence::external::ExternalConversation = db
        .query_one(
            "SELECT record FROM cs_external_conversations WHERE id=$1",
            &[&id.to_string()],
        )
        .unwrap()
        .get::<_, postgres::types::Json<_>>(0)
        .0;
    assert!(!record.disclosure().permits_dispatch());
    let repo = repository(&f);
    let p = repo
        .prepare(BOB, &GuestMailbox::parse("carol@example.test").unwrap())
        .unwrap();
    let incoming = support::external::reply(
        &p,
        repo.scope(),
        ConversationId::new(id).unwrap(),
        MessageId(901),
        GuestMailbox::parse("carol@example.test").unwrap(),
        f.clock.now(),
        None,
    );
    repo.incoming().receive(&incoming).unwrap();
    repo.incoming().receive(&incoming).unwrap();
    assert_eq!(
        db.query_one("SELECT count(*) FROM cs_external_messages", &[])
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    let key = f
        .members
        .address(Fixture::identity("carol@example.test"), BOB)
        .unwrap();
    assert_eq!(
        f.members
            .deployment()
            .external_conversations_for(key.aggregate_key())
            .unwrap(),
        vec![ConversationId::new(id).unwrap()]
    );
}

#[test]
#[ignore = "requires isolated PostgreSQL and local HTTP sockets"]
#[allow(clippy::too_many_lines)] // One interruption, concurrent retry, callback and cross-family identity experiment.
fn interrupted_notices_reconcile_once_and_verification_intent_is_atomic() {
    let mut f = Fixture::new();
    let carrier = Carrier::new();
    let mut db = f.client();
    db.batch_execute("CREATE FUNCTION refuse_verification_notice() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected intent failure'; END $$; CREATE TRIGGER fail_notice BEFORE INSERT ON cs_mail_notices FOR EACH ROW EXECUTE FUNCTION refuse_verification_notice();").unwrap();
    assert!(
        gateway(&f, &carrier)
            .begin_verification("carol@example.test")
            .is_err()
    );
    assert_eq!(carrier.count(), 0);
    assert_eq!(
        db.query_one("SELECT count(*) FROM cs_guest_verifications", &[])
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    db.batch_execute("DROP TRIGGER fail_notice ON cs_mail_notices")
        .unwrap();
    carrier.push(accepted("verification-only"));
    let gateway = Arc::new(gateway(&f, &carrier));
    let challenge = gateway.begin_verification("carol@example.test").unwrap();
    let mime = carrier.mime(0);
    let parsed = mail_parser::MessageParser::default().parse(&mime).unwrap();
    let body = parsed.body_text(0).unwrap();
    let code = body
        .split_whitespace()
        .find(|w| w.len() == 7 && w.ends_with('.'))
        .unwrap()
        .trim_end_matches('.');
    let session = gateway.confirm_verification(&challenge, code).unwrap();
    f.send(&session, "4242424242424242", "secret guest request");
    // Provider receives a submitted notice, but its response is lost.
    carrier.push(Reply::Disconnect);
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let second = gateway.clone();
    let wait = barrier.clone();
    let worker = std::thread::spawn(move || {
        wait.wait();
        second.send_notices().unwrap()
    });
    barrier.wait();
    assert_eq!(gateway.send_notices().unwrap() + worker.join().unwrap(), 1);
    assert_eq!(gateway.send_notices().unwrap(), 0);
    assert_eq!(carrier.count(), 2);
    let notice: NoticeRecord = db
        .query_one(
            "SELECT record FROM cs_mail_notices WHERE state ? 'DispatchBegun'",
            &[],
        )
        .unwrap()
        .get::<_, postgres::types::Json<_>>(0)
        .0;
    let wire = serde_json::json!({"id":"notice-event", "email_id":"notice-accepted", "event":"delivered", "time":"2026-10-08 12:00:00", "from_address":"notices@example.test", "rcpt":"carol@example.test", "message-id":format!("<notice-{:032x}@example.test>", notice.id.value())});
    let webhook = Webhook::new(
        [0x42; 32],
        cs_mail_adapters::DomainIdentity::parse_ascii("example.test").unwrap(),
    )
    .unwrap();
    let body = serde_json::to_vec(&wire).unwrap();
    assert!(webhook.authenticate("Bearer wrong", &body).is_err());
    let Feedback::Notice(event) = webhook
        .authenticate(&format!("Bearer {}", "42".repeat(32)), &body)
        .unwrap()
    else {
        panic!("notice feedback");
    };
    let accounts = repository(&f).accounts;
    let mut wrong = event.clone();
    wrong.recipient = GuestMailbox::parse("someone@example.test").unwrap();
    assert!(accounts.notice_feedback(&wrong).is_err());
    db.batch_execute("CREATE FUNCTION refuse_notice_event() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected callback failure'; END $$; CREATE TRIGGER fail_notice_event BEFORE INSERT ON cs_mail_notice_events FOR EACH ROW EXECUTE FUNCTION refuse_notice_event();").unwrap();
    assert!(accounts.notice_feedback(&event).is_err());
    assert!(matches!(
        f.members.deployment().notice_state(notice.id).unwrap(),
        Some(DeliveryState::DispatchBegun { .. })
    ));
    db.batch_execute("DROP TRIGGER fail_notice_event ON cs_mail_notice_events")
        .unwrap();
    accounts.notice_feedback(&event).unwrap();
    accounts.notice_feedback(&event).unwrap();
    assert!(matches!(
        f.members.deployment().notice_state(notice.id).unwrap(),
        Some(DeliveryState::Accepted { .. })
    ));
    wrong = event.clone();
    wrong.kind = cs_mail_correspondence::external::delivery::ProviderEventKind::Rejected;
    assert!(accounts.notice_feedback(&wrong).is_err());
    // A carrier reference already used for a notice cannot complete authored mail.
    let repo = repository(&f);
    // Fixture text uses a different member key; publish that key for this authored send.
    let key = cs_mail_content::EndpointSecretKey::from_secret_bytes(ContentKeyRef(500), &[50; 32])
        .unwrap()
        .1;
    f.members
        .publish_endpoint_key(
            &SignedEndpointKey::sign(
                f.members.scope(),
                BOB_KEY,
                EndpointKeyPublication {
                    persona: BOB,
                    version: 2,
                    key,
                },
                &[21; 32],
            )
            .unwrap(),
        )
        .unwrap();
    let p = repo
        .prepare(BOB, &GuestMailbox::parse("carol@example.test").unwrap())
        .unwrap();
    let account = repo.accounts.persona_account(BOB).unwrap();
    let start = app::SignedCommand {
        scope: repo.scope(),
        product: f.members.record().record().identity.product.clone(),
        account,
        persona: BOB,
        key: BOB_KEY,
        conversation: ConversationId::new(902).unwrap(),
        operation: IdempotencyKey(902),
        command: app::Command::Start {
            mailbox: GuestMailbox::parse("carol@example.test").unwrap(),
            disclose_to_external_mail: true,
            message: Box::new(support::external::text(
                &p,
                ConversationId::new(902).unwrap(),
                MessageId(902),
                BOB,
                f.clock.now(),
            )),
        },
        signature: vec![],
    }
    .sign(&[21; 32])
    .unwrap();
    app::Service(&repo).handle(&start).unwrap();
    repo.begin_dispatch(MessageId(902)).unwrap().unwrap();
    assert!(
        repo.finish_dispatch(MessageId(902), Some(event.reference))
            .is_err()
    );
    assert_eq!(gateway.send_notices().unwrap(), 0);
    assert_eq!(carrier.count(), 2);
}
