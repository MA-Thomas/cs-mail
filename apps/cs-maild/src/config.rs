//! Host configuration. Secrets are read from protected files and never logged; the
//! database URL may carry credentials and is never logged either.
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ConfigError(pub String);
impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for ConfigError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub database_url: String,
    /// Loopback only; TLS is terminated by the deployment's proxy.
    pub listen: SocketAddr,
    /// Upper bound on concurrent blocking storage calls from HTTP handlers.
    #[serde(default = "default_permits")]
    pub blocking_permits: usize,
    #[serde(default)]
    pub workers: WorkerConfig,
    /// The identity service that decides enrollment eligibility.
    pub identity: Option<IdentityConfig>,
    #[serde(default)]
    pub enrollment: EnrollmentConfig,
    /// Development only, with the `simulation` feature.
    pub simulation: Option<SimulationConfig>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityConfig {
    /// The identity service's enrollment endpoint: HTTPS, or HTTP on loopback.
    pub endpoint: String,
    /// JSON array of the product signing seed the identity service trusts.
    pub product_secret_file: PathBuf,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentConfig {
    /// Funding limit for each new account: unresolved request charges at once.
    pub maximum_unresolved: u32,
}
impl Default for EnrollmentConfig {
    fn default() -> Self {
        Self {
            maximum_unresolved: 10,
        }
    }
}
#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerConfig {
    pub interval_ms: u64,
    pub lease_ms: u64,
    pub batch: i64,
}
impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            interval_ms: 1_000,
            lease_ms: 30_000,
            batch: 50,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_field_names)] // Each field names one protected secret file.
pub struct SimulationConfig {
    /// JSON array of the simulated processor's 32-byte signing seed.
    #[cfg_attr(not(feature = "simulation"), allow(dead_code))]
    pub processor_secret_file: PathBuf,
    /// JSON array of the simulated bank's financial-verification seed (the deployment's
    /// bank authority).
    #[cfg_attr(not(feature = "simulation"), allow(dead_code))]
    pub bank_authority_secret_file: PathBuf,
    /// JSON array of the simulated bank's ownership-attestation seed (trusted by the
    /// identity service).
    #[cfg_attr(not(feature = "simulation"), allow(dead_code))]
    pub bank_ownership_secret_file: PathBuf,
}
const fn default_permits() -> usize {
    8
}

impl Config {
    /// # Errors
    /// Rejects unreadable or malformed configuration and a non-loopback listener.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let bytes = std::fs::read(path)
            .map_err(|e| ConfigError(format!("cannot read configuration: {e}")))?;
        let config: Self = serde_json::from_slice(&bytes)
            .map_err(|e| ConfigError(format!("invalid configuration: {e}")))?;
        if !config.listen.ip().is_loopback() {
            return Err(ConfigError(
                "listen must be a loopback address; terminate TLS at the proxy".into(),
            ));
        }
        if config.enrollment.maximum_unresolved == 0
            || config.blocking_permits == 0
            || config.workers.interval_ms == 0
            || config.workers.lease_ms == 0
            || config.workers.batch <= 0
        {
            return Err(ConfigError(
                "worker and blocking limits must be positive".into(),
            ));
        }
        Ok(config)
    }
}

/// Reads a 32-byte secret stored as a JSON array; its contents are never printed.
/// # Errors
/// Rejects an unreadable file or anything but exactly 32 bytes.
pub fn secret(path: &Path) -> Result<[u8; 32], ConfigError> {
    let bytes = std::fs::read(path)
        .map_err(|_| ConfigError(format!("cannot read secret file {}", path.display())))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| ConfigError(format!("secret file {} is not 32 bytes", path.display())))
}
