//! The bounded boundary between async handlers and synchronous storage.
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct Blocking {
    permits: Arc<Semaphore>,
}
/// The blocking pool was closed or a task panicked.
#[derive(Debug)]
pub struct Unavailable;

impl Blocking {
    pub fn new(permits: usize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(permits)),
        }
    }
    /// Runs one storage call on the blocking pool, at most `permits` at a time.
    pub async fn run<T: Send + 'static>(
        &self,
        call: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, Unavailable> {
        let permit = self
            .permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| Unavailable)?;
        let result = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            call()
        })
        .await
        .map_err(|_| Unavailable)?;
        Ok(result)
    }
}
