//! `cs-maild`, the cs-mail service process.
//!
//! An adapter: it validates input, invokes application use cases and renders outcomes. It
//! makes no domain decisions and obtains trusted time only inside application decisions.

mod blocking;
mod config;
mod diagnostics;
mod enrollment;
mod http;
mod review;
#[cfg(feature = "simulation")]
mod simulation;
mod workers;

use clap::{Parser, Subcommand};
use cs_mail_operations::{BootstrapOutcome, DeploymentBootstrap};
use cs_mail_storage_postgres::PostgresDeployment;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The cs-mail service: HTTP API and workers.
#[derive(Parser)]
#[command(name = "cs-maild", version, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Apply database migrations and report the schema version.
    Migrate {
        #[arg(long)]
        config: PathBuf,
    },
    /// Create the deployment from a bootstrap record, or confirm an identical one.
    Bootstrap {
        #[arg(long)]
        config: PathBuf,
        /// JSON bootstrap record: provider, domain, scope, public keys and trust roots.
        #[arg(long)]
        deployment: PathBuf,
    },
    /// Serve the HTTP API and run the workers until interrupted.
    Serve {
        #[arg(long)]
        config: PathBuf,
    },
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_writer(std::io::stderr)
        .init();
    let result = match Cli::parse().command {
        Command::Migrate { config } => migrate(&config),
        Command::Bootstrap { config, deployment } => bootstrap(&config, &deployment),
        Command::Serve { config } => serve(&config),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            tracing::error!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn connect(config: &config::Config) -> Result<PostgresDeployment, String> {
    PostgresDeployment::connect(&config.database_url)
        .map_err(|e| format!("database: {}", diagnostics::code(&e)))
}

fn migrate(path: &std::path::Path) -> Result<(), String> {
    let config = config::Config::load(path).map_err(|e| e.to_string())?;
    let deployment = connect(&config)?;
    let version = deployment
        .schema_version()
        .map_err(|e| format!("database: {}", diagnostics::code(&e)))?;
    tracing::info!(schema_version = version, "migrations applied");
    #[cfg(feature = "simulation")]
    if let Some(simulation) = &config.simulation {
        let secret =
            config::secret(&simulation.processor_secret_file).map_err(|e| e.to_string())?;
        cs_mail_simulation::PostgresSimulator::connect(&config.database_url, secret)
            .map_err(|_| "simulator migrations failed".to_owned())?;
        tracing::info!("simulator migrations applied");
    }
    Ok(())
}

fn bootstrap(path: &std::path::Path, record: &std::path::Path) -> Result<(), String> {
    let config = config::Config::load(path).map_err(|e| e.to_string())?;
    let bootstrap: DeploymentBootstrap = serde_json::from_slice(
        &std::fs::read(record).map_err(|e| format!("cannot read bootstrap record: {e}"))?,
    )
    .map_err(|e| format!("invalid bootstrap record: {e}"))?;
    let deployment = connect(&config)?;
    match cs_mail_application::operations::bootstrap(&deployment, &bootstrap, http::now) {
        Ok(BootstrapOutcome::Created) => tracing::info!("deployment created"),
        Ok(BootstrapOutcome::AlreadyBootstrapped) => {
            tracing::info!("deployment already bootstrapped identically; nothing changed");
        }
        Err(error) => return Err(format!("bootstrap refused: {}", diagnostics::code(&error))),
    }
    Ok(())
}

/// The enrollment bank-evidence source, when one is configured.
type Bank = Arc<dyn enrollment::BankEvidence>;
/// Development only: the shared simulator and the simulated bank.
#[cfg(feature = "simulation")]
type SimulationParts = (Option<http::Simulator>, Option<Bank>);

/// Background worker threads and their shared stop signal.
struct Workers {
    stop: Arc<AtomicBool>,
    handles: Vec<std::thread::JoinHandle<()>>,
}
impl Workers {
    fn new() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            handles: Vec::new(),
        }
    }
    #[cfg_attr(not(feature = "simulation"), allow(dead_code))]
    fn start(
        &mut self,
        name: &'static str,
        config: config::WorkerConfig,
        tick: impl FnMut(cs_mail_primitives::Duration, i64) + Send + 'static,
    ) {
        self.handles
            .push(workers::spawn(name, config, self.stop.clone(), tick));
    }
    fn stop(self) {
        self.stop.store(true, Ordering::SeqCst);
        for handle in self.handles {
            let _ = handle.join();
        }
    }
}

