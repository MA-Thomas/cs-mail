//! Claims agreed for this pass: receive with sending disabled; isolate/authenticate
//! the HTTP surface; acknowledge only committed, once-only provider evidence.
use super::*;
use crate::{Workers, config, external, members};
use cs_mail_service::members::{GatewaySecrets, MemberGateway};
use cs_mail_storage_postgres::PostgresDeployment;
use postgres::{Client, NoTls};
use std::{
    io::Write,
    sync::mpsc,
    time::{Duration, Instant},
};

fn event() -> serde_json::Value {
    serde_json::json!({"id":"callback-one", "email_id":"carrier-one", "event":"delivered",
        "time":"2026-10-07 17:38:18", "from_address":"member-00000000000000000000000000000014@example.test",
        "rcpt":"carol@example.test", "message-id":"<cs-00000000000000000000000000000384@example.test>",
        "auth":"DO-NOT-STORE-CREDENTIAL", "subject":"DO-NOT-STORE-SUBJECT"})
}
fn count(db: &mut Client) -> i64 {
    db.query_one("SELECT count(*) FROM cs_external_provider_events", &[])
        .unwrap()
        .get(0)
}
fn state(db: &mut Client, message: &str) -> serde_json::Value {
    db.query_one(
        "SELECT state FROM cs_external_deliveries WHERE message=$1",
        &[&message],
    )
    .unwrap()
    .get(0)
}

