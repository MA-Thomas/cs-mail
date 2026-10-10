//! Local HTTP carrier experiment. No provider account or Internet delivery.
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
pub enum Reply {
    Json(u16, serde_json::Value),
    Disconnect,
    Raw(u16, String),
    Before(Box<dyn FnOnce() + Send>, Box<Reply>),
}
pub struct Carrier {
    pub endpoint: String,
    pub requests: Arc<Mutex<Vec<serde_json::Value>>>,
    replies: Arc<Mutex<VecDeque<Reply>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Default for Carrier {
    fn default() -> Self {
        Self::new()
    }
}
impl Carrier {
    pub fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v3/email/mime", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let replies = Arc::new(Mutex::new(VecDeque::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (recorded, queue, stopped) = (requests.clone(), replies.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::SeqCst) {
                let Ok((mut socket, _)) = listener.accept() else {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                };
                // The listener is nonblocking for shutdown polling, but this fixture
                // reads each accepted request synchronously with a bounded timeout.
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(&mut socket);
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                assert_eq!(first, "POST /v3/email/mime HTTP/1.1\r\n");
                let mut size = 0;
                let mut key_seen = false;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    let lower = line.to_ascii_lowercase();
                    if let Some(length) = lower.strip_prefix("content-length:") {
                        size = length.trim().parse::<usize>().unwrap();
                    }
                    if lower.starts_with("x-smtp2go-api-key:") {
                        key_seen = true;
                    }
                }
                assert!(key_seen && size < 100_000);
                let mut body = vec![0; size];
                reader.read_exact(&mut body).unwrap();
                recorded
                    .lock()
                    .unwrap()
                    .push(serde_json::from_slice(&body).unwrap());
                let mut reply = queue
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("unexpected submission");
                while let Reply::Before(callback, next) = reply {
                    callback();
                    reply = *next;
                }
                let (status, body) = match reply {
                    Reply::Disconnect => continue,
                    Reply::Json(status, body) => (status, body.to_string()),
                    Reply::Raw(status, body) => (status, body),
                    Reply::Before(..) => unreachable!(),
                };
                let redirect = if status == 302 {
                    "Location: http://127.0.0.1:9/never\r\n"
                } else {
                    ""
                };
                let response = format!(
                    "HTTP/1.1 {status} Fixture\r\n{redirect}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes());
            }
        });
        Self {
            endpoint,
            requests,
            replies,
            stop,
            thread: Some(thread),
        }
    }
    pub fn push(&self, reply: Reply) {
        self.replies.lock().unwrap().push_back(reply);
    }
    pub fn count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
    pub fn mime(&self, index: usize) -> Vec<u8> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(
                self.requests.lock().unwrap()[index]["mime_email"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap()
    }
}
impl Drop for Carrier {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}
pub fn accepted(reference: &str) -> Reply {
    Reply::Json(200, serde_json::json!({"data":{"email_id":reference}}))
}
