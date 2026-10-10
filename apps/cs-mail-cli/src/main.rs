//! `cs-mail-cli`, the developer member client.
//!
//! It holds one member's keys, signs that member's commands and reads, encrypts and
//! decrypts message content on the member's side, and talks to the service's member API.
//! It makes no domain decisions: the service decides and the client renders the outcome.
//!
//! Every signed request is recorded in the member's journal before it is sent. A request
//! whose answer was lost is sent again with exactly the same bytes, so the service
//! recognizes it as a replay rather than a new request or charge.

mod api;
mod member;
mod render;
mod state;

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::process::ExitCode;

/// cs-mail developer member client.
#[derive(Parser)]
#[command(name = "cs-mail-cli", version, arg_required_else_help = true)]
struct Cli {
    /// The member's state directory. Its `member.json` holds secret keys (mode 0600).
    #[arg(long, global = true, default_value = ".cs-mail")]
    home: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Accept external contact, or grant an express lane for the specified duration.
    ExternalAccept {
        #[arg(long)]
        conversation: u128,
        #[arg(long)]
        lane_lifetime_ms: Option<u64>,
    },
    /// Start an external conversation. The relay sees plaintext; consent is explicit.
    Email {
        #[arg(long)]
        conversation: u128,
        #[arg(long)]
        to: String,
        #[arg(long)]
        message_file: PathBuf,
        #[arg(long)]
        disclose_to_external_mail: bool,
        #[arg(long)]
        operation: String,
    },
    /// Send another message in an external conversation.
    EmailReply {
        #[arg(long)]
        conversation: u128,
        #[arg(long)]
        message_file: PathBuf,
        #[arg(long)]
        operation: String,
    },
    /// Read and decrypt an external conversation. Paginate with the returned cursor.
    ExternalRead {
        #[arg(long)]
        conversation: u128,
        #[arg(long, default_value_t = 0)]
        after: u64,
    },
    /// Close an unpaid invitation; does not block or settle a paid request.
    ExternalReject {
        #[arg(long)]
        conversation: u128,
    },
    /// Cancel waiting disclosure; dispatch already begun may be too late to cancel.
    WithdrawDisclosure {
        #[arg(long)]
        conversation: u128,
    },
    /// Explicitly allow this conversation's member replies to leave through external email.
    GrantDisclosure {
        #[arg(long)]
        conversation: u128,
        #[arg(long)]
        operation: String,
    },
    /// Create a member's keys and state.
    Init {
        /// Service base URL: HTTPS, or HTTP on a loopback host.
        #[arg(long)]
        server: String,
    },
    /// Reserve an enrollment and print the challenge a fresh login must carry as nonce.
    EnrollBegin {
        /// Bank-integration request naming the account to verify (JSON file).
        #[arg(long)]
        bank: PathBuf,
    },
    /// Complete the enrollment with a fresh login, bank ownership and the applicant's
    /// statement of name, date of birth, address and phone.
    EnrollComplete {
        /// The ID token from a fresh login whose nonce is the enrollment challenge.
        #[arg(long)]
        oidc_token: String,
        #[arg(long)]
        login_issuer: String,
        #[arg(long)]
        login_subject: String,
        /// The applicant's statement (JSON file: `name`, `date_of_birth`, `address`, `phone`).
        #[arg(long)]
        applicant: PathBuf,
        /// The same bank-integration request as at the start (JSON file).
        #[arg(long)]
        bank: PathBuf,
    },
    /// Ask the service how the enrollment ended, without authenticating again.
    EnrollStatus,
    /// Register the key this member signs recipient decisions with.
    RegisterRecipientKey,
    /// Publish the content key senders encrypt to when writing to this member.
    PublishContentKey,
    /// Show this member's account: keys, payment method and service coverage.
    Account,
    /// Purchase annual service under a published offer version.
    Purchase {
        #[arg(long)]
        offer: u64,
    },
    /// Publish this member's request classes, replacing earlier ones.
    PublishClasses {
        /// Publication version; each publication must be newer than the last.
        #[arg(long)]
        version: u64,
        /// A class as `ID=DESCRIPTION=COLLATERAL`, collateral in minor units.
        #[arg(long = "class", required = true)]
        classes: Vec<String>,
    },
    /// Show what a sender may choose when approaching an address.
    Offer {
        #[arg(long)]
        to: u128,
    },
    /// Send a request: issue terms, authorize one charge, wait for its funding, upload
    /// the encrypted message and submit it. Resumes an unfinished request.
    Request {
        #[arg(long)]
        to: u128,
        #[arg(long)]
        class: u64,
        #[arg(long)]
        message: String,
        /// Development only: record the named step's request but discard its answer, as
        /// if the response had been lost (issue, create, submit).
        #[arg(long)]
        lose_answer_at: Option<String>,
        /// Stop once the charge is funded, before uploading and submitting the message.
        #[arg(long)]
        prepare_only: bool,
    },
    /// Send a follow-up under the current request, when the recipient permits them.
    Followup {
        #[arg(long)]
        to: u128,
        #[arg(long)]
        message: String,
    },
    /// Cancel the current request before it is submitted.
    Cancel {
        #[arg(long)]
        to: u128,
    },
    /// As recipient: permit follow-ups from a sender.
    PermitFollowups {
        #[arg(long)]
        from: u128,
        #[arg(long)]
        max_messages: u32,
        #[arg(long)]
        max_per_interval: u32,
        #[arg(long)]
        interval_ms: u64,
    },
    /// As recipient: decide about a sender.
    Decide {
        #[arg(long)]
        from: u128,
        #[arg(long, value_enum)]
        decision: Decision,
        /// For an express lane: how long it lasts, in milliseconds.
        #[arg(long, default_value_t = 30 * 24 * 60 * 60 * 1000)]
        lane_lifetime_ms: u64,
    },
    /// As recipient: list who is writing, with their requests and decrypted messages.
    Inbox,
    /// Show the relationship with another address and its money, from this side.
    Status {
        #[arg(long)]
        with: u128,
        #[arg(long, value_enum, default_value_t = Side::Sender)]
        side: Side,
    },
    /// Send again every journaled request whose answer is not known.
    Retry,
    /// Print this member's address and public keys.
    Whoami,
}