#[test]
#[ignore = "requires isolated PostgreSQL and local HTTP sockets"]
#[allow(clippy::too_many_lines)] // One actual HTTP/commit experiment with adversarial inputs.
fn callbacks_are_isolated_and_acknowledge_only_durable_evidence_without_sending() {
    let base = std::env::var("CS_MAIL_TEST_DATABASE_URL").unwrap();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let schema = format!("cs_callback_{}_{}", std::process::id(), nonce);
    Client::connect(&base, NoTls)
        .unwrap()
        .batch_execute(&format!("CREATE SCHEMA {schema}"))
        .unwrap();
    let url = format!("{base}?options=-csearch_path%3D{schema}");
    let fixture = cs_mail_test_support::external::callback_fixture(&url);
    let deployment = PostgresDeployment::connect(&url).unwrap();
    let member: members::Gateway = Arc::new(
        MemberGateway::new(
            deployment.clone(),
            fixture.deployment.clone(),
            GatewaySecrets {
                provider_seed: [3; 32],
                relationship_secret: [5; 32],
                principal_secret: [6; 32],
            },
            cs_mail_service::UnixMillisecondClock,
        )
        .unwrap(),
    );
    let directory = std::env::temp_dir().join(&schema);
    std::fs::create_dir(&directory).unwrap();
    let content = directory.join("content.json");
    let secret = directory.join("feedback.json");
    // Public fixture secrets, never deployment credentials.
    std::fs::write(&content, serde_json::to_vec(&[60_u8; 32]).unwrap()).unwrap();
    std::fs::write(&secret, serde_json::to_vec(&[66_u8; 32]).unwrap()).unwrap();
    let config = config::ExternalConfig {
        content_secret_file: content,
        mail_domain: "example.test".into(),
        delivery: config::ExternalDelivery::Disabled,
        feedback: config::ExternalFeedback::Smtp2go {
            listen: "127.0.0.1:0".parse().unwrap(),
            secret_file: secret,
        },
    };
    let mut workers = Workers::new();
    let (_gateway, receiver) = external::configure(
        &config,
        member,
        &mut workers,
        config::WorkerConfig::default(),
    )
    .unwrap();
    assert!(
        workers.handles.is_empty(),
        "feedback configuration must not start any sender"
    );
    let receiver = receiver.unwrap();
    let listener = crate::bind(config.feedback.listen().unwrap()).unwrap();
    let address = listener.local_addr().unwrap();
    let endpoint = format!("http://{address}/v1/external/smtp2go/events");
    let (stop, shutdown) = tokio::sync::oneshot::channel::<()>();
    let server = std::thread::spawn(move || {
        let keep_alive = receiver.clone();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            axum::serve(
                tokio::net::TcpListener::from_std(listener).unwrap(),
                receiver.router(),
            )
            .with_graceful_shutdown(async {
                let _ = shutdown.await;
            })
            .await
            .unwrap();
        });
        drop(runtime);
        drop(keep_alive);
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let authorization = format!("Bearer {}", "42".repeat(32));
    let post = |body: &serde_json::Value| {
        client
            .post(&endpoint)
            .header("Authorization", &authorization)
            .json(body)
            .send()
            .unwrap()
            .status()
            .as_u16()
    };
    let mut db = Client::connect(&url, NoTls).unwrap();
    for path in [
        "/ready",
        "/v1/operator/commands",
        "/v1/members/reads",
        "/v1/external/commands",
        "/r/20",
    ] {
        assert_eq!(
            client
                .post(format!("http://{address}{path}"))
                .body("{}")
                .send()
                .unwrap()
                .status(),
            404
        );
    }
    assert_eq!(client.get(&endpoint).send().unwrap().status(), 405);
    assert_eq!(
        client
            .post(&endpoint)
            .json(&event())
            .send()
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .post(&endpoint)
            .header("Authorization", "Bearer wrong")
            .body("not JSON")
            .send()
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .post(&endpoint)
            .header("Authorization", &authorization)
            .header("Authorization", "Bearer wrong")
            .json(&event())
            .send()
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .post(&endpoint)
            .header("Authorization", &authorization)
            .body(vec![b'x'; 65_537])
            .send()
            .unwrap()
            .status(),
        413
    );
    assert_eq!(
        client
            .post(&endpoint)
            .header("Authorization", &authorization)
            .body("not JSON")
            .send()
            .unwrap()
            .status(),
        400
    );
    let mut wrong = event();
    wrong["rcpt"] = "wrong@example.test".into();
    assert_eq!(post(&wrong), 409);
    assert_eq!(count(&mut db), 0);

    db.batch_execute("CREATE FUNCTION refuse_callback() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'DO-NOT-ECHO-DATABASE-DETAIL'; END $$; CREATE TRIGGER fail_callback BEFORE INSERT ON cs_external_provider_events FOR EACH ROW EXECUTE FUNCTION refuse_callback();").unwrap();
    let response = client
        .post(&endpoint)
        .header("Authorization", &authorization)
        .json(&event())
        .send()
        .unwrap();
    assert_eq!(response.status(), 503);
    assert!(!response.text().unwrap().contains("DO-NOT-ECHO"));
    assert_eq!(count(&mut db), 0);
    assert!(state(&mut db, "900").get("DispatchBegun").is_some());
    db.batch_execute("DROP TRIGGER fail_callback ON cs_external_provider_events")
        .unwrap();

    // Hold the delivery row. Observe PostgreSQL actually blocking the HTTP request,
    // then prove no response escapes before releasing the transaction.
    let mut lock_db = Client::connect(&url, NoTls).unwrap();
    let mut transaction = lock_db.transaction().unwrap();
    transaction
        .query_one(
            "SELECT message FROM cs_external_deliveries WHERE message='900' FOR UPDATE",
            &[],
        )
        .unwrap();
    let (sent, result) = mpsc::channel();
    let (target, auth) = (endpoint.clone(), authorization.clone());
    let request = std::thread::spawn(move || {
        let response = reqwest::blocking::Client::new()
            .post(target)
            .header("Authorization", auth)
            .json(&event())
            .send()
            .unwrap();
        sent.send(response.status().as_u16()).unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let blocked: bool = db.query_one("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE cardinality(pg_blocking_pids(pid)) > 0 AND query LIKE '%cs_external_messages m JOIN cs_external_deliveries%')", &[]).unwrap().get(0);
        if blocked {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "HTTP request never reached the protected row"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(matches!(result.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert_eq!(count(&mut db), 0);
    transaction.commit().unwrap();
    assert_eq!(result.recv_timeout(Duration::from_secs(5)).unwrap(), 204);
    request.join().unwrap();
    assert_eq!(count(&mut db), 1);
    assert!(state(&mut db, "900").get("Accepted").is_some());

    // Deliver a second event and close the client without reading the success.
    let mut lost = event();
    lost["id"] = "lost-response".into();
    lost["event"] = "processed".into();
    let body = lost.to_string();
    let mut socket = std::net::TcpStream::connect(address).unwrap();
    write!(socket, "POST /v1/external/smtp2go/events HTTP/1.1\r\nHost: localhost\r\nAuthorization: {authorization}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while count(&mut db) != 2 {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(socket); // No response was consumed.
    assert_eq!(post(&lost), 204);
    assert_eq!(count(&mut db), 2);
    let mut altered = lost;
    altered["event"] = "reject".into();
    assert_eq!(post(&altered), 409);
    assert_eq!(state(&mut db, "901"), "Waiting");
    let records: Vec<String> = db
        .query("SELECT record::text FROM cs_external_provider_events", &[])
        .unwrap()
        .into_iter()
        .map(|r| r.get(0))
        .collect();
    assert!(records.iter().all(|r| !r.contains("DO-NOT-STORE")));
    assert!(records.iter().any(|r| r.contains("Delivered")));
    stop.send(()).unwrap();
    server.join().unwrap();
    workers.stop();
    std::fs::remove_dir_all(directory).unwrap();
}
