//! Conjectures: expiry across external I/O or storage waits cannot authorize new
//! effects or private reads; it cannot erase durable work or duplicate a renewed retry.
use super::*;
use cs_mail_guests::{GuestCommand, GuestSession, SESSION_LIFETIME, SessionTokenDigest};
use cs_mail_protocol::CancellationReason;
use sha2::{Digest, Sha256};

pub(super) fn session(f: &Fixture, token: &str) -> GuestSession {
    let digest = Sha256::new()
        .chain_update(b"cs-mail/guest-session/v1")
        .chain_update(token.as_bytes())
        .finalize()
        .into();
    f.members
        .deployment()
        .guest_session(SessionTokenDigest::new(digest), || f.clock.now())
        .unwrap()
}
fn gateway(f: &Fixture, name: &str) -> (Arc<MemberGateway<Clock>>, GuestGateway<Clock>) {
    let deployment =
        PostgresDeployment::connect(&format!("{}&application_name={name}", f.url)).unwrap();
    let members = Arc::new(
        MemberGateway::new(
            deployment,
            f.members.record().clone(),
            GatewaySecrets {
                provider_seed: [3; 32],
                relationship_secret: [5; 32],
                principal_secret: [6; 32],
            },
            f.clock.clone(),
        )
        .unwrap(),
    );
    let guests = GuestGateway::new(
        members.clone(),
        [4; 32],
        f.mail.clone(),
        Arc::new(cards()),
        cs_mail_smtp_gateway::outbound::RequestSite::new("https://requests.example.test").unwrap(),
    );
    (members, guests)
}
fn wait_for_lock(db: &mut postgres::Client, name: &str) -> bool {
    (0..250).any(|_| {
        let waiting: bool = db.query_one("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE application_name=$1 AND wait_event_type='Lock')", &[&name]).unwrap().get(0);
        if !waiting { std::thread::sleep(std::time::Duration::from_millis(20)); }
        waiting
    })
}
struct DelayedCards {
    entered: std::sync::mpsc::Sender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
}
impl InstrumentTokens for DelayedCards {
    fn tokenize(
        &self,
        instrument: &str,
        holder: &GuestMailbox,
    ) -> Result<([u8; 32], InstrumentFingerprint), String> {
        self.entered.send(()).unwrap();
        self.release.lock().unwrap().recv().unwrap();
        cards().tokenize(instrument, holder)
    }
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn expiry_during_tokenization_leaves_no_request_preparation_effects() {
    let f = Fixture::new();
    let token = f.login("delayed@guest.example");
    let (entered, notice) = std::sync::mpsc::channel();
    let (release, waiting) = std::sync::mpsc::channel();
    let guests = GuestGateway::new(
        f.members.clone(),
        [4; 32],
        f.mail.clone(),
        Arc::new(DelayedCards {
            entered,
            release: Mutex::new(waiting),
        }),
        cs_mail_smtp_gateway::outbound::RequestSite::new("https://requests.example.test").unwrap(),
    );
    let worker = std::thread::spawn(move || {
        guests.request(
            &token,
            cs_mail_primitives::AliasId::new(BOB.0).unwrap(),
            RequestClassId(1),
            "4242424242424242",
            "hello",
            500,
        )
    });
    notice
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    f.advance(SESSION_LIFETIME.0 + 1);
    release.send(()).unwrap();
    assert_eq!(
        guest_error(worker.join().unwrap()),
        GuestError::SessionExpired
    );
    let mut db = f.client();
    for table in [
        "cs_guest_request_inputs",
        "cs_guest_history_links",
        "cs_relationship_aggregates",
        "cs_received_commands",
        "cs_encrypted_content",
    ] {
        assert_eq!(
            db.query_one(&format!("SELECT count(*) FROM {table}"), &[])
                .unwrap()
                .get::<_, i64>(0),
            0,
            "{table}"
        );
    }
    let renewed = f.login("delayed@guest.example");
    f.guests
        .request(
            &renewed,
            cs_mail_primitives::AliasId::new(BOB.0).unwrap(),
            RequestClassId(1),
            "4242424242424242",
            "hello",
            500,
        )
        .unwrap();
    assert_eq!(f.snapshot("delayed@guest.example").state.requests.len(), 1);
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn expiry_across_command_content_and_status_waits_is_rechecked() {
    for action in ["command", "content", "status"] {
        let mut f = Fixture::new();
        let mailbox = "waiting@guest.example";
        let token = f.login(mailbox);
        let (_, answer) = f.prepare(&token, "4242424242424242", "hello").unwrap();
        let verified = session(&f, &token);
        let before = f.snapshot(mailbox);
        let name = format!("guest_action_{}_{}", std::process::id(), action);
        let (members, guests) = gateway(&f, &name);
        let address = f.members.address(Fixture::identity(mailbox), BOB).unwrap();
        let engine = members
            .deployment()
            .existing_relationship(address.aggregate_key())
            .unwrap()
            .unwrap();
        let (content, certificate) = engine
            .guest_content_with_certificates()
            .unwrap()
            .pop()
            .unwrap();
        let command = GuestCommand::authorize(
            &verified,
            &DOMAIN,
            ProtocolCommand::CancelRequestSubmission {
                request_id: answer.request,
                expected_request_version: before.state.requests[&answer.request].version.into(),
                reason: CancellationReason::SenderRequested,
            },
            IdempotencyKey(501),
            f.clock.now(),
        )
        .unwrap();
        let policy = f.members.policy().unwrap();
        let provider = f.members.record().record().provider_key.reference;
        let clock = f.clock.clone();
        let mut db = f.client();
        let mut observer = f.client();
        let mut lock = db.transaction().unwrap();
        if action == "status" {
            lock.batch_execute("LOCK TABLE cs_relationship_aggregates IN ACCESS EXCLUSIVE MODE")
                .unwrap();
        } else {
            lock.query_one(
                "SELECT pg_advisory_xact_lock(hashtext('cs-mail-receipt-order'))",
                &[],
            )
            .unwrap();
        }
        let worker = std::thread::spawn(move || match action {
            "command" => engine
                .receive_guest(&command, provider, DOMAIN, || clock.now(), policy)
                .map(|_| ())
                .map_err(ServiceError::from),
            "content" => engine
                .store_guest_content(
                    &verified,
                    &content,
                    &certificate.unwrap(),
                    || clock.now(),
                    &policy,
                    DOMAIN,
                )
                .map_err(ServiceError::from),
            _ => guests
                .status(&token, cs_mail_primitives::AliasId::new(BOB.0).unwrap())
                .map(|_| ()),
        });
        let waiting = wait_for_lock(&mut observer, &name);
        f.advance(SESSION_LIFETIME.0 + 1);
        lock.commit().unwrap();
        let result = worker.join().unwrap();
        assert!(waiting, "{action} did not wait");
        assert_eq!(guest_error(result), GuestError::SessionExpired, "{action}");
        assert_eq!(f.snapshot(mailbox), before);
    }
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn preparation_checks_expiry_after_reservation_and_history_waits() {
    for table in ["cs_guest_request_inputs", "cs_guest_history_links"] {
        let f = Fixture::new();
        let token = f.login("preparing@guest.example");
        let name = format!("guest_prepare_{}_{}", std::process::id(), table);
        let (_, guests) = gateway(&f, &name);
        let mut db = f.client();
        let mut observer = f.client();
        let mut lock = db.transaction().unwrap();
        lock.batch_execute(&format!("LOCK TABLE {table} IN ACCESS EXCLUSIVE MODE"))
            .unwrap();
        let worker = std::thread::spawn(move || {
            guests.request(
                &token,
                cs_mail_primitives::AliasId::new(BOB.0).unwrap(),
                RequestClassId(1),
                "4242424242424242",
                "hello",
                600,
            )
        });
        // Alias lookup also acquires the receipt barrier. Wait for this exact
        // relation, rather than mistaking another schema's advisory wait for it.
        let waiting = (0..250).any(|_| {
            let waiting: bool = observer.query_one("SELECT EXISTS(SELECT 1 FROM pg_locks l JOIN pg_stat_activity a ON a.pid=l.pid WHERE a.application_name=$1 AND NOT l.granted AND l.relation=to_regclass($2))", &[&name, &table]).unwrap().get(0);
            if !waiting { std::thread::sleep(std::time::Duration::from_millis(20)); }
            waiting
        });
        f.advance(SESSION_LIFETIME.0 + 1);
        lock.commit().unwrap();
        let result = worker.join().unwrap();
        assert!(waiting, "preparation did not reach {table}");
        assert_eq!(guest_error(result), GuestError::SessionExpired);
        for absent in [
            "cs_guest_history_links",
            "cs_relationship_aggregates",
            "cs_received_commands",
        ] {
            assert_eq!(
                observer
                    .query_one(&format!("SELECT count(*) FROM {absent}"), &[])
                    .unwrap()
                    .get::<_, i64>(0),
                0,
                "{absent}"
            );
        }
        // A reservation accepted while the session was live remains a historical fact.
        let reservations: i64 = observer
            .query_one("SELECT count(*) FROM cs_guest_request_inputs", &[])
            .unwrap()
            .get(0);
        assert_eq!(reservations, i64::from(table == "cs_guest_history_links"));
    }
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn accepted_cancellation_and_refund_recover_after_session_expiry() {
    let mut f = Fixture::new();
    let mailbox = "recover@guest.example";
    let token = f.login(mailbox);
    let (_, answer) = f.prepare(&token, "4242424242424242", "hello").unwrap();
    let verified = session(&f, &token);
    let address = f.members.address(Fixture::identity(mailbox), BOB).unwrap();
    let engine = f
        .members
        .deployment()
        .existing_relationship(address.aggregate_key())
        .unwrap()
        .unwrap();
    let before = engine.snapshot().unwrap();
    let body = ProtocolCommand::CancelRequestSubmission {
        request_id: answer.request,
        expected_request_version: before.state.requests[&answer.request].version.into(),
        reason: CancellationReason::SenderRequested,
    };
    let command = GuestCommand::authorize(
        &verified,
        &DOMAIN,
        body.clone(),
        IdempotencyKey(701),
        f.clock.now(),
    )
    .unwrap();
    let provider = f.members.record().record().provider_key.reference;
    let handle = engine
        .receive_guest(
            &command,
            provider,
            DOMAIN,
            || f.clock.now(),
            f.members.policy().unwrap(),
        )
        .unwrap();
    f.advance(SESSION_LIFETIME.0 + 1);
    assert_eq!(
        guest_error(
            f.guests
                .status(&token, cs_mail_primitives::AliasId::new(BOB.0).unwrap())
        ),
        GuestError::SessionExpired
    );
    assert!(!matches!(
        engine.process_received(&handle).unwrap(),
        cs_mail_application::receipts::ReceivedOutcome::Refused(_)
    ));
    f.work();
    let renewed = f.login(mailbox);
    let replay = GuestCommand::authorize(
        &session(&f, &renewed),
        &DOMAIN,
        body,
        IdempotencyKey(701),
        f.clock.now(),
    )
    .unwrap();
    let handle = engine
        .receive_guest(
            &replay,
            provider,
            DOMAIN,
            || f.clock.now(),
            f.members.policy().unwrap(),
        )
        .unwrap();
    assert!(!matches!(
        engine.process_received(&handle).unwrap(),
        cs_mail_application::receipts::ReceivedOutcome::Refused(_)
    ));
    f.work();
    let refunds: Vec<_> = f
        .processor
        .executed
        .iter()
        .filter(|p| matches!(p.kind, PaymentKind::Refund { .. }))
        .collect();
    assert_eq!(refunds.len(), 1);
    let snapshot = engine.snapshot().unwrap();
    assert_eq!(
        refunds[0].destination,
        snapshot.payments[&answer.request].capture().destination
    );
    assert_eq!(snapshot.state.requests.len(), 1);
    assert!(
        f.guests
            .status(&renewed, cs_mail_primitives::AliasId::new(BOB.0).unwrap())
            .unwrap()
            .is_some()
    );
}
