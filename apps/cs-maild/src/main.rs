//! `cs-maild`, the cs-mail service process.
//!
//! This binary is an adapter: it will host the HTTP API, workers, and the sender
//! request page over the application layer, and must not make domain decisions.
//! Milestone 0 provides only its identity; the service arrives in Milestone 1.

use clap::Parser;

/// The cs-mail service: HTTP API, workers, and sender request page hosting.
///
/// No service commands exist yet. They arrive in Milestone 1 of the product build plan.
#[derive(Parser)]
#[command(name = "cs-maild", version, arg_required_else_help = true)]
struct Cli {}

fn main() {
    let Cli {} = Cli::parse();
}
