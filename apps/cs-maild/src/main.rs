//! `cs-maild`, the cs-mail service process.
//!
//! An adapter: it validates input, invokes application use cases and renders outcomes. It
//! makes no domain decisions and obtains trusted time only inside application decisions.

mod aliases;
mod blocking;
mod callbacks;
mod config;
mod diagnostics;
mod directory;
mod enrollment;
mod external;
mod guests;
mod http;
mod members;
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

/// The member gateway on its own database connection, when configured and bootstrapped.
fn member_gateway(
    config: &config::Config,
    record: Option<&DeploymentBootstrap>,
) -> Result<Option<members::Gateway>, String> {
    let Some(members) = &config.members else {
        tracing::warn!("no member configuration; the member API is unavailable");
        return Ok(None);
    };
    let Some(record) = record else {
        tracing::warn!("deployment not bootstrapped; the member API is unavailable until restart");
        return Ok(None);
    };
    let secret = |path| config::secret(path).map_err(|e| e.to_string());
    let gateway = cs_mail_service::members::MemberGateway::new(
        connect(config)?,
        record.clone(),
        cs_mail_service::members::GatewaySecrets {
            provider_seed: secret(&members.provider_secret_file)?,
            relationship_secret: secret(&members.relationship_secret_file)?,
            principal_secret: secret(&members.principal_secret_file)?,
        },
        cs_mail_service::UnixMillisecondClock,
    )
    .map_err(|_| "the provider secret is not the bootstrapped provider key")?;
    let custody = cs_mail_key_custody::LocalCustodian::from_secret_bytes(
        members.custody_key_ref,
        &secret(&members.custody_secret_file)?,
    )
    .map_err(|e| e.to_string())?;
    gateway
        .deployment()
        .accounts(crate::http::now)
        .configure_custody(custody.public_key())
        .map_err(|e| e.to_string())?;
    Ok(Some(std::sync::Arc::new(gateway)))
}
/// The guest request page's service, when configured. Development builds tokenize guest
/// cards with the simulator; a build without a payment provider has no guest page.
fn guest_gateway(
    config: &config::Config,
    members: Option<&members::Gateway>,
) -> Result<Option<guests::Guests>, String> {
    let Some(guests) = &config.guests else {
        tracing::warn!("no guest configuration; the request page is unavailable");
        return Ok(None);
    };
    let Some(members) = members else {
        return Err("the guest page needs the member configuration".into());
    };
    #[cfg(feature = "simulation")]
    {
        let secret = |path| config::secret(path).map_err(|e| e.to_string());
        let simulation = config
            .simulation
            .as_ref()
            .ok_or("the guest page needs a payment provider: the simulation in development")?;
        let outbox = guests::outbox(guests)?;
        Ok(Some(std::sync::Arc::new(
            cs_mail_service::guests::GuestGateway::new(
                members.clone(),
                secret(&guests.content_secret_file)?,
                outbox,
                std::sync::Arc::new(simulation::Cards(cs_mail_simulation::SimulatedCards::new(
                    secret(&simulation.processor_secret_file)?,
                ))),
                cs_mail_smtp_gateway::outbound::RequestSite::new(&guests.public_url)
                    .map_err(|_| "invalid public request-page origin")?,
            ),
        )))
    }
    #[cfg(not(feature = "simulation"))]
    {
        let _ = (guests, members);
        Err("this build has no payment provider for guests".into())
    }
}

#[cfg(feature = "simulation")]
fn gateway_record(gateway: &members::Gateway) -> cs_mail_operations::BootstrapRecord {
    gateway.record().record().clone()
}