#[derive(Clone, Copy, ValueEnum)]
enum Decision {
    Accept,
    Lane,
    Reject,
    Block,
    Unblock,
}
#[derive(Clone, Copy, ValueEnum)]
enum Side {
    Sender,
    Recipient,
}

#[allow(clippy::too_many_lines)] // Exhaustive routing to CLI adapters.
fn run(cli: Cli) -> Result<(), String> {
    let home = cli.home;
    match cli.command {
        Command::Init { server } => member::init(&home, server),
        command => {
            let mut member = member::Member::open(&home)?;
            match command {
                Command::Init { .. } => unreachable!(),
                Command::ExternalAccept {
                    conversation,
                    lane_lifetime_ms,
                } => member.external_accept(conversation, lane_lifetime_ms),
                Command::Email {
                    conversation,
                    to,
                    message_file,
                    disclose_to_external_mail,
                    operation,
                } => member.external_send(
                    conversation,
                    Some(&to),
                    &message_file,
                    disclose_to_external_mail,
                    &operation,
                ),
                Command::EmailReply {
                    conversation,
                    message_file,
                    operation,
                } => member.external_send(conversation, None, &message_file, false, &operation),
                Command::ExternalRead {
                    conversation,
                    after,
                } => member.external_read(conversation, after),
                Command::ExternalReject { conversation } => {
                    member.external_decision(conversation, false)
                }
                Command::WithdrawDisclosure { conversation } => {
                    member.external_decision(conversation, true)
                }
                Command::GrantDisclosure {
                    conversation,
                    operation,
                } => member.external_grant_disclosure(conversation, &operation),
                Command::EnrollBegin { bank } => member.enroll_begin(&bank),
                Command::EnrollComplete {
                    oidc_token,
                    login_issuer,
                    login_subject,
                    applicant,
                    bank,
                } => member.enroll_complete(
                    &oidc_token,
                    &login_issuer,
                    &login_subject,
                    &applicant,
                    &bank,
                ),
                Command::EnrollStatus => member.enroll_status(),
                Command::RegisterRecipientKey => member.register_recipient_key(),
                Command::PublishContentKey => member.publish_content_key(),
                Command::Account => member.account(),
                Command::Purchase { offer } => member.purchase(offer),
                Command::PublishClasses { version, classes } => {
                    member.publish_classes(version, &classes)
                }
                Command::Offer { to } => member.offer(to),
                Command::Request {
                    to,
                    class,
                    message,
                    lose_answer_at,
                    prepare_only,
                } => member.request(to, class, &message, lose_answer_at.as_deref(), prepare_only),
                Command::Followup { to, message } => member.followup(to, &message),
                Command::Cancel { to } => member.cancel(to),
                Command::PermitFollowups {
                    from,
                    max_messages,
                    max_per_interval,
                    interval_ms,
                } => member.permit_followups(from, max_messages, max_per_interval, interval_ms),
                Command::Decide {
                    from,
                    decision,
                    lane_lifetime_ms,
                } => member.decide(
                    from,
                    match decision {
                        Decision::Accept => member::Decision::Accept,
                        Decision::Lane => member::Decision::Lane(lane_lifetime_ms),
                        Decision::Reject => member::Decision::Reject,
                        Decision::Block => member::Decision::Block,
                        Decision::Unblock => member::Decision::Unblock,
                    },
                ),
                Command::Inbox => member.inbox(),
                Command::Status { with, side } => {
                    member.status(with, matches!(side, Side::Recipient))
                }
                Command::Retry => member.retry(),
                Command::Whoami => member.whoami(),
            }
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("cs-mail-cli: {message}");
            ExitCode::FAILURE
        }
    }
}
