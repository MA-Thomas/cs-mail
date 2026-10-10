//! Consequential claims: a challenge creates at most one durable session; failed
//! writes consume nothing; lock waits cannot extend authority; restored invalid
//! evidence or a mismatched token cannot authorize a mailbox.
use cs_mail_guests::{
    GuestError, GuestMailbox, SESSION_LIFETIME, SessionTokenDigest, VERIFICATION_LIFETIME,
};
use cs_mail_primitives::CanonicalTime;
use cs_mail_storage_postgres::{PostgresDeployment, StorageError};
use postgres::{Client, NoTls, types::Json};
use serde_json::{Value, json};
use std::sync::{
    Arc, Barrier,
    atomic::{AtomicU64, Ordering},
};

const START: CanonicalTime = CanonicalTime(100);
const SALT: [u8; 32] = [7; 32];
const CODE: &str = "123456";
static NEXT: AtomicU64 = AtomicU64::new(1);

fn database_url() -> String {
    let base = std::env::var("CS_MAIL_TEST_DATABASE_URL").expect("requires isolated PostgreSQL");
    let schema = format!(
        "guest_verification_{}_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    Client::connect(&base, NoTls)
        .unwrap()
        .batch_execute(&format!("CREATE SCHEMA {schema}"))
        .unwrap();
    format!("{base}?options=-csearch_path%3D{schema}")
}
fn token(n: u8) -> SessionTokenDigest {
    SessionTokenDigest::new([n; 32])
}
fn begin(store: &PostgresDeployment, id: &str) {
    store
        .begin_guest_verification(
            &[7; 32],
            id,
            &GuestMailbox::parse("guest@example.test").unwrap(),
            CODE,
            &SALT,
            || START,
        )
        .unwrap();
}
fn record(db: &mut Client, id: &str) -> Value {
    db.query_one(
        "SELECT verification FROM cs_guest_verifications WHERE id=$1",
        &[&id],
    )
    .unwrap()
    .get::<_, Json<Value>>(0)
    .0
}
fn sessions(db: &mut Client) -> i64 {
    db.query_one("SELECT count(*) FROM cs_guest_sessions", &[])
        .unwrap()
        .get(0)
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn competing_confirmations_create_one_session_and_preserve_wrong_attempts() {
    // Two successful confirmations, or lost wrong attempts, would permit duplicate
    // authority or bypass the guessing limit. Independent connections compete.
    let url = database_url();
    let store = PostgresDeployment::connect(&url).unwrap();
    begin(&store, "race");
    assert!(matches!(
        store.confirm_guest_verification("race", "wrong", &SALT, token(9), || START),
        Err(StorageError::Guest(GuestError::WrongCode))
    ));
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = [1, 2]
        .into_iter()
        .map(|n| {
            let store = PostgresDeployment::connect(&url).unwrap();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                (
                    n,
                    store.confirm_guest_verification("race", CODE, &SALT, token(n), || START),
                )
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|(_, r)| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|(_, r)| matches!(r, Err(StorageError::Guest(GuestError::VerificationClosed))))
            .count(),
        1
    );
    let winner = results.iter().find(|(_, r)| r.is_ok()).unwrap().0;
    let reopened = PostgresDeployment::connect(&url).unwrap();
    assert_eq!(
        reopened
            .guest_session(token(winner), || START)
            .unwrap()
            .mailbox()
            .address(),
        "guest@example.test"
    );
    assert!(reopened.guest_session(token(3 - winner), || START).is_err());
    let mut db = Client::connect(&url, NoTls).unwrap();
    assert_eq!(sessions(&mut db), 1);
    assert_eq!(record(&mut db, "race")["attempts"], 1);
    begin(&store, "guesses");
    for _ in 0..cs_mail_guests::VERIFICATION_ATTEMPTS {
        assert!(matches!(
            store.confirm_guest_verification("guesses", "wrong", &SALT, token(3), || START),
            Err(StorageError::Guest(GuestError::WrongCode))
        ));
    }
    assert!(matches!(
        store.confirm_guest_verification("guesses", CODE, &SALT, token(3), || START),
        Err(StorageError::Guest(GuestError::TooManyAttempts))
    ));
    assert_eq!(sessions(&mut db), 1);
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn failed_session_insert_rolls_back_consumption_and_exact_retry_can_finish() {
    // Consuming a valid code without its session strands the guest; leaving a session
    // without consumption allows the same code to grant authority again.
    let url = database_url();
    let store = PostgresDeployment::connect(&url).unwrap();
    begin(&store, "failure");
    let mut db = Client::connect(&url, NoTls).unwrap();
    let before = record(&mut db, "failure");
    db.batch_execute("CREATE FUNCTION fail_session() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected session failure'; END $$; CREATE TRIGGER fail_session BEFORE INSERT ON cs_guest_sessions FOR EACH ROW EXECUTE FUNCTION fail_session()").unwrap();
    assert!(matches!(
        store.confirm_guest_verification("failure", CODE, &SALT, token(1), || START),
        Err(StorageError::Database(_))
    ));
    assert_eq!(record(&mut db, "failure"), before);
    assert_eq!(sessions(&mut db), 0);
    db.batch_execute(
        "DROP TRIGGER fail_session ON cs_guest_sessions; DROP FUNCTION fail_session()",
    )
    .unwrap();
    store
        .confirm_guest_verification("failure", CODE, &SALT, token(1), || START)
        .unwrap();
    assert_eq!(sessions(&mut db), 1);
    assert!(matches!(
        store.confirm_guest_verification("failure", CODE, &SALT, token(1), || START),
        Err(StorageError::Guest(GuestError::VerificationClosed))
    ));
    store.guest_session(token(1), || START).unwrap();
}

fn wait_for_lock(observer: &mut Client, application: &str) {
    for _ in 0..250 {
        if observer.query_one("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE application_name=$1 AND wait_event_type='Lock')", &[&application]).unwrap().get::<_, bool>(0) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("worker did not reach the database lock");
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn expiry_is_checked_after_verification_and_session_read_waits() {
    // A timestamp captured before either wait would let an expired challenge create
    // authority, or return an already expired session as live.
    let url = database_url();
    let store = PostgresDeployment::connect(&url).unwrap();
    begin(&store, "expires");
    begin(&store, "session");
    store
        .confirm_guest_verification("session", CODE, &SALT, token(2), || START)
        .unwrap();
    let mut db = Client::connect(&url, NoTls).unwrap();
    let mut observer = Client::connect(&url, NoTls).unwrap();
    let before = record(&mut db, "expires");
    for confirmation in [true, false] {
        let name = format!("guest_wait_{}_{}", std::process::id(), confirmation);
        let worker =
            PostgresDeployment::connect(&format!("{url}&application_name={name}")).unwrap();
        let mut lock = db.transaction().unwrap();
        if confirmation {
            lock.query_one(
                "SELECT id FROM cs_guest_verifications WHERE id='expires' FOR UPDATE",
                &[],
            )
            .unwrap();
        } else {
            lock.batch_execute("LOCK TABLE cs_guest_sessions IN ACCESS EXCLUSIVE MODE")
                .unwrap();
        }
        let clock = Arc::new(AtomicU64::new(START.0));
        let observed_clock = clock.clone();
        let thread = std::thread::spawn(move || {
            let now = || CanonicalTime(observed_clock.load(Ordering::SeqCst));
            if confirmation {
                worker.confirm_guest_verification("expires", CODE, &SALT, token(1), now)
            } else {
                worker.guest_session(token(2), now)
            }
        });
        wait_for_lock(&mut observer, &name);
        let lifetime = if confirmation {
            VERIFICATION_LIFETIME
        } else {
            SESSION_LIFETIME
        };
        clock.store(START.0 + lifetime.0 + 1, Ordering::SeqCst);
        lock.commit().unwrap();
        let result = thread.join().unwrap();
        assert!(
            matches!(result, Err(StorageError::Guest(error)) if error == if confirmation { GuestError::VerificationClosed } else { GuestError::SessionExpired })
        );
    }
    assert_eq!(record(&mut db, "expires"), before);
    assert_eq!(sessions(&mut db), 1);
}

#[test]
#[ignore = "requires isolated PostgreSQL"]
fn restoration_refuses_invalid_facts_unconsumed_verifications_and_token_substitution() {
    // Successful authorization from corrupt lifetime/attempt facts or a different
    // verification would attach a mailbox to authority it never established.
    let url = database_url();
    let store = PostgresDeployment::connect(&url).unwrap();
    begin(&store, "restore");
    let mut db = Client::connect(&url, NoTls).unwrap();
    let pending = record(&mut db, "restore");
    store
        .confirm_guest_verification("restore", CODE, &SALT, token(1), || START)
        .unwrap();
    let valid = record(&mut db, "restore");
    for (pointer, replacement) in [
        ("/mailbox", json!("invalid")),
        ("/issued_at", json!(u64::MAX)),
        ("/attempts", json!(5)),
        ("/expires_at", json!(u64::MAX)),
        ("/state/Consumed/verified_at", json!(START.0 - 1)),
        ("/state/Consumed/session_expires_at", json!(u64::MAX)),
    ] {
        let mut bad = valid.clone();
        *bad.pointer_mut(pointer).unwrap() = replacement;
        db.execute(
            "UPDATE cs_guest_verifications SET verification=$1 WHERE id='restore'",
            &[&Json(bad)],
        )
        .unwrap();
        assert!(
            matches!(
                store.guest_session(token(1), || START),
                Err(StorageError::Database(_))
            ),
            "{pointer}"
        );
    }
    for bad in [pending, {
        let mut other = valid.clone();
        other["state"]["Consumed"]["token_digest"] = serde_json::to_value([2; 32]).unwrap();
        other
    }] {
        db.execute(
            "UPDATE cs_guest_verifications SET verification=$1 WHERE id='restore'",
            &[&Json(bad)],
        )
        .unwrap();
        assert!(matches!(
            store.guest_session(token(1), || START),
            Err(StorageError::Guest(GuestError::SessionExpired))
        ));
    }
    db.execute(
        "UPDATE cs_guest_verifications SET verification=$1 WHERE id='restore'",
        &[&Json(valid)],
    )
    .unwrap();
    PostgresDeployment::connect(&url)
        .unwrap()
        .guest_session(token(1), || START)
        .unwrap();
    assert!(
        store
            .guest_session(token(1), || CanonicalTime(START.0 - 1))
            .is_err()
    );
}
