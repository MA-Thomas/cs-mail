//! Conjectures: queued authentication cannot outlive permission; an interrupted
//! lane admission cannot leave a message without consumption or consume twice.
use super::*;
use postgres::{Client, NoTls, types::Json};

fn lane_record(db: &mut Client) -> serde_json::Value {
    db.query_one("SELECT lane FROM cs_capability_lanes", &[])
        .unwrap()
        .get::<_, Json<serde_json::Value>>(0)
        .0
}
fn messages(db: &mut Client) -> i64 {
    db.query_one(
        "SELECT count(*) FROM cs_external_messages WHERE inbound_digest IS NOT NULL",
        &[],
    )
    .unwrap()
    .get(0)
}
fn lane_reply(f: &External, id: u128) -> cs_mail_smtp_gateway::incoming::AuthenticatedReply {
    support::external::reply(
        &f.preparation(),
        f.repository().scope(),
        ConversationId::new(1500).unwrap(),
        MessageId(id),
        f.mailbox.clone(),
        f.base.clock.now(),
        Some(LaneId(1501)),
    )
}
fn fixture() -> External {
    let f = External::new();
    f.start(1500);
    f.grant_lane(1501);
    f
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn lane_admission_rolls_back_and_concurrent_redelivery_consumes_once() {
    let f = fixture();
    let reply = lane_reply(&f, 1502);
    let mut db = Client::connect(&f.base.url, NoTls).unwrap();
    let before = lane_record(&mut db);
    db.batch_execute("CREATE FUNCTION fail_lane() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected lane failure'; END $$; CREATE TRIGGER fail_lane BEFORE UPDATE ON cs_capability_lanes FOR EACH ROW EXECUTE FUNCTION fail_lane()").unwrap();
    assert!(matches!(
        f.repository().incoming().receive(&reply),
        Err(StorageError::Database(_))
    ));
    assert_eq!(messages(&mut db), 0);
    assert_eq!(lane_record(&mut db), before);
    db.batch_execute("DROP TRIGGER fail_lane ON cs_capability_lanes; DROP FUNCTION fail_lane()")
        .unwrap();
    // Invalid restoration is an error, not a panic that poisons the connection.
    let repo = f.repository();
    db.execute(
        "UPDATE cs_capability_lanes SET lane=jsonb_set(lane,'{horizon_at}','0')",
        &[],
    )
    .unwrap();
    assert!(matches!(
        repo.incoming().receive(&reply),
        Err(StorageError::Database(_))
    ));
    assert_eq!(messages(&mut db), 0);
    db.execute("UPDATE cs_capability_lanes SET lane=$1", &[&Json(before)])
        .unwrap();
    let other = f.repository();
    let duplicate = lane_reply(&f, 1502);
    let barrier = Arc::new(Barrier::new(2));
    let left = barrier.clone();
    let worker = std::thread::spawn(move || {
        left.wait();
        other.incoming().receive(&duplicate)
    });
    barrier.wait();
    let committed = repo.incoming().receive(&reply).unwrap();
    assert_eq!(worker.join().unwrap().unwrap(), committed);
    assert_eq!(messages(&mut db), 1);
    assert_eq!(lane_record(&mut db)["messages_in_rate_window"], 1);
    assert!(repo.incoming().receive(&lane_reply(&f, 1503)).is_err());
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn prepared_reply_cannot_cross_a_block_or_expiry_during_a_database_wait() {
    let f = fixture();
    let prepared = lane_reply(&f, 1510);
    let version = f.read(1500).0.version;
    f.contact(
        ProtocolCommand::BlockRelationship {
            expected_version: version.into(),
        },
        1511,
    );
    assert!(f.repository().incoming().receive(&prepared).is_err());
    assert_eq!(f.read(1500).1.len(), 1);

    let f = fixture();
    let prepared = lane_reply(&f, 1520);
    let name = format!("smtp_expiry_{}", std::process::id());
    let deployment =
        PostgresDeployment::connect(&format!("{}&application_name={name}", f.base.url)).unwrap();
    let clock = f.base.clock.clone();
    let repo = Repository {
        accounts: deployment.accounts(move || clock.now()),
        deployment: f.base.gateway.record().clone(),
        deriver: cs_mail_privacy::PrivacyDeriver::new([5; 32], [6; 32]).into(),
        relay_key: f.relay,
    };
    let mut db = Client::connect(&f.base.url, NoTls).unwrap();
    let mut observer = Client::connect(&f.base.url, NoTls).unwrap();
    let before = lane_record(&mut observer);
    let mut lock = db.transaction().unwrap();
    lock.query_one("SELECT lane FROM cs_capability_lanes FOR UPDATE", &[])
        .unwrap();
    let worker = std::thread::spawn(move || repo.incoming().receive(&prepared));
    let waiting = (0..250).any(|_| {
        if observer.query_one("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE application_name=$1 AND wait_event_type='Lock')", &[&name]).unwrap().get::<_, bool>(0) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        false
    });
    f.base.advance(1001);
    lock.commit().unwrap();
    let result = worker.join().unwrap();
    assert!(waiting, "worker did not reach database lock");
    assert!(result.is_err());
    assert_eq!(messages(&mut observer), 0);
    assert_eq!(lane_record(&mut observer), before);
}
