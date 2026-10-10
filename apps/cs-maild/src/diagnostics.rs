//! Redacted diagnostics: typed codes only. Database messages can echo row values, so only
//! their SQLSTATE is reported; no secrets, keys, bodies or URLs are ever logged.
use cs_mail_storage_postgres::StorageError;

pub fn code(error: &StorageError) -> String {
    match error {
        StorageError::Database(e) => {
            format!("database:{}", e.code().map_or("connection", |c| c.code()))
        }
        StorageError::Operations(e) => format!("operations:{e:?}"),
        StorageError::Protocol(e) => format!("protocol:{e:?}"),
        StorageError::Billing(e) => format!("billing:{e:?}"),
        StorageError::Finance(e) => format!("finance:{e:?}"),
        StorageError::Security(e) => format!("security:{e:?}"),
        StorageError::Identity(e) => format!("identity:{e:?}"),
        other => format!("storage:{}", variant(other)),
    }
}
fn variant(error: &StorageError) -> String {
    let debug = format!("{error:?}");
    debug
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or("Unknown")
        .to_owned()
}

/// Reports each batch's failure when it starts or its code changes, and its recovery, so a
/// sustained outage does not repeat the same warning every interval.
#[derive(Default)]
pub struct BatchFailures(std::collections::HashMap<&'static str, String>);
impl BatchFailures {
    pub fn failed(&mut self, batch: &'static str, code: String) {
        if self.0.get(batch) != Some(&code) {
            tracing::warn!(batch, error = %code, "batch failed; repeats are not logged");
            self.0.insert(batch, code);
        }
    }
    pub fn succeeded(&mut self, batch: &'static str) {
        if self.0.remove(batch).is_some() {
            tracing::info!(batch, "batch recovered");
        }
    }
}
