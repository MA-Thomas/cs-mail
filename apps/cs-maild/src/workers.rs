//! Background workers on their own database connections. Each batch is bounded and
//! claims work with fenced leases, so an interrupted run is recovered by the next one.
use crate::config::WorkerConfig;
#[cfg(feature = "simulation")]
use crate::{diagnostics, http::now};
use cs_mail_primitives::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Runs `tick` every interval until `stop` is set.
#[cfg_attr(not(feature = "simulation"), allow(dead_code))]
pub fn spawn(
    name: &'static str,
    config: WorkerConfig,
    stop: Arc<AtomicBool>,
    mut tick: impl FnMut(Duration, i64) + Send + 'static,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        tracing::info!(worker = name, "worker started");
        while !stop.load(Ordering::SeqCst) {
            tick(Duration(config.lease_ms), config.batch);
            let mut waited = 0;
            while waited < config.interval_ms && !stop.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(50));
                waited += 50;
            }
        }
        tracing::info!(worker = name, "worker stopped");
    })
}

/// Development only: collection, annual allocation and member payments against the
/// durable simulator.
#[cfg(feature = "simulation")]
pub fn payments(
    deployment: cs_mail_storage_postgres::PostgresDeployment,
    mut simulator: cs_mail_simulation::PostgresSimulator,
    unit: cs_mail_primitives::SettlementUnit,
) -> impl FnMut(Duration, i64) + Send + 'static {
    let mut failures = diagnostics::BatchFailures::default();
    move |lease, batch| {
        let clock = now;
        let mut report = |kind: &'static str,
                          result: Result<
            cs_mail_storage_postgres::WorkReport,
            cs_mail_worker::WorkerError,
        >| match result {
            Ok(report) => {
                failures.succeeded(kind);
                if report.claimed > 0 {
                    tracing::info!(
                        batch = kind,
                        claimed = report.claimed,
                        completed = report.completed,
                        retried = report.retried,
                        blocked = report.blocked,
                        lost_claims = report.lost_claims,
                        "batch finished"
                    );
                }
            }
            Err(cs_mail_worker::WorkerError::Storage(error)) => {
                failures.failed(kind, diagnostics::code(&error));
            }
        };
        report(
            "utility-payments",
            cs_mail_worker::run_utility_payment_batch(
                &deployment,
                &mut simulator,
                &clock,
                lease,
                batch,
            ),
        );
        report(
            "annual-distribution",
            cs_mail_worker::run_annual_distribution_batch(&deployment, unit, &clock, lease, batch),
        );
        report(
            "member-payments",
            cs_mail_worker::run_member_payment_batch(
                &deployment,
                &mut simulator,
                unit,
                clock(),
                lease,
                batch,
            ),
        );
    }
}
