//! Plain-text rendering of what the service answered.
use cs_mail_application::members::{AccountView, RelationshipView};
use cs_mail_finance::{CaptureStatus, RefundView, RequestFinancialView};
use cs_mail_protocol::{RequestLifecycle, RequestTerms};

fn money(value: cs_mail_primitives::Money) -> String {
    let units = value.minor_units();
    format!("{}.{:02}", units / 100, units % 100)
}

pub fn account(view: &AccountView) {
    println!("account {}", view.account.0);
    for persona in &view.personas {
        println!("  address {}", persona.0);
    }
    println!("  status {:?}, revision {}", view.status, view.revision);
    for key in &view.keys {
        println!(
            "  key {} as {:?}{}{}",
            key.reference.0,
            key.actor,
            if key.manager { ", manager" } else { "" },
            if key.active { "" } else { ", inactive" }
        );
    }
    println!(
        "  annual service: {} ({} contract{})",
        if view.covered {
            "covered now"
        } else {
            "not covering now"
        },
        view.contracts,
        if view.contracts == 1 { "" } else { "s" }
    );
}

pub fn terms(terms: &RequestTerms) {
    let total = terms
        .processing_charge
        .checked_add(terms.collateral)
        .unwrap_or(terms.collateral);
    println!(
        "terms for class {} ({}): C {} + S {} = one charge of {}",
        terms.selected_class.id().0,
        terms.selected_class.description(),
        money(terms.processing_charge),
        money(terms.collateral),
        money(total)
    );
    println!(
        "  submit within {} s of creating the request; the recipient decides within {} s of submission",
        terms.submission_window.0 / 1000,
        terms.decision_window.0 / 1000
    );
    println!(
        "  accept: full refund; reject: C retained, S to the member program; expire: C retained, S refunded; cancel before submission: full refund"
    );
}

fn lifecycle(lifecycle: &RequestLifecycle) -> String {
    match lifecycle {
        RequestLifecycle::PreparingSubmission(p) => {
            format!("preparing submission (deadline {})", p.deadline.0)
        }
        RequestLifecycle::AwaitingRecipientDecision(s) => {
            format!(
                "awaiting the recipient's decision (deadline {})",
                s.decision_deadline.0
            )
        }
        RequestLifecycle::Accepted { permission, .. } => format!("accepted ({permission:?})"),
        RequestLifecycle::Rejected { .. } => "rejected".into(),
        RequestLifecycle::Expired { .. } => "expired".into(),
        RequestLifecycle::Cancelled { .. } => "cancelled".into(),
    }
}

fn financial(view: &RequestFinancialView) {
    let capture = match view.capture {
        CaptureStatus::Pending => "charge pending with the processor (outcome not yet known)",
        CaptureStatus::CancellationRequested => {
            "charge cancellation requested (outcome not yet known)"
        }
        CaptureStatus::FundingFinalized => "charge funded",
        CaptureStatus::Voided => "charge voided",
        CaptureStatus::Reversed => "charge reversed",
    };
    println!("    charge {}: {capture}", money(view.charge));
    match view.refund {
        RefundView::None => {}
        RefundView::Owed(amount) => println!(
            "    refund {} owed to the sender (not yet confirmed by the processor)",
            money(amount)
        ),
        RefundView::Confirmed(amount) => {
            println!("    refund {} confirmed by the processor", money(amount));
        }
    }
    if !view.processing_retained.is_zero() {
        println!(
            "    processing revenue retained by CSQD: {}",
            money(view.processing_retained)
        );
    }
    if !view.collateral_forfeited.is_zero() {
        println!(
            "    collateral pending forfeiture to the member program: {}",
            money(view.collateral_forfeited)
        );
    }
    if view.awaiting_processor {
        println!("    uncertain: the processor has not confirmed every operation yet");
    }
}

/// A sender as a recipient knows them: a guest by the mailbox they verified.
pub fn sender(view: &RelationshipView) -> String {
    view.guest.as_ref().map_or_else(
        || view.handle.sender.0.to_string(),
        |mailbox| format!("{} (guest {})", view.handle.sender.0, mailbox.address()),
    )
}

pub fn relationship(view: &RelationshipView, as_recipient: bool) {
    let other = if as_recipient {
        format!("from {}", sender(view))
    } else {
        format!("to {}", view.handle.recipient.0)
    };
    println!("relationship {other}: {:?}", view.state);
    if view.followup_policy.max_messages > 0 {
        println!(
            "  follow-ups permitted: up to {}",
            view.followup_policy.max_messages
        );
    }
    if let Some(lane) = &view.lane {
        println!(
            "  express lane {:?} until {}",
            lane.state(),
            lane.grant().not_after.0
        );
    }
    for request in &view.requests {
        println!(
            "  request {}: {}, {} message{}",
            request.id.0,
            lifecycle(&request.lifecycle),
            request.messages,
            if request.messages == 1 { "" } else { "s" }
        );
        if let Some(money) = &request.money {
            financial(money);
        }
    }
}
