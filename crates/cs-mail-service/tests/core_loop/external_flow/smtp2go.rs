//! Agreed conjectures: withdrawal prevents undispatched disclosure; uncertain
//! submissions never automatically repeat; authenticated feedback cannot change
//! another message or partially reconcile delivery. Experiments use real HTTP/PG.
use super::*;
use cs_mail_adapters::DomainIdentity;
use cs_mail_correspondence::external::delivery::{ProviderEvent, ProviderEventKind};
use cs_mail_service::external::{Smtp2goTransport, dispatch};
use cs_mail_smtp_gateway::smtp2go::{Smtp2go, Webhook, member_local, message_id};
use support::smtp2go::{Carrier, Reply, accepted};

fn domain() -> DomainIdentity {
    DomainIdentity::parse_ascii("example.test").unwrap()
}
fn transport(carrier: &Carrier) -> Smtp2goTransport {
    Smtp2goTransport {
        carrier: Smtp2go::new(&carrier.endpoint, "api-local-test-credential").unwrap(),
        dkim: support::smtp::dkim("example.test"),
        domain: domain(),
    }
}
fn secret() -> EndpointSecretKey {
    EndpointSecretKey::from_secret_bytes(ContentKeyRef(600), &[60; 32])
        .unwrap()
        .0
}
fn authorization() -> String {
    format!("Bearer {}", "42".repeat(32))
}
fn wire(id: u128, event_id: &str, reference: &str, kind: &str) -> serde_json::Value {
    serde_json::json!({"id":event_id,"email_id":reference,"event":kind,
        "time":"2026-10-07 15:00:00","from_address":format!("{}@example.test", member_local(BOB)),
        "rcpt":"carol@example.test", "message-id":message_id(MessageId(id), &domain()),
        "auth":"DO-NOT-PERSIST-API-CREDENTIAL", "subject":"DO-NOT-PERSIST-SUBJECT"})
}
fn authenticate(value: &serde_json::Value) -> ProviderEvent {
    let event = Webhook::new([0x42; 32], domain())
        .unwrap()
        .authenticate(&authorization(), &serde_json::to_vec(value).unwrap())
        .unwrap();
    let cs_mail_smtp_gateway::smtp2go::Feedback::Conversation(event) = event else {
        panic!("expected conversation feedback");
    };
    event
}
#[test]
#[ignore = "requires isolated PostgreSQL and local HTTP sockets"]
#[allow(clippy::too_many_lines)] // One handoff/retry experiment, including transport counterexamples.
fn withdrawal_and_uncertain_submission_do_not_disclose_twice() {
    let f = External::new();
    let carrier = Carrier::new();
    let mut sender = transport(&carrier);
    f.start(800);
    app::Service(&f.repository())
        .handle(&f.command(800, 1800, app::Command::WithdrawDisclosure))
        .unwrap();
    assert!(dispatch(&f.repository(), &secret(), MessageId(800), &mut sender).is_err());
    assert_eq!(
        carrier.count(),
        0,
        "withdrawal must prevent any provider request"
    );

    f.start(801);
    carrier.push(accepted("first"));
    let barrier = Arc::new(Barrier::new(2));
    let (left, right) = (f.repository(), f.repository());
    let mut second = transport(&carrier);
    let b = barrier.clone();
    let worker = std::thread::spawn(move || {
        b.wait();
        dispatch(&left, &secret(), MessageId(801), &mut second)
    });
    barrier.wait();
    let outcome = dispatch(&right, &secret(), MessageId(801), &mut sender);
    assert_ne!(outcome.is_ok(), worker.join().unwrap().is_ok());
    assert_eq!(carrier.count(), 1);
    assert!(matches!(
        f.read(801).1[0].delivery,
        Some(DeliveryState::Accepted { .. })
    ));
    assert!(
        f.read(801).1[0].provider_events.is_empty(),
        "API acceptance is not delivery"
    );
    assert_eq!(carrier.requests.lock().unwrap()[0]["fastaccept"], true);
    let mime = carrier.mime(0);
    assert!(String::from_utf8_lossy(&mime).contains("a=rsa-sha256"));
    let parsed = mail_parser::MessageParser::default().parse(&mime).unwrap();
    assert_eq!(parsed.body_text(0).unwrap(), "external fixture 801");
    assert_eq!(
        parsed.message_id(),
        Some("cs-00000000000000000000000000000321@example.test")
    );
    assert_eq!(
        parsed.reply_to().unwrap().first().unwrap().address(),
        Some("reply+00000000000000000000000000000321@example.test")
    );
    // The MIME actually sent carries a verifiable deployment signature.
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        use cs_mail_adapters::{DmarcVerifier, SmtpAuthenticationRequest};
        let (_dns, verifier) = support::smtp::Dns::start().await;
        let original = String::from_utf8(mime.clone()).unwrap();
        let mut altered_body = mime.clone();
        let body_offset = altered_body
            .windows(4)
            .position(|b| b == b"\r\n\r\n")
            .unwrap()
            + 4;
        assert_ne!(altered_body[body_offset], b'A');
        altered_body[body_offset] = b'A';
        for (candidate, expected) in [
            (original.clone(), true),
            (
                original.replace("Subject: cs-mail conversation", "Subject: altered"),
                false,
            ),
            (String::from_utf8(altered_body).unwrap(), false),
        ] {
            let result = verifier
                .verify(SmtpAuthenticationRequest {
                    raw_message: candidate.as_bytes(),
                    remote_ip: "127.0.0.1".parse().unwrap(),
                    helo_identity: "example.test",
                    mail_from: Some("member@example.test"),
                    receiver_hostname: "destination.test",
                    received_at: f.base.clock.now(),
                })
                .await;
            assert_eq!(
                result.is_ok(),
                expected,
                "RSA must authenticate the actual headers and body"
            );
        }
    });

    let package = support::external::text(
        &f.preparation(),
        ConversationId::new(801).unwrap(),
        MessageId(802),
        BOB,
        f.base.clock.now(),
    );
    app::Service(&f.repository())
        .handle(&f.command(801, 1802, app::Command::Send(Box::new(package))))
        .unwrap();
    // The API may include complete accounting as well as its queue reference.
    carrier.push(Reply::Json(
        200,
        serde_json::json!({"data": {
            "email_id":"followup", "succeeded":1, "failed":0, "failures":[]
        }}),
    ));
    dispatch(&f.repository(), &secret(), MessageId(802), &mut sender).unwrap();
    let mime = carrier.mime(1);
    let parsed = mail_parser::MessageParser::default().parse(&mime).unwrap();
    assert!(
        String::from_utf8(mime.clone())
            .unwrap()
            .contains("References: <cs-00000000000000000000000000000321@example.test>")
    );
    assert_eq!(parsed.body_text(0).unwrap(), "external fixture 802");

    // Provider receives the entire request and disconnects before answering.
    f.start(803);
    carrier.push(Reply::Disconnect);
    dispatch(&f.repository(), &secret(), MessageId(803), &mut sender).unwrap();
    drop(sender); // A fresh process/client must not retry the durable unknown outcome.
    let mut sender = transport(&carrier);
    assert!(dispatch(&f.repository(), &secret(), MessageId(803), &mut sender).is_err());
    assert_eq!(carrier.count(), 3);
    assert!(matches!(
        f.read(803).1[0].delivery,
        Some(DeliveryState::DispatchBegun { .. })
    ));
    assert!(
        !f.repository()
            .waiting(100)
            .unwrap()
            .contains(&MessageId(803))
    );

    for (id, reply, failed) in [
        (
            804,
            Reply::Json(
                200,
                serde_json::json!({"data":{"succeeded":0,"failed":1,"failures":[{"error":"refused"}],"email_id":""}}),
            ),
            true,
        ),
        (805, Reply::Raw(200, "not JSON".into()), false),
        (
            806,
            Reply::Json(503, serde_json::json!({"data":{"error_code":"EXCEPTION"}})),
            false,
        ),
        (807, Reply::Raw(302, "{}".into()), false),
        (
            808,
            Reply::Json(
                401,
                serde_json::json!({"data":{"error_code":"UNAUTHORIZED"}}),
            ),
            true,
        ),
        (810, Reply::Json(200, serde_json::json!({"data":{}})), false),
        (
            811,
            Reply::Json(200, serde_json::json!({"data":{"email_id":""}})),
            false,
        ),
        (
            812,
            Reply::Json(
                200,
                serde_json::json!({"data":{"email_id":"contradiction","error_code":"REFUSED"}}),
            ),
            false,
        ),
        (
            813,
            Reply::Json(
                200,
                serde_json::json!({"data":{"email_id":"contradiction","error":"refused"}}),
            ),
            false,
        ),
        (
            814,
            Reply::Json(
                200,
                serde_json::json!({"data":{"email_id":"partial","succeeded":1}}),
            ),
            false,
        ),
        (
            815,
            Reply::Json(
                200,
                serde_json::json!({"data":{"email_id":"contradiction","succeeded":0,"failed":1,"failures":["refused"]}}),
            ),
            false,
        ),
        (
            816,
            Reply::Json(503, serde_json::json!({"data":{"email_id":"server-error"}})),
            false,
        ),
    ] {
        f.start(id);
        carrier.push(reply);
        dispatch(&f.repository(), &secret(), MessageId(id), &mut sender).unwrap();
        assert_eq!(
            matches!(f.read(id).1[0].delivery, Some(DeliveryState::Failed { .. })),
            failed
        );
        if !failed {
            assert!(matches!(
                f.read(id).1[0].delivery,
                Some(DeliveryState::DispatchBegun { .. })
            ));
        }
        assert!(dispatch(&f.repository(), &secret(), MessageId(id), &mut sender).is_err());
    }
    f.start(809);
    let mut db = postgres::Client::connect(&f.base.url, postgres::NoTls).unwrap();
    db.batch_execute("CREATE FUNCTION refuse_handoff_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.state ? 'Accepted' THEN RAISE EXCEPTION 'lost local receipt'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_handoff BEFORE UPDATE ON cs_external_deliveries FOR EACH ROW EXECUTE FUNCTION refuse_handoff_receipt();").unwrap();
    carrier.push(accepted("accepted-before-db-failure"));
    assert!(dispatch(&f.repository(), &secret(), MessageId(809), &mut sender).is_err());
    db.batch_execute("DROP TRIGGER fail_handoff ON cs_external_deliveries")
        .unwrap();
    assert!(matches!(
        f.read(809).1[0].delivery,
        Some(DeliveryState::DispatchBegun { .. })
    ));
    assert!(dispatch(&f.repository(), &secret(), MessageId(809), &mut sender).is_err());
    assert_eq!(
        carrier.count(),
        16,
        "no hidden HTTP retry or redirect may disclose again"
    );
}