/// Development only: the durable simulator for payment workers and operator scenario
/// commands, and the simulated bank for enrollment.
#[cfg(feature = "simulation")]
fn simulation_parts(
    config: &config::Config,
    record: Option<&DeploymentBootstrap>,
    workers: &mut Workers,
) -> Result<SimulationParts, String> {
    let Some(simulation) = &config.simulation else {
        tracing::warn!("no simulator configured; payment workers are not running");
        return Ok((None, None));
    };
    let secret = |path| config::secret(path).map_err(|e| e.to_string());
    let bank: Bank = Arc::new(simulation::Bank(cs_mail_simulation::SimulatedBank::new(
        secret(&simulation.bank_authority_secret_file)?,
        secret(&simulation.bank_ownership_secret_file)?,
    )));
    let Some(record) = record else {
        return Ok((None, Some(bank)));
    };
    let processor = secret(&simulation.processor_secret_file)?;
    let trusted = record.record().payment.processor_key;
    workers.start(
        "payments",
        config.workers,
        workers::payments(
            connect(config)?,
            simulation::connect(&config.database_url, processor, trusted)?,
            record.record().unit,
        ),
    );
    let simulator = simulation::connect(&config.database_url, processor, trusted)?;
    Ok((Some(Arc::new(std::sync::Mutex::new(simulator))), Some(bank)))
}

fn identity_service(
    config: &config::Config,
    workers: &mut Workers,
) -> Result<Option<Arc<enrollment::IdentityService>>, String> {
    let Some(identity) = &config.identity else {
        tracing::warn!("no identity service configured; enrollment is unavailable");
        return Ok(None);
    };
    let service = Arc::new(enrollment::IdentityService {
        client: cs_mail_identity_client::HttpIdentityClient::new(&identity.endpoint)
            .map_err(|_| "identity endpoint must be HTTPS, or HTTP on loopback")?,
        product_secret: config::secret(&identity.product_secret_file).map_err(|e| e.to_string())?,
    });
    workers.handles.push(workers::spawn(
        "identity-confirmations",
        config.workers,
        workers.stop.clone(),
        enrollment::confirmations(connect(config)?, service.clone()),
    ));
    Ok(Some(service))
}

fn serve(path: &std::path::Path) -> Result<(), String> {
    let config = config::Config::load(path).map_err(|e| e.to_string())?;
    let deployment = connect(&config)?;
    let mut workers = Workers::new();
    let record = deployment
        .deployment_record()
        .map_err(|e| format!("database: {}", diagnostics::code(&e)))?;
    if record.is_none() {
        tracing::warn!("deployment not bootstrapped; readiness will report unavailable");
    }
    #[cfg(feature = "simulation")]
    let (simulator, bank) = simulation_parts(&config, record.as_ref(), &mut workers)?;
    #[cfg(not(feature = "simulation"))]
    let bank: Option<Bank> = {
        if config.simulation.is_some() {
            return Err("this build has no simulation feature".into());
        }
        tracing::warn!("no payment provider in this build; payment workers are not running");
        None
    };
    if bank.is_none() {
        tracing::warn!("no bank integration; enrollment is unavailable");
    }
    let state = http::AppState {
        deployment,
        blocking: blocking::Blocking::new(config.blocking_permits),
        bank,
        identity: identity_service(&config, &mut workers)?,
        maximum_unresolved: config.enrollment.maximum_unresolved,
        #[cfg(feature = "simulation")]
        simulator,
    };
    let served = run_http(config.listen, state);
    workers.stop();
    served
}

fn run_http(listen: std::net::SocketAddr, state: http::AppState) -> Result<(), String> {
    // Synchronous database clients must be dropped outside the async runtime: this clone
    // keeps the last references alive until the runtime has stopped.
    let outlives_runtime = state.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("runtime: {e}"))?;
    let lost = Arc::new(AtomicBool::new(false));
    let watched = (state.clone(), lost.clone());
    let served = runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(listen)
            .await
            .map_err(|e| format!("cannot listen: {e}"))?;
        tracing::info!(listen = %listen, "serving");
        axum::serve(listener, http::router(state))
            .with_graceful_shutdown(async move {
                tokio::select! {
                    signal = tokio::signal::ctrl_c() => if signal.is_err() {
                        tracing::error!("shutdown signal unavailable");
                    },
                    () = connection_lost(watched.0) => {
                        tracing::error!("database connection lost; exiting so the supervisor restarts the service");
                        watched.1.store(true, Ordering::SeqCst);
                    }
                }
                tracing::info!("shutting down");
            })
            .await
            .map_err(|e| format!("server: {e}"))
    });
    drop(runtime);
    drop(outlives_runtime);
    if lost.load(Ordering::SeqCst) {
        return Err("database connection lost".into());
    }
    served
}

/// Resolves once the service's database connection has ended. Connections are not
/// reopened in place; a restart gives every component fresh connections, and readiness
/// reports unavailable until then.
async fn connection_lost(state: http::AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    loop {
        interval.tick().await;
        let deployment = state.deployment.clone();
        if let Ok(true) = state
            .blocking
            .run(move || deployment.connection_lost())
            .await
        {
            return;
        }
    }
}
