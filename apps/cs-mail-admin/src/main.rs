//! `cs-mail-admin`, the operator client.
//!
//! It holds an operator key, signs operator commands and sends them to the service's
//! authenticated API. It never writes database tables and makes no domain decisions.
//! Secret files are JSON arrays of 32 bytes and are never printed.

use clap::{Parser, Subcommand};
use cs_mail_operations::{
    CommandFamily, OperatorCommand, OperatorKeyRef, ReviewCommand, SignedOperatorCommand,
};
use cs_mail_primitives::IdempotencyKey;
use ed25519_dalek::SigningKey;
use identity_contract::evidence::{ReviewCaseId, ReviewResolution};
use serde::Deserialize;
use sha2::Digest as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// cs-mail operator CLI.
#[derive(Parser)]
#[command(name = "cs-mail-admin", version, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Generate a new Ed25519 signing key file and print its public key.
    Keygen {
        /// Where to write the secret; an existing file is never overwritten.
        #[arg(long)]
        out: PathBuf,
    },
    /// Print the public key of a secret key file.
    PublicKey {
        #[arg(long)]
        secret: PathBuf,
    },
    /// Report the service's readiness.
    Status {
        #[arg(long)]
        config: PathBuf,
    },
    /// Sign and send a configuration command (a service offer or a pricing policy).
    Configure {
        #[arg(long)]
        config: PathBuf,
        /// JSON operator command, for example {"PublishServiceOffer": {...}}.
        #[arg(long)]
        command: PathBuf,
        /// Command identity; defaults to a digest of the command, so a repeat is a replay.
        #[arg(long)]
        idempotency_key: Option<u128>,
    },
    /// List enrollments held for review because their evidence overlaps another's.
    Reviews {
        #[arg(long)]
        config: PathBuf,
    },
    /// Approve or deny an enrollment held for review.
    Review {
        #[arg(long)]
        config: PathBuf,
        /// The case identifier, as `reviews` lists it.
        #[arg(long)]
        case: String,
        #[arg(long, value_enum)]
        resolution: Resolution,
    },
    /// Development only: sign and send a payment simulator scenario command.
    #[cfg(feature = "simulation")]
    Simulate {
        #[arg(long)]
        config: PathBuf,
        /// JSON simulation command, for example {"QueueDirectives": [...]}.
        #[arg(long)]
        command: PathBuf,
        #[arg(long)]
        idempotency_key: Option<u128>,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Resolution {
    Approve,
    Deny,
}

/// Operator client configuration.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    /// Service base URL: HTTPS, or HTTP on a loopback host for development.
    server: String,
    operator: u128,
    operator_secret_file: PathBuf,
    deployment_domain: [u8; 32],
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path, what: &str) -> Result<T, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {what}: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid {what}: {e}"))
}
fn secret(path: &Path) -> Result<[u8; 32], String> {
    let bytes = std::fs::read(path).map_err(|_| format!("cannot read {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| format!("{} is not a 32-byte secret", path.display()))
}
fn public(secret: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(secret).verifying_key().to_bytes()
}

fn keygen(out: &Path) -> Result<(), String> {
    let mut seed = [0; 32];
    getrandom::getrandom(&mut seed).map_err(|e| format!("no system randomness: {e}"))?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(out)
        .map_err(|e| format!("cannot create {}: {e}", out.display()))?;
    std::io::Write::write_all(
        &mut file,
        &serde_json::to_vec(&seed).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    println!("{}", serde_json::json!({ "verifying_key": public(&seed) }));
    Ok(())
}

fn client(config: &Config) -> Result<(reqwest::blocking::Client, reqwest::Url), String> {
    let base = reqwest::Url::parse(&config.server).map_err(|e| format!("invalid server: {e}"))?;
    let loopback = matches!(base.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"));
    if base.scheme() != "https" && !(base.scheme() == "http" && loopback) {
        return Err("server must use HTTPS, or HTTP on a loopback host".into());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    Ok((client, base))
}
fn report(response: reqwest::blocking::Response) -> Result<(), String> {
    let status = response.status();
    let body = response.text().unwrap_or_default();
    println!("{body}");
    if status.is_success() {
        Ok(())
    } else {
        Err(format!("service answered {status}"))
    }
}

fn send<C: CommandFamily>(
    config_path: &Path,
    command: C,
    identity: Option<u128>,
    route: &str,
) -> Result<(), String> {
    let config: Config = read_json(config_path, "configuration")?;
    let identity = if let Some(identity) = identity {
        identity
    } else {
        let digest = sha2::Sha256::digest(
            serde_json::to_vec(&(C::NAME, &command)).map_err(|e| e.to_string())?,
        );
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&digest[..16]);
        u128::from_be_bytes(bytes)
    };
    let signed = SignedOperatorCommand::sign(
        config.deployment_domain,
        OperatorKeyRef(config.operator),
        IdempotencyKey(identity),
        command,
        &secret(&config.operator_secret_file)?,
    )
    .map_err(|e| format!("cannot sign: {e}"))?;
    let (client, base) = client(&config)?;
    let url = base.join(route).map_err(|e| e.to_string())?;
    report(
        client
            .post(url)
            .json(&signed)
            .send()
            .map_err(|e| format!("cannot reach service: {e}"))?,
    )
}

fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Keygen { out } => keygen(&out),
        Command::PublicKey { secret: path } => {
            println!(
                "{}",
                serde_json::json!({ "verifying_key": public(&secret(&path)?) })
            );
            Ok(())
        }
        Command::Status { config } => {
            let config: Config = read_json(&config, "configuration")?;
            let (client, base) = client(&config)?;
            report(
                client
                    .get(base.join("ready").map_err(|e| e.to_string())?)
                    .send()
                    .map_err(|e| format!("cannot reach service: {e}"))?,
            )
        }
        Command::Configure {
            config,
            command,
            idempotency_key,
        } => send(
            &config,
            read_json::<OperatorCommand>(&command, "command")?,
            idempotency_key,
            "v1/operator/commands",
        ),
        Command::Reviews { config } => {
            send(&config, ReviewCommand::Open, None, "v1/operator/reviews")
        }
        Command::Review {
            config,
            case,
            resolution,
        } => send(
            &config,
            ReviewCommand::Resolve {
                case: ReviewCaseId::try_from(case).map_err(|_| "invalid case identifier")?,
                resolution: match resolution {
                    Resolution::Approve => ReviewResolution::Approve,
                    Resolution::Deny => ReviewResolution::Deny,
                },
            },
            None,
            "v1/operator/reviews",
        ),
        #[cfg(feature = "simulation")]
        Command::Simulate {
            config,
            command,
            idempotency_key,
        } => send(
            &config,
            read_json::<cs_mail_simulation::SimulationCommand>(&command, "command")?,
            idempotency_key,
            "v1/operator/simulation",
        ),
    }
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("cs-mail-admin: {message}");
            ExitCode::FAILURE
        }
    }
}