fn serve(path: &std::path::Path) -> Result<(), String> {
    let config = config::Config::load(path).map_err(|e| e.to_string())?;
    // Reserve both ports before any worker can begin external work.
    let listener = bind(config.listen)?;
    let feedback_listener = config
        .external
        .as_ref()
        .and_then(|e| e.feedback.listen())
        .map(bind)
        .transpose()?;
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
    let members = member_gateway(&config, record.as_ref())?;
    #[cfg(feature = "simulation")]
    if let (Some(gateway), Some(simulation)) = (&members, &config.simulation) {
        let processor =
            config::secret(&simulation.processor_secret_file).map_err(|e| e.to_string())?;
        let trusted = gateway_record(gateway).payment.processor_key;
        workers.start(
            "relationships",
            config.workers,
            members::worker(
                gateway.clone(),
                simulation::connect(&config.database_url, processor, trusted)?,
            ),
        );
    }
    #[cfg(not(feature = "simulation"))]
    if members.is_some() {
        tracing::warn!("no payment provider in this build; relationship workers are not running");
    }
    let guests = guest_gateway(&config, members.as_ref())?;
    if let Some(guests) = &guests {
        workers.start(
            "guest-notices",
            config.workers,
            guests::notices(guests.clone()),
        );
    }
    let (external, feedback) = if let Some(external_config) = &config.external {
        let gateway = members
            .clone()
            .ok_or("external correspondence needs member configuration")?;
        let (external, webhook) =
            external::configure(external_config, gateway, &mut workers, config.workers)?;
        (Some(external), webhook)
    } else {
        (None, None)
    };
    let state = http::AppState {
        directory_domain: config.directory_domain.clone(),
        mail_domain: config.external.as_ref().map(|e| e.mail_domain.clone()),
        browser_login: config.browser_login.clone(),
        deployment,
        blocking: blocking::Blocking::new(config.blocking_permits),
        bank,
        identity: identity_service(&config, &mut workers)?,
        maximum_unresolved: config.enrollment.maximum_unresolved,
        members,
        guests,
        external,
        #[cfg(feature = "simulation")]
        simulator,
    };
    let feedback = feedback_listener.zip(feedback);
    let served = run_http(listener, state, feedback);
    workers.stop();
    served
}

fn bind(listen: std::net::SocketAddr) -> Result<std::net::TcpListener, String> {
    let listener =
        std::net::TcpListener::bind(listen).map_err(|e| format!("cannot listen: {e}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("listener: {e}"))?;
    Ok(listener)
}

fn run_http(
    listener: std::net::TcpListener,
    state: http::AppState,
    feedback: Option<(std::net::TcpListener, callbacks::Receiver)>,
) -> Result<(), String> {
    // Keep synchronous PostgreSQL clients alive until the async runtime is dropped.
    let outlives_runtime = state.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("runtime: {e}"))?;
    let lost = Arc::new(AtomicBool::new(false));
    let watched = (state.clone(), lost.clone());
    let served = runtime.block_on(async move {
        let (stop, stopping) = tokio::sync::watch::channel(false);
        let api_stop = stopping.clone();
        let callback_stop = stopping.clone();
        let listener =
            tokio::net::TcpListener::from_std(listener).map_err(|e| format!("listener: {e}"))?;
        let api = axum::serve(listener, http::router(state))
            .with_graceful_shutdown(shutdown_requested(api_stop));
        let callback = async move {
            if let Some((listener, receiver)) = feedback {
                let listener = tokio::net::TcpListener::from_std(listener)?;
                axum::serve(listener, receiver.router())
                    .with_graceful_shutdown(shutdown_requested(callback_stop))
                    .await
            } else {
                shutdown_requested(callback_stop).await;
                Ok(())
            }
        };
        let api = std::future::IntoFuture::into_future(api);
        tokio::pin!(api, callback);
        tracing::info!("serving configured listeners");
        let result = tokio::select! {
            result = &mut api => {
                let _ = stop.send(true);
                let other = callback.await;
                result.and(other)
            },
            result = &mut callback => {
                let _ = stop.send(true);
                let other = api.await;
                result.and(other)
            },
            signal = tokio::signal::ctrl_c() => {
                if signal.is_err() { tracing::error!("shutdown signal unavailable"); }
                let _ = stop.send(true);
                let (a, b) = tokio::join!(api, callback);
                a.and(b)
            },
            () = connection_lost(watched.0) => {
                watched.1.store(true, Ordering::SeqCst);
                let _ = stop.send(true);
                let (a, b) = tokio::join!(api, callback);
                a.and(b)
            }
        };
        result.map_err(|e| format!("server: {e}"))
    });
    drop(runtime);
    drop(outlives_runtime);
    if lost.load(Ordering::SeqCst) {
        return Err("database connection lost".into());
    }
    served
}

async fn shutdown_requested(mut receiver: tokio::sync::watch::Receiver<bool>) {
    while !*receiver.borrow_and_update() {
        if receiver.changed().await.is_err() {
            break;
        }
    }
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
