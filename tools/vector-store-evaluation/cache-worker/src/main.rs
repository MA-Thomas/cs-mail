//! Isolated native-storage experiment. No member credentials or production API.
use std::{
    ffi::{CString, c_char, c_int, c_void},
    io::{self, Read},
    ptr,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

enum Db {}
enum Stmt {}
unsafe extern "C" {
    fn sqlite3_open_v2(
        p: *const c_char,
        db: *mut *mut Db,
        flags: c_int,
        vfs: *const c_char,
    ) -> c_int;
    fn sqlite3_close(db: *mut Db) -> c_int;
    fn sqlite3_key(db: *mut Db, key: *const c_void, len: c_int) -> c_int;
    fn sqlite3_enable_load_extension(db: *mut Db, on: c_int) -> c_int;
    fn sqlite3_load_extension(
        db: *mut Db,
        path: *const c_char,
        entry: *const c_char,
        error: *mut *mut c_char,
    ) -> c_int;
    fn sqlite3_prepare_v2(
        db: *mut Db,
        sql: *const c_char,
        len: c_int,
        stmt: *mut *mut Stmt,
        tail: *mut *const c_char,
    ) -> c_int;
    fn sqlite3_bind_blob(
        stmt: *mut Stmt,
        at: c_int,
        value: *const c_void,
        len: c_int,
        drop: Option<unsafe extern "C" fn(*mut c_void)>,
    ) -> c_int;
    fn sqlite3_bind_text(
        stmt: *mut Stmt,
        at: c_int,
        value: *const c_char,
        len: c_int,
        drop: Option<unsafe extern "C" fn(*mut c_void)>,
    ) -> c_int;
    fn sqlite3_bind_int64(stmt: *mut Stmt, at: c_int, value: i64) -> c_int;
    fn sqlite3_step(stmt: *mut Stmt) -> c_int;
    fn sqlite3_finalize(stmt: *mut Stmt) -> c_int;
    fn sqlite3_column_int64(stmt: *mut Stmt, col: c_int) -> i64;
    fn sqlite3_column_double(stmt: *mut Stmt, col: c_int) -> f64;
    fn sqlite3_progress_handler(
        db: *mut Db,
        n: c_int,
        callback: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
        context: *mut c_void,
    );
    fn sqlite3_interrupt(db: *mut Db);
    fn sqlite3_db_status(
        db: *mut Db,
        op: c_int,
        current: *mut c_int,
        high: *mut c_int,
        reset: c_int,
    ) -> c_int;
}
fn ok(code: c_int) {
    assert!(matches!(code, 0 | 100 | 101), "SQLite status {code}");
}
fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Lifetime(u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RequestId(u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ticket {
    lifetime: Lifetime,
    request: RequestId,
}
struct State {
    lifetime: Lifetime,
    latest: RequestId,
    closed: bool,
    active: Option<usize>,
}
#[derive(Clone)]
struct Control(Arc<Mutex<State>>);
impl Control {
    fn new(lifetime: Lifetime) -> Self {
        Self(Arc::new(Mutex::new(State {
            lifetime,
            latest: RequestId(0),
            closed: false,
            active: None,
        })))
    }
    fn current(s: &State, t: Ticket) -> bool {
        !s.closed && s.lifetime == t.lifetime && s.latest == t.request
    }
    fn accepts(&self, t: Ticket) -> bool {
        Self::current(&self.0.lock().unwrap(), t)
    }
    fn issue(&self) -> Ticket {
        let mut s = self.0.lock().unwrap();
        assert!(!s.closed);
        s.latest = RequestId(s.latest.0.checked_add(1).unwrap());
        // SQLite permits interrupt from another thread. This mutex also fences
        // connection close; the opaque address is never used for any other API.
        if let Some(p) = s.active {
            unsafe { sqlite3_interrupt(p as *mut Db) };
        }
        Ticket {
            lifetime: s.lifetime,
            request: s.latest,
        }
    }
    fn lock(&self) {
        let mut s = self.0.lock().unwrap();
        s.closed = true;
        if let Some(p) = s.active {
            unsafe { sqlite3_interrupt(p as *mut Db) };
        }
    }
}
struct Connection {
    raw: *mut Db,
    control: Control,
}
impl Connection {
    fn open(path: &str, key: &[u8], extension: &str, cache_kib: usize, control: Control) -> Self {
        let mut raw = ptr::null_mut();
        unsafe {
            ok(sqlite3_open_v2(
                CString::new(path).unwrap().as_ptr(),
                &mut raw,
                2,
                ptr::null(),
            ));
        }
        let conn = Self { raw, control };
        unsafe {
            ok(sqlite3_key(raw, key.as_ptr().cast(), key.len() as c_int));
        }
        conn.exec("SELECT count(*) FROM sqlite_schema");
        unsafe {
            ok(sqlite3_enable_load_extension(raw, 1));
            ok(sqlite3_load_extension(
                raw,
                CString::new(extension).unwrap().as_ptr(),
                ptr::null(),
                ptr::null_mut(),
            ));
            ok(sqlite3_enable_load_extension(raw, 0));
        }
        for sql in [
            "PRAGMA temp_store=MEMORY",
            "PRAGMA secure_delete=ON",
            "PRAGMA synchronous=FULL",
            "PRAGMA mmap_size=0",
            &format!("PRAGMA cache_size=-{cache_kib}"),
        ] {
            conn.exec(sql);
        }
        conn
    }
    fn try_prepare(&self, sql: &str) -> Result<Statement, i32> {
        let mut raw = ptr::null_mut();
        let code = unsafe {
            sqlite3_prepare_v2(
                self.raw,
                CString::new(sql).unwrap().as_ptr(),
                -1,
                &mut raw,
                ptr::null_mut(),
            )
        };
        if code == 0 {
            Ok(Statement(raw))
        } else {
            if !raw.is_null() {
                drop(Statement(raw));
            }
            Err(code)
        }
    }
    fn prepare(&self, sql: &str) -> Statement {
        self.try_prepare(sql)
            .unwrap_or_else(|code| panic!("SQLite prepare status {code}"))
    }
    fn exec(&self, sql: &str) {
        let s = self.prepare(sql);
        loop {
            let code = unsafe { sqlite3_step(s.0) };
            ok(code);
            if code == 101 {
                break;
            }
        }
    }
    fn stat(&self, op: i32, reset: bool) -> i32 {
        let (mut current, mut high) = (0, 0);
        unsafe {
            ok(sqlite3_db_status(
                self.raw,
                op,
                &mut current,
                &mut high,
                i32::from(reset),
            ));
        }
        current
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        let mut s = self.control.0.lock().unwrap();
        s.active = None;
        unsafe {
            ok(sqlite3_close(self.raw));
        }
    }
}
struct Statement(*mut Stmt);
impl Drop for Statement {
    fn drop(&mut self) {
        unsafe {
            sqlite3_finalize(self.0);
        }
    }
}
struct Pause {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}
enum Mode {
    Vector,
    Paused(Pause),
    Expensive(mpsc::Sender<()>),
    Deadline,
}
struct Command {
    ticket: Ticket,
    filtered: bool,
    vector: Vec<u8>,
    mode: Mode,
}
struct Outcome {
    ticket: Ticket,
    rows: Vec<(i64, f64)>,
    code: i32,
    started: bool,
    milliseconds: f64,
    cache: i32,
    hits: i32,
    misses: i32,
}
struct Progress {
    control: Control,
    ticket: Ticket,
    until: Instant,
    pause: Option<Pause>,
    entered: Option<mpsc::Sender<()>>,
}
unsafe extern "C" fn progress(context: *mut c_void) -> c_int {
    // Context and callback are confined to the owning worker's sqlite3_step.
    let p = unsafe { &mut *context.cast::<Progress>() };
    if let Some(entered) = p.entered.take() {
        if entered.send(()).is_err() {
            return 1;
        }
    }
    if let Some(pause) = p.pause.take() {
        if pause.entered.send(()).is_err()
            || pause.release.recv_timeout(Duration::from_secs(10)).is_err()
        {
            return 1;
        }
    }
    i32::from(Instant::now() >= p.until || !p.control.accepts(p.ticket))
}
struct Worker {
    control: Control,
    input: mpsc::SyncSender<Option<Command>>,
    output: mpsc::Receiver<Outcome>,
    join: thread::JoinHandle<()>,
}
impl Worker {
    fn start(
        path: String,
        key: Vec<u8>,
        extension: String,
        cache: usize,
        profile: String,
        metadata: bool,
        sqls: [String; 2],
        lifetime: Lifetime,
    ) -> Self {
        let control = Control::new(lifetime);
        let other = control.clone();
        let (input, commands) = mpsc::sync_channel::<Option<Command>>(1);
        let (results, output) = mpsc::channel();
        let (ready, opened) = mpsc::channel();
        let join = thread::spawn(move || {
            let conn = Connection::open(&path, &key, &extension, cache, other.clone());
            ready.send(()).unwrap();
            while let Some(command) = commands.recv().unwrap() {
                let start = Instant::now();
                {
                    let mut s = other.0.lock().unwrap();
                    if !Control::current(&s, command.ticket) {
                        results
                            .send(Outcome {
                                ticket: command.ticket,
                                rows: vec![],
                                code: 0,
                                started: false,
                                milliseconds: 0.,
                                cache: 0,
                                hits: 0,
                                misses: 0,
                            })
                            .unwrap();
                        continue;
                    }
                    s.active = Some(conn.raw as usize);
                }
                let sql = match command.mode {
                    Mode::Expensive(_) | Mode::Deadline => {
                        "WITH RECURSIVE n(x) AS (VALUES(0) UNION ALL SELECT x+1 FROM n WHERE x<1000000000) SELECT sum(x),0 FROM n"
                    }
                    _ => &sqls[usize::from(command.filtered)],
                };
                let stmt = match conn.try_prepare(sql) {
                    Ok(stmt) => stmt,
                    Err(code) => {
                        assert_eq!(code, 9, "Unexpected prepare failure");
                        other.0.lock().unwrap().active = None;
                        results
                            .send(Outcome {
                                ticket: command.ticket,
                                rows: vec![],
                                code,
                                started: true,
                                milliseconds: start.elapsed().as_secs_f64() * 1000.,
                                cache: conn.stat(1, false),
                                hits: 0,
                                misses: 0,
                            })
                            .unwrap();
                        continue;
                    }
                };
                if !matches!(command.mode, Mode::Expensive(_) | Mode::Deadline) {
                    // SQLITE_STATIC is valid: command/profile/vector live through
                    // finalization. No pointers escape the owning worker.
                    unsafe {
                        ok(sqlite3_bind_blob(
                            stmt.0,
                            1,
                            command.vector.as_ptr().cast(),
                            command.vector.len() as c_int,
                            None,
                        ));
                        let mut at = 2;
                        for _ in 0..if metadata { 2 } else { 1 } {
                            ok(sqlite3_bind_text(
                                stmt.0,
                                at,
                                profile.as_ptr().cast(),
                                profile.len() as c_int,
                                None,
                            ));
                            at += 1;
                            ok(sqlite3_bind_text(
                                stmt.0,
                                at,
                                c"reference".as_ptr(),
                                9,
                                None,
                            ));
                            at += 1;
                            ok(sqlite3_bind_int64(stmt.0, at, 1000));
                            at += 1;
                            if command.filtered {
                                ok(sqlite3_bind_text(stmt.0, at, c"7".as_ptr(), 1, None));
                                at += 1;
                            }
                        }
                    }
                }
                let until = start
                    + if matches!(command.mode, Mode::Deadline) {
                        Duration::from_millis(10)
                    } else {
                        Duration::from_secs(2)
                    };
                let (pause, entered) = match command.mode {
                    Mode::Paused(pause) => (Some(pause), None),
                    Mode::Expensive(entered) => (None, Some(entered)),
                    _ => (None, None),
                };
                let mut context = Progress {
                    control: other.clone(),
                    ticket: command.ticket,
                    until,
                    pause,
                    entered,
                };
                conn.stat(7, true);
                conn.stat(8, true);
                unsafe {
                    sqlite3_progress_handler(
                        conn.raw,
                        1000,
                        Some(progress),
                        (&mut context as *mut Progress).cast(),
                    );
                }
                let mut rows = Vec::new();
                let code = loop {
                    let code = unsafe { sqlite3_step(stmt.0) };
                    if code != 100 {
                        break code;
                    }
                    rows.push(unsafe {
                        (
                            sqlite3_column_int64(stmt.0, 0),
                            sqlite3_column_double(stmt.0, 1),
                        )
                    });
                };
                unsafe {
                    sqlite3_progress_handler(conn.raw, 0, None, ptr::null_mut());
                }
                drop(stmt);
                {
                    let mut s = other.0.lock().unwrap();
                    s.active = None;
                    if !Control::current(&s, command.ticket) {
                        rows.clear();
                    }
                }
                results
                    .send(Outcome {
                        ticket: command.ticket,
                        rows,
                        code,
                        started: true,
                        milliseconds: start.elapsed().as_secs_f64() * 1000.,
                        cache: conn.stat(1, false),
                        hits: conn.stat(7, false),
                        misses: conn.stat(8, false),
                    })
                    .unwrap();
            }
        });
        opened.recv_timeout(Duration::from_secs(30)).unwrap();
        Self {
            control,
            input,
            output,
            join,
        }
    }
    fn send(&self, ticket: Ticket, filtered: bool, vector: &[u8], mode: Mode) {
        self.input
            .send(Some(Command {
                ticket,
                filtered,
                vector: vector.to_vec(),
                mode,
            }))
            .unwrap();
    }
    fn receive(&self) -> Outcome {
        let mut result = self.output.recv_timeout(Duration::from_secs(30)).unwrap();
        if !self.control.accepts(result.ticket) {
            result.rows.clear();
        }
        result
    }
    fn query(&self, filtered: bool, vector: &[u8], mode: Mode) -> Outcome {
        let t = self.control.issue();
        self.send(t, filtered, vector, mode);
        self.receive()
    }
    fn stop(self) {
        self.control.lock();
        self.input.send(None).unwrap();
        self.join.join().unwrap();
    }
}
fn take_string(input: &mut &[u8]) -> String {
    let n = u32::from_le_bytes(input[..4].try_into().unwrap()) as usize;
    *input = &input[4..];
    let value = String::from_utf8(input[..n].to_vec()).unwrap();
    *input = &input[n..];
    value
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 6);
    let cache: usize = args[3].parse().unwrap();
    assert!([2048, 65536, 262144, 524288].contains(&cache));
    let mut bytes = Vec::new();
    io::stdin().read_to_end(&mut bytes).unwrap();
    let key = bytes[..32].to_vec();
    let mut input = &bytes[32..];
    let sqls = [take_string(&mut input), take_string(&mut input)];
    assert_eq!(input.len(), 12 * 512 * 4);
    let vectors: Vec<_> = input.chunks_exact(512 * 4).map(Vec::from).collect();
    let start_worker = |life| {
        Worker::start(
            args[1].clone(),
            key.clone(),
            args[2].clone(),
            cache,
            args[4].clone(),
            args[5] == "vec1",
            sqls.clone(),
            life,
        )
    };
    let worker = start_worker(Lifetime(1));
    print!(
        "{{\"cache_kib\":{cache},\"sql\":[{},{}],\"scopes\":[",
        json_string(&sqls[0]),
        json_string(&sqls[1])
    );
    for filtered in [false, true] {
        if filtered {
            print!(",");
        }
        let warm = worker.query(filtered, &vectors[0], Mode::Vector);
        assert_eq!(warm.code, 101);
        print!(
            "{{\"filtered\":{filtered},\"first_query_ms\":{},\"measurements\":[",
            warm.milliseconds
        );
        for repeat in 0..3 {
            for (i, vector) in vectors.iter().enumerate() {
                let begin = Instant::now();
                let out = worker.query(filtered, vector, Mode::Vector);
                assert_eq!(out.code, 101);
                assert_eq!(out.rows.len(), 10);
                if repeat != 0 || i != 0 {
                    print!(",");
                }
                print!(
                    "{{\"ms\":{},\"roundtrip_ms\":{},\"cache_bytes\":{},\"hits\":{},\"misses\":{},\"rows\":[",
                    out.milliseconds,
                    begin.elapsed().as_secs_f64() * 1000.,
                    out.cache,
                    out.hits,
                    out.misses
                );
                for (j, (id, distance)) in out.rows.iter().enumerate() {
                    if j != 0 {
                        print!(",");
                    }
                    print!("[{id},{distance}]");
                }
                print!("]}}");
            }
        }
        print!("]}}");
    }
    // Deadline interrupts a computation, yet leaves this live worker usable.
    let deadline = worker.query(false, &vectors[0], Mode::Deadline);
    assert_eq!(deadline.code, 9);
    assert!(deadline.rows.is_empty());
    assert_eq!(
        worker.query(false, &vectors[0], Mode::Vector).rows.len(),
        10
    );
    // Completed-but-unconsumed results must be rejected after replacement.
    let completed = worker.control.issue();
    worker.send(completed, false, &vectors[0], Mode::Vector);
    let mut waiting = worker.output.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!(waiting.rows.len(), 10);
    let replacement = worker.control.issue();
    if !worker.control.accepts(waiting.ticket) {
        waiting.rows.clear();
    }
    assert!(waiting.rows.is_empty());
    worker.send(replacement, false, &vectors[0], Mode::Vector);
    assert_eq!(worker.receive().rows.len(), 10);
    // Pause a real vector sqlite3_step, enqueue its replacement, then lock.
    let (entered_tx, entered) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let active = worker.control.issue();
    worker.send(
        active,
        false,
        &vectors[0],
        Mode::Paused(Pause {
            entered: entered_tx,
            release: released,
        }),
    );
    entered.recv_timeout(Duration::from_secs(10)).unwrap();
    let queued = worker.control.issue();
    worker.send(queued, true, &vectors[0], Mode::Vector);
    let begin = Instant::now();
    worker.control.lock();
    let invalidate_ms = begin.elapsed().as_secs_f64() * 1000.;
    release.send(()).unwrap();
    let active_out = worker.receive();
    let queued_out = worker.receive();
    let drain_ms = begin.elapsed().as_secs_f64() * 1000.;
    assert_eq!(active_out.code, 9);
    assert!(active_out.rows.is_empty());
    assert!(!queued_out.started);
    assert!(queued_out.rows.is_empty());
    assert!(invalidate_ms < 50.);
    worker.stop();
    let next = start_worker(Lifetime(2));
    assert!(!next.control.accepts(queued));
    assert_eq!(next.query(false, &vectors[0], Mode::Vector).rows.len(), 10);
    // Actual unpaused recursive work: interrupt from the controller while stepping.
    let ticket = next.control.issue();
    let (entered_tx, entered) = mpsc::channel();
    next.send(ticket, false, &vectors[0], Mode::Expensive(entered_tx));
    entered.recv_timeout(Duration::from_secs(10)).unwrap();
    let begin = Instant::now();
    next.control.lock();
    let recursive_invalidate_ms = begin.elapsed().as_secs_f64() * 1000.;
    let recursive = next.receive();
    let recursive_drain_ms = begin.elapsed().as_secs_f64() * 1000.;
    assert_eq!(recursive.code, 9);
    assert!(recursive.rows.is_empty());
    next.stop();
    println!(
        "],\"cancellation\":{{\"deadline_code\":{},\"deadline_ms\":{},\"completed_result_suppressed\":true,\"replacement_query_succeeded\":true,\"vector_interrupt_code\":{},\"queued_sql_started\":false,\"old_ticket_refused_after_reopen\":true,\"reopened_query_succeeded\":true,\"invalidate_ms\":{},\"drain_ms\":{},\"recursive_interrupt_code\":{},\"recursive_invalidate_ms\":{},\"recursive_drain_ms\":{}}}}}",
        deadline.code,
        deadline.milliseconds,
        active_out.code,
        invalidate_ms,
        drain_ms,
        recursive.code,
        recursive_invalidate_ms,
        recursive_drain_ms
    );
}
