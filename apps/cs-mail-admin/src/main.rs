//! `cs-mail-admin`, the operator administration client.
//!
//! This binary is an adapter: it will call the service's authenticated
//! administration API and never write canonical tables directly. Milestone 0
//! provides only its identity; administration arrives in Milestone 1.

use clap::Parser;

/// Operator administration client for a cs-mail deployment.
///
/// No administration commands exist yet. They arrive in Milestone 1 of the product
/// build plan and use the service's authenticated API, never direct database access.
#[derive(Parser)]
#[command(name = "cs-mail-admin", version, arg_required_else_help = true)]
struct Cli {}

fn main() {
    let Cli {} = Cli::parse();
}