#[test]
#[ignore = "requires isolated PostgreSQL and local HTTP sockets"]
#[allow(clippy::too_many_lines)] // One feedback identity/atomicity experiment, including ordering races.
fn feedback_is_authenticated_bound_atomic_and_safe_out_of_order() {
    let f = External::new();
    let carrier = Carrier::new();
    let mut sender = transport(&carrier);
    f.start(900);
    carrier.push(Reply::Disconnect);
    dispatch(&f.repository(), &secret(), MessageId(900), &mut sender).unwrap();
    let body = wire(900, "event-one", "lost-answer", "delivered");
    let webhook = Webhook::new([0x42; 32], domain()).unwrap();
    assert!(
        webhook
            .authenticate("Bearer wrong", &serde_json::to_vec(&body).unwrap())
            .is_err()
    );
    let event = authenticate(&body);
    let repo = f.repository();
    let mut db = postgres::Client::connect(&f.base.url, postgres::NoTls).unwrap();
    db.batch_execute("CREATE FUNCTION refuse_provider_event() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected failure'; END $$; CREATE TRIGGER fail_event BEFORE INSERT ON cs_external_provider_events FOR EACH ROW EXECUTE FUNCTION refuse_provider_event();").unwrap();
    assert!(repo.provider_feedback(&event).is_err());
    assert!(matches!(
        f.read(900).1[0].delivery,
        Some(DeliveryState::DispatchBegun { .. })
    ));
    assert!(f.read(900).1[0].provider_events.is_empty());
    db.batch_execute("DROP TRIGGER fail_event ON cs_external_provider_events")
        .unwrap();
    repo.provider_feedback(&event).unwrap();
    let accepted_state = f.read(900).1[0].delivery.clone();
    f.base.advance(100);
    repo.provider_feedback(&event).unwrap();
    assert_eq!(f.read(900).1[0].provider_events.len(), 1);
    assert_eq!(f.read(900).1[0].delivery, accepted_state);
    let mut altered = event.clone();
    altered.kind = ProviderEventKind::Rejected;
    assert!(
        repo.provider_feedback(&altered).is_err(),
        "event ID cannot be rewritten"
    );

    let processed = authenticate(&wire(900, "event-two", "lost-answer", "processed"));
    repo.provider_feedback(&processed).unwrap();
    let evidence = f.read(900).1[0].provider_events.clone();
    assert!(
        evidence
            .iter()
            .any(|e| e.kind == ProviderEventKind::Delivered),
        "late processed cannot erase delivered evidence"
    );
    let mut wrong = wire(900, "event-wrong", "lost-answer", "delivered");
    wrong["rcpt"] = "other@example.test".into();
    assert!(repo.provider_feedback(&authenticate(&wrong)).is_err());
    let mut wrong = wire(900, "event-wrong", "lost-answer", "delivered");
    wrong["from_address"] = format!("{}@example.test", member_local(ALICE)).into();
    assert!(repo.provider_feedback(&authenticate(&wrong)).is_err());
    assert!(
        repo.provider_feedback(&authenticate(&wire(
            900,
            "event-wrong",
            "different-carrier-mail",
            "delivered"
        )))
        .is_err()
    );
    f.start(901);
    assert!(
        repo.provider_feedback(&authenticate(&wire(
            901,
            "event-wrong",
            "lost-answer",
            "delivered"
        )))
        .is_err(),
        "waiting mail cannot be declared delivered"
    );
    repo.begin_dispatch(MessageId(901)).unwrap().unwrap();
    assert!(
        repo.provider_feedback(&authenticate(&wire(
            901,
            "event-wrong",
            "lost-answer",
            "delivered"
        )))
        .is_err(),
        "one provider mail cannot refer to a second local mail"
    );
    assert!(matches!(
        f.read(901).1[0].delivery,
        Some(DeliveryState::DispatchBegun { .. })
    ));

    // Feedback can arrive before the submission response, without either path
    // overwriting the other's evidence or causing a second send.
    f.start(902);
    let early = authenticate(&wire(902, "early-event", "early", "delivered"));
    let early_repo = f.repository();
    carrier.push(Reply::Before(
        Box::new(move || early_repo.provider_feedback(&early).unwrap()),
        Box::new(accepted("early")),
    ));
    dispatch(&f.repository(), &secret(), MessageId(902), &mut sender).unwrap();
    assert_eq!(f.read(902).1[0].provider_events.len(), 1);
    assert_eq!(carrier.count(), 2);
    let rows = db
        .query("SELECT record::text FROM cs_external_provider_events", &[])
        .unwrap();
    for row in rows {
        let record: String = row.get(0);
        assert!(!record.contains("DO-NOT-PERSIST"));
    }
}
