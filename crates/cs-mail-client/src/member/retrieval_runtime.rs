//! Infrastructure adapter: bounded pipe frames; no source authority or key-custody decisions.
use super::{Error, SearchRequestId};
use cs_mail_search::CompatibilityProfile;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::{
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use zeroize::Zeroizing;

const FRAME_LIMIT: usize = 64 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    files: std::collections::BTreeMap<String, String>,
    profile: CompatibilityProfile,
}
pub(super) struct Runtime {
    resources: PathBuf,
    profile: CompatibilityProfile,
    io: Mutex<Option<Session>>,
    child: Arc<Mutex<Option<ChildControl>>>,
    epoch: Arc<AtomicU64>,
    #[cfg(test)]
    fixture: Option<Mutex<std::collections::BTreeMap<u64, Value>>>,
}
struct ChildControl {
    epoch: u64,
    query: Option<SearchRequestId>,
    process: Child,
}
struct Session {
    epoch: u64,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    directory: PathBuf,
}
/// A dispatched query is distinct from its unlocked lifetime.
#[derive(Clone, Copy)]
pub(super) struct QueryFence<'a> {
    pub id: SearchRequestId,
    pub revision: &'a AtomicU64,
}
impl QueryFence<'_> {
    pub fn check(self) -> Result<(), Error> {
        if self.revision.load(Ordering::SeqCst) == self.id.value() {
            Ok(())
        } else {
            Err(Error::SearchSuperseded)
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
impl Runtime {
    pub fn new(resources: &Path, epoch: Arc<AtomicU64>) -> Result<Self, Error> {
        let manifest: Manifest = serde_json::from_slice(
            &std::fs::read(resources.join("manifest.json")).map_err(|_| Error::Unavailable)?,
        )
        .map_err(|_| Error::InvalidData)?;
        for (relative, expected) in &manifest.files {
            let relative = Path::new(relative);
            if relative.is_absolute()
                || relative
                    .components()
                    .any(|p| matches!(p, std::path::Component::ParentDir))
            {
                return Err(Error::InvalidData);
            }
            let mut file =
                std::fs::File::open(resources.join(relative)).map_err(|_| Error::Unavailable)?;
            let mut hash = Sha256::new();
            let mut buffer = vec![0; 65536];
            loop {
                let n = file.read(&mut buffer).map_err(|_| Error::Unavailable)?;
                if n == 0 {
                    break;
                }
                hash.update(&buffer[..n]);
            }
            let mut actual = String::with_capacity(64);
            for byte in hash.finalize() {
                write!(&mut actual, "{byte:02x}").map_err(|_| Error::Unavailable)?;
            }
            if &actual != expected {
                return Err(Error::InvalidData);
            }
        }
        Ok(Self {
            resources: resources.to_owned(),
            profile: manifest.profile,
            io: Mutex::new(None),
            child: Arc::new(Mutex::new(None)),
            epoch,
            #[cfg(test)]
            fixture: None,
        })
    }
    pub fn profile(&self) -> &CompatibilityProfile {
        &self.profile
    }
    /// Interrupt without acquiring the pipe/index mutex. Old replies cannot be consumed.
    pub fn interrupt(&self) {
        self.interrupt_before(u64::MAX);
    }
    pub fn interrupt_before(&self, next: u64) {
        if let Ok(mut child) = self.child.lock() {
            if let Some(ChildControl {
                epoch,
                process: child,
                ..
            }) = child.as_mut()
            {
                if *epoch < next {
                    let _ = child.kill();
                }
            }
        }
        if let Ok(mut io) = self.io.try_lock() {
            if io.as_ref().is_some_and(|s| s.epoch < next) {
                *io = None;
            }
        }
    }
    /// Only a search that has entered the pipe owns cancellation of that child.
    /// A delayed cancellation cannot terminate a newer search or background turn.
    pub fn supersede_search(&self, next: SearchRequestId) {
        if let Ok(mut child) = self.child.lock() {
            if let Some(ChildControl {
                query: Some(active),
                process,
                ..
            }) = child.as_mut()
            {
                if *active < next {
                    let _ = process.kill();
                }
            }
        }
    }
    pub fn request<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        epoch: u64,
        key: &[u8; 32],
        request: &T,
    ) -> Result<R, Error> {
        self.request_scoped(epoch, key, request, None)
    }
    pub fn request_query<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        epoch: u64,
        key: &[u8; 32],
        request: &T,
        fence: QueryFence<'_>,
    ) -> Result<R, Error> {
        self.request_scoped(epoch, key, request, Some(fence))
    }
    fn request_scoped<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        epoch: u64,
        key: &[u8; 32],
        request: &T,
        query: Option<QueryFence<'_>>,
    ) -> Result<R, Error> {
        let check = || {
            if self.epoch.load(Ordering::SeqCst) != epoch || epoch % 2 != 1 {
                return Err(Error::Unavailable);
            }
            if let Some(query) = query {
                query.check()?;
            }
            Ok(())
        };
        check()?;
        let mut io = self.io.lock().map_err(|_| Error::Unavailable)?;
        // Check after queueing, before initialization, upsert or inference.
        check()?;
        #[cfg(test)]
        if self.fixture.is_some() {
            return serde_json::from_value(self.fixture_request(
                &serde_json::to_value(request).map_err(|_| Error::InvalidData)?,
            )?)
            .map_err(|_| Error::InvalidData);
        }
        if io.as_ref().is_some_and(|s| s.epoch != epoch) {
            *io = None;
        }
        let result = (|| {
            let mut control = self.child.lock().map_err(|_| Error::Unavailable)?;
            check()?;
            if io.is_none() {
                if let Some(ChildControl {
                    process: mut old, ..
                }) = control.take()
                {
                    let _ = old.kill();
                    let _ = old.wait();
                }
                let mut nonce = [0; 16];
                getrandom::fill(&mut nonce).map_err(|_| Error::Unavailable)?;
                let directory = std::env::temp_dir()
                    .join(format!("cs-mail-retrieval-{}", u128::from_be_bytes(nonce)));
                std::fs::create_dir(&directory).map_err(|_| Error::StorageUnavailable)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
                        .map_err(|_| Error::StorageUnavailable)?;
                }
                let mut child = Command::new(self.resources.join("python/bin/python3"))
                    .arg("-I")
                    .arg("-B")
                    .arg(self.resources.join("helper.py"))
                    .env("PYTHONDONTWRITEBYTECODE", "1")
                    .env("HF_HUB_OFFLINE", "1")
                    .env("TRANSFORMERS_OFFLINE", "1")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|_| {
                        let _ = std::fs::remove_dir_all(&directory);
                        Error::RetrievalUnavailable
                    })?;
                let input = child.stdin.take().ok_or(Error::RetrievalUnavailable)?;
                let output =
                    BufReader::new(child.stdout.take().ok_or(Error::RetrievalUnavailable)?);
                *control = Some(ChildControl {
                    epoch,
                    query: query.map(|q| q.id),
                    process: child,
                });
                drop(control);
                *io = Some(Session {
                    epoch,
                    input,
                    output,
                    directory,
                });
                let session = io.as_mut().ok_or(Error::Unavailable)?;
                let init = serde_json::json!({"op":"open","key":key,"directory":session.directory,"profile":self.profile.id()});
                self.exchange_bounded(session, &init)?;
            } else {
                if let Some(ChildControl { query: active, .. }) = control.as_mut() {
                    *active = query.map(|q| q.id);
                }
                drop(control);
            }
            check()?;
            self.exchange_bounded(io.as_mut().ok_or(Error::Unavailable)?, request)
        })();
        if let Ok(mut child) = self.child.lock() {
            if let Some(ChildControl { query: active, .. }) = child.as_mut() {
                *active = None;
            }
        }
        let current = check();
        if current.is_err() || result.is_err() {
            *io = None;
        }
        current?;
        serde_json::from_value(result?).map_err(|_| Error::InvalidData)
    }
    fn exchange_bounded(
        &self,
        session: &mut Session,
        request: &impl Serialize,
    ) -> Result<Value, Error> {
        let child = self.child.clone();
        let expected = session.epoch;
        let (done, waiting) = std::sync::mpsc::channel();
        let watchdog = std::thread::spawn(move || {
            if waiting
                .recv_timeout(std::time::Duration::from_secs(30))
                .is_err()
            {
                if let Ok(mut child) = child.lock() {
                    if let Some(ChildControl {
                        epoch,
                        process: child,
                        ..
                    }) = child.as_mut()
                    {
                        if *epoch == expected {
                            let _ = child.kill();
                        }
                    }
                }
            }
        });
        let result = Self::exchange(session, request);
        let _ = done.send(());
        let _ = watchdog.join();
        result
    }
    fn exchange(session: &mut Session, request: &impl Serialize) -> Result<Value, Error> {
        let bytes = Zeroizing::new(serde_json::to_vec(request).map_err(|_| Error::InvalidData)?);
        if bytes.len() > FRAME_LIMIT {
            return Err(Error::InvalidData);
        }
        session
            .input
            .write_all(
                &u32::try_from(bytes.len())
                    .map_err(|_| Error::InvalidData)?
                    .to_be_bytes(),
            )
            .and_then(|()| session.input.write_all(&bytes))
            .and_then(|()| session.input.flush())
            .map_err(|_| Error::RetrievalUnavailable)?;
        let mut size = [0; 4];
        session
            .output
            .read_exact(&mut size)
            .map_err(|_| Error::RetrievalUnavailable)?;
        let length = u32::from_be_bytes(size) as usize;
        if length > FRAME_LIMIT {
            return Err(Error::InvalidData);
        }
        let mut reply = Zeroizing::new(vec![0; length]);
        session
            .output
            .read_exact(&mut reply)
            .map_err(|_| Error::RetrievalUnavailable)?;
        let reply: Value = serde_json::from_slice(&reply).map_err(|_| Error::InvalidData)?;
        if reply.get("error").is_some() {
            return Err(Error::RetrievalUnavailable);
        }
        Ok(reply)
    }
}
#[cfg(test)]
impl Runtime {
    /// Numerical evidence fixture for existing authority/custody scenarios only.
    /// It does not establish model quality, `SQLite` behavior or runtime performance.
    pub fn fixture(epoch: Arc<AtomicU64>) -> Self {
        Self {
            resources: PathBuf::new(),
            profile: CompatibilityProfile::new(
                [1; 32], [2; 32], [3; 32], [4; 32], [5; 32], [6; 32], 512, true,
            )
            .unwrap(),
            io: Mutex::new(None),
            child: Arc::new(Mutex::new(None)),
            epoch,
            fixture: Some(Mutex::new(std::collections::BTreeMap::new())),
        }
    }
    fn fixture_request(&self, request: &Value) -> Result<Value, Error> {
        let mut vector = vec![0f32; 512];
        vector[0] = 1.;
        match request["op"].as_str().ok_or(Error::InvalidData)? {
            "prepare" => {
                let chunks=request["parts"].as_array().ok_or(Error::InvalidData)?.iter().enumerate().filter_map(|(part,text)|text.as_str().filter(|t|!t.trim().is_empty()).map(|t|serde_json::json!({"part":part,"start":0,"end":t.len(),"values":vector}))).collect::<Vec<_>>();
                let after = usize::try_from(request["after"].as_u64().ok_or(Error::InvalidData)?)
                    .map_err(|_| Error::InvalidData)?;
                Ok(
                    serde_json::json!({"chunks":chunks.iter().skip(after).take(4).collect::<Vec<_>>(),"complete":after+4>=chunks.len()}),
                )
            }
            "upsert" => {
                let mut rows = self.fixture.as_ref().unwrap().lock().unwrap();
                for row in request["rows"].as_array().ok_or(Error::InvalidData)? {
                    rows.insert(row["id"].as_u64().ok_or(Error::InvalidData)?, row.clone());
                }
                Ok(serde_json::json!({}))
            }
            "query" => {
                let ids = request["allowed"]
                    .as_array()
                    .ok_or(Error::InvalidData)?
                    .iter()
                    .filter_map(Value::as_u64)
                    .collect::<Vec<_>>();
                Ok(
                    serde_json::json!({"semantic":ids.iter().map(|id|(*id,0f64)).collect::<Vec<_>>(),"lexical":ids,"vector":vector}),
                )
            }
            _ => Err(Error::InvalidData),
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.interrupt();
        if let Ok(mut child) = self.child.lock() {
            if let Some(ChildControl {
                process: mut child, ..
            }) = child.take()
            {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

#[cfg(test)]
mod native_tests {
    use super::*;
    use serde_json::json;
    /// Conjecture: the shipped numerical/storage bridge preserves bounded vectors,
    /// filters by current allowed occurrences, and cannot revive a locked lifetime.
    #[test]
    #[ignore = "requires packaged Mac MPS runtime; run explicitly with CS_MAIL_SEARCH_RESOURCES"]
    fn packaged_offline_retrieval_roundtrip() {
        let resources =
            std::env::var_os("CS_MAIL_SEARCH_RESOURCES").expect("explicit packaged resources");
        let epoch = Arc::new(AtomicU64::new(1));
        let runtime = Runtime::new(Path::new(&resources), epoch.clone()).unwrap();
        let mut key = [0; 32];
        getrandom::fill(&mut key).unwrap();
        // Conjecture: explicit startup preparation initializes the shipped model
        // without producing search results or retaining authority after lock.
        let startup = std::time::Instant::now();
        let _: Value = runtime
            .request(
                1,
                &key,
                &json!({"op":"prepare","title":"","parts":["Local search preparation."],"after":0}),
            )
            .unwrap();
        println!("startup preparation: {:?}", startup.elapsed());
        let text = "The wool sweater was exchanged for a larger size at the clothing shop. 😀";
        let prepared:Value=runtime.request(1,&key,&json!({"op":"prepare","title":"Fictional receipt","parts":[text,"We discussed journal peer review and an editorial platform."],"after":0})).unwrap();
        assert_eq!(prepared["complete"], true);
        assert_eq!(prepared["chunks"].as_array().unwrap().len(), 2);
        let rows=prepared["chunks"].as_array().unwrap().iter().enumerate().map(|(i,c)| {
            let values:Vec<f32>=serde_json::from_value(c["values"].clone()).unwrap();
            cs_mail_search::Embedding::new(runtime.profile(),values).unwrap();
            json!({"id":i+1,"alias":if i==0{"7"}else{"9"},"text":if i==0{text}else{"We discussed journal peer review and an editorial platform."},"values":c["values"]})
        }).collect::<Vec<_>>();
        let _: Value = runtime
            .request(1, &key, &json!({"op":"upsert","rows":rows}))
            .unwrap();
        let querying = std::time::Instant::now();
        let ranks:Value=runtime.request(1,&key,&json!({"op":"query","query":"What garment did I exchange?","scope":null,"all":[1,2],"allowed":[1,2]})).unwrap();
        println!("first query after preparation: {:?}", querying.elapsed());
        assert_eq!(ranks["semantic"][0][0], 1);
        let excluded:Value=runtime.request(1,&key,&json!({"op":"query","query":"journal review","scope":"9","all":[1,2],"allowed":[1]})).unwrap();
        assert!(excluded["semantic"].as_array().unwrap().is_empty());
        assert!(excluded["lexical"].as_array().unwrap().is_empty());
        let directory = runtime
            .io
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .directory
            .clone();
        for file in [
            directory.join("projection.sqlite"),
            directory.join("projection.sqlite-wal"),
        ] {
            let bytes = std::fs::read(file).unwrap();
            assert!(
                !bytes
                    .windows(text.len())
                    .any(|window| window == text.as_bytes())
            );
        }
        epoch.store(2, Ordering::SeqCst);
        runtime.interrupt_before(2);
        assert!(!directory.exists());
        let denied: Result<Value, _> = runtime.request(
            1,
            &key,
            &json!({"op":"prepare","title":"","parts":[text],"after":0}),
        );
        assert!(denied.is_err());
        epoch.store(3, Ordering::SeqCst);
        let reopened: Value = runtime
            .request(
                3,
                &key,
                &json!({"op":"prepare","title":"","parts":[text],"after":0}),
            )
            .unwrap();
        assert_eq!(reopened["complete"], true);
        runtime.interrupt_before(2); // Delayed old cleanup must not kill the new worker.
        let alive: Value = runtime
            .request(
                3,
                &key,
                &json!({"op":"prepare","title":"","parts":[text],"after":0}),
            )
            .unwrap();
        assert_eq!(alive["complete"], true);
        runtime.interrupt();
    }
}

#[cfg(all(test, unix))]
mod cancellation_tests {
    use super::*;
    use serde_json::json;
    use std::time::{Duration, Instant};

    struct Resources(PathBuf);
    impl Drop for Resources {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn resources() -> Resources {
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).unwrap();
        let root = std::env::temp_dir().join(format!(
            "cs-mail-query-check-{}",
            u128::from_be_bytes(nonce)
        ));
        std::fs::create_dir_all(root.join("python/bin")).unwrap();
        Resources(root)
    }
    fn wait_for(path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !path.exists() {
            assert!(
                Instant::now() < deadline,
                "worker did not reach declared wait"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    // Conjecture: superseding an active search releases the shared worker,
    // obsolete queued work cannot mutate its projection, and late cancellation
    // cannot kill the current search. This uses real anonymous pipes and a child.
    #[test]
    fn supersession_interrupts_active_work_and_rejects_obsolete_queue() {
        let root = resources();
        std::os::unix::fs::symlink("/usr/bin/python3", root.0.join("python/bin/python3")).unwrap();
        std::fs::write(
            root.0.join("helper.py"),
            r"
import json, struct, sys, time
from pathlib import Path
root=Path(__file__).parent
while True:
    size=sys.stdin.buffer.read(4)
    if not size: break
    request=json.loads(sys.stdin.buffer.read(struct.unpack('>I',size)[0]))
    if request['op']=='wait':
        (root/request['entered']).touch()
        while not (root/request['release']).exists(): time.sleep(.005)
    elif request['op']=='mutation': (root/'obsolete-executed').touch()
    response=b'{}'
    sys.stdout.buffer.write(struct.pack('>I',len(response))+response);sys.stdout.buffer.flush()
",
        )
        .unwrap();
        let mut runtime = Runtime::fixture(Arc::new(AtomicU64::new(1)));
        runtime.fixture = None;
        runtime.resources = root.0.clone();
        let runtime = Arc::new(runtime);
        let revision = Arc::new(AtomicU64::new(1));
        let active = runtime.clone();
        let active_revision = revision.clone();
        let old = std::thread::spawn(move || {
            active.request_query::<_, Value>(
                1,
                &[7; 32],
                &json!({"op":"wait","entered":"old-entered","release":"old-release"}),
                QueryFence {
                    id: SearchRequestId::try_from(1).unwrap(),
                    revision: &active_revision,
                },
            )
        });
        wait_for(&root.0.join("old-entered"));
        revision.store(2, Ordering::SeqCst);
        let queued = runtime.clone();
        let queued_revision = revision.clone();
        let obsolete = std::thread::spawn(move || {
            queued.request_query::<_, Value>(
                1,
                &[7; 32],
                &json!({"op":"mutation"}),
                QueryFence {
                    id: SearchRequestId::try_from(2).unwrap(),
                    revision: &queued_revision,
                },
            )
        });
        revision.store(3, Ordering::SeqCst);
        runtime.supersede_search(SearchRequestId::try_from(3).unwrap());
        assert_eq!(old.join().unwrap().unwrap_err(), Error::SearchSuperseded);
        assert_eq!(
            obsolete.join().unwrap().unwrap_err(),
            Error::SearchSuperseded
        );
        assert!(!root.0.join("obsolete-executed").exists());
        let current = runtime.clone();
        let current_revision = revision.clone();
        let latest = std::thread::spawn(move || {
            current.request_query::<_, Value>(
                1,
                &[7; 32],
                &json!({"op":"wait","entered":"new-entered","release":"new-release"}),
                QueryFence {
                    id: SearchRequestId::try_from(3).unwrap(),
                    revision: &current_revision,
                },
            )
        });
        wait_for(&root.0.join("new-entered"));
        runtime.supersede_search(SearchRequestId::try_from(2).unwrap());
        std::fs::write(root.0.join("new-release"), []).unwrap();
        assert!(latest.join().unwrap().is_ok());
    }
    // Conjecture: an unavailable executable is a capability failure, not a busy
    // foreground lock that the host should retry indefinitely.
    #[test]
    fn missing_runtime_is_distinct_from_contention() {
        let root = resources();
        let mut runtime = Runtime::fixture(Arc::new(AtomicU64::new(1)));
        runtime.fixture = None;
        runtime.resources = root.0.clone();
        let result: Result<Value, _> = runtime.request(1, &[7; 32], &json!({"op":"prepare"}));
        assert_eq!(result.unwrap_err(), Error::RetrievalUnavailable);
        assert!(runtime.io.lock().unwrap().is_none());
    }
}
