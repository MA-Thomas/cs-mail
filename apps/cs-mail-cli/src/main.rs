//! `cs-mail-cli`, the developer member client.
//!
//! This binary is an adapter over the client core: it will drive member journeys
//! (enrollment, requests, decisions) before the desktop application exists.
//! Milestone 0 provides only its identity; member journeys arrive in Milestone 2.

use clap::Parser;

/// Developer member client for cs-mail.
///
/// No member commands exist yet. They arrive in Milestone 2 of the product build plan.
#[derive(Parser)]
#[command(name = "cs-mail-cli", version, arg_required_else_help = true)]
struct Cli {}

fn main() {
    let Cli {} = Cli::parse();
}
