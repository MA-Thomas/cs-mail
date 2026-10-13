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
    /// Namespace for deployment-issued member addresses; grants no external mail transport.
    pub directory_domain: Option<String>,
    /// Loopback only; TLS is terminated by the deployment's proxy.
    pub listen: SocketAddr,
    /// Upper bound on concurrent blocking storage calls from HTTP handlers.
    #[serde(default = "default_permits")]
    pub blocking_permits: usize,
    #[serde(default)]
    pub workers: WorkerConfig,
    /// The identity service that decides enrollment eligibility.
    pub identity: Option<IdentityConfig>,
    /// Public browser-login settings; must match the identity service's configured issuer/audience.
    pub browser_login: Option<BrowserLoginConfig>,
    #[serde(default)]
    pub enrollment: EnrollmentConfig,
    /// The member API and relationship workers.
    pub members: Option<MembersConfig>,
    /// The guest request page, its API and guest notices; needs `members`.
    pub guests: Option<GuestsConfig>,
    /// Member-initiated external correspondence; independent of guest payments.
    pub external: Option<ExternalConfig>,
    /// Development only, with the `simulation` feature.
    pub simulation: Option<SimulationConfig>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_field_names)] // Each field names one protected secret file.
pub struct MembersConfig {
    pub custody_key_ref: cs_mail_primitives::ContentKeyRef,
    pub custody_secret_file: PathBuf,
    /// JSON array of the provider's 32-byte signing seed; its public key must be the
    /// bootstrapped provider key.
    pub provider_secret_file: PathBuf,
    /// JSON array of the 32-byte secret relationship references are derived with.
    /// Replacing it disconnects every existing relationship.
    pub relationship_secret_file: PathBuf,
    /// JSON array of the 32-byte secret request histories are derived with. Replacing it
    /// disconnects every existing request history.
    pub principal_secret_file: PathBuf,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityConfig {
    /// The identity service's enrollment endpoint: HTTPS, or HTTP on loopback.
    pub endpoint: String,
    /// JSON array of the product signing seed the identity service trusts.
    pub product_secret_file: PathBuf,
}
#[derive(Clone, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserLoginConfig {
    pub issuer: String,
    pub client_id: String,
    pub redirect_uri: String,
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
// A production host parses this shape to reject unsupported guest simulation at boot.
#[cfg_attr(not(feature = "simulation"), allow(dead_code))]
pub struct GuestsConfig {
    pub public_url: String,
    /// JSON array of the 32-byte secret of the provider's guest content key: guest
    /// messages are encrypted to members under it on arrival.
    pub content_secret_file: PathBuf,
    /// The deployment's mail domain, from which notices are sent.
    pub mail_domain: String,
    /// Transactional mail is configured independently of member-authored email.
    pub notices: GuestNoticeDelivery,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(not(feature = "simulation"), allow(dead_code))]
pub enum GuestNoticeDelivery {
    Smtp2go {
        api_key_file: PathBuf,
        dkim_secret_file: PathBuf,
        dkim_selector: String,
        region: Smtp2goRegion,
    },
    #[cfg(feature = "simulation")]
    Development {
        directory: PathBuf,
        dkim_secret_file: PathBuf,
        dkim_selector: String,
    },
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
        if let Some(listen) = config.external.as_ref().and_then(|e| e.feedback.listen()) {
            if !listen.ip().is_loopback() || listen == config.listen {
                return Err(ConfigError(
                    "feedback needs a separate loopback listener".into(),
                ));
            }
        }
        if let Some(guests) = &config.guests {
            if config.members.is_none()
                || config
                    .external
                    .as_ref()
                    .is_none_or(|e| e.mail_domain != guests.mail_domain)
            {
                return Err(ConfigError(
                    "guests requires members and external with the same mail_domain".into(),
                ));
            }
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalConfig {
    pub content_secret_file: PathBuf,
    pub mail_domain: String,
    pub delivery: ExternalDelivery,
    pub feedback: ExternalFeedback,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExternalDelivery {
    Disabled,
    #[cfg(feature = "simulation")]
    Development {
        directory: PathBuf,
    },
    Smtp2go {
        api_key_file: PathBuf,
        dkim_secret_file: PathBuf,
        dkim_selector: String,
        region: Smtp2goRegion,
    },
}
/// Receiving provider evidence is independent of permission to submit new mail.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExternalFeedback {
    Disabled,
    Smtp2go {
        /// Dedicated loopback listener; expose only this listener through HTTPS.
        listen: SocketAddr,
        secret_file: PathBuf,
    },
}
impl ExternalFeedback {
    pub fn listen(&self) -> Option<SocketAddr> {
        match self {
            Self::Disabled => None,
            Self::Smtp2go { listen, .. } => Some(*listen),
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Smtp2goRegion {
    Global,
    Us,
    Eu,
    Au,
}
impl Smtp2goRegion {
    pub const fn endpoint(&self) -> &'static str {
        match self {
            Self::Global => "https://api.smtp2go.com/v3/email/mime",
            Self::Us => "https://us-api.smtp2go.com/v3/email/mime",
            Self::Eu => "https://eu-api.smtp2go.com/v3/email/mime",
            Self::Au => "https://au-api.smtp2go.com/v3/email/mime",
        }
    }
}
