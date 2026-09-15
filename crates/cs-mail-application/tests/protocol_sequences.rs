use cs_mail_application::{EngineError, InMemoryEngine};
use cs_mail_finance::{
    FinancialTerms, PaymentError, PaymentOutcome, PaymentProcessor, SimulatedProcessor,
};
use cs_mail_ledger::Account;
use cs_mail_primitives::*;
use cs_mail_protocol::*;

#[test]
fn serialized_requests_use_submission_terminology() {
    let mut f = Fixture::new();
    f.create(1, 1);
    let snapshot = f.engine.snapshot().unwrap();
    let request = &snapshot.state.requests[&RequestId(1)];
    assert_eq!(
        serde_json::to_value(request.lifecycle).unwrap(),
        serde_json::json!({"PreparingSubmission": {"deadline": 11}})
    );
    assert_eq!(
        serde_json::to_value(&f.policy).unwrap()["submission_window"],
        10
    );
    assert_eq!(
        serde_json::to_value(&request.terms).unwrap()["submission_window"],
        10
    );
    let history = serde_json::to_value(&snapshot.history).unwrap();
    assert!(history["pending_submission"].is_object());
    assert_eq!(history["earliest_next_submission"], 0);

    f.payment(1, false, 2);
    let result = f.submit(1, 3).unwrap();
    let submission = serde_json::json!({"at": 3, "decision_deadline": 53});
    assert_eq!(
        serde_json::to_value(f.engine.snapshot().unwrap().state.requests[&RequestId(1)].lifecycle)
            .unwrap(),
        serde_json::json!({"AwaitingRecipientDecision": submission})
    );
    assert!(result.transition.protocol_events.iter().any(
        |e| serde_json::to_value(e.kind).unwrap() == serde_json::json!({"RequestSubmitted": 1})
    ));
    f.decision(true, 4);
    let lifecycle =
        serde_json::to_value(f.engine.snapshot().unwrap().state.requests[&RequestId(1)].lifecycle)
            .unwrap();
    assert_eq!(lifecycle["Accepted"]["submission"], submission);

    let command = ProtocolCommand::SubmitRequestToRecipient {
        request_id: RequestId(1),
        expected_request_version: Version(0),
        content_ref: ContentRef(1),
        delivery_intent_ref: DeliveryIntentRef(1),
        declaration_digest: MessageDeclarationDigest([0; 32]),
        message_valid_until: MessageValidityUntil(CanonicalTime(1000)),
    };
    assert!(serde_json::to_value(command).unwrap()["SubmitRequestToRecipient"].is_object());
    let command = ProtocolCommand::CancelRequestSubmission {
        request_id: RequestId(1),
        expected_request_version: Version(0),
        reason: CancellationReason::SubmissionTimeout,
    };
    assert_eq!(
        serde_json::to_value(command).unwrap(),
        serde_json::json!({
            "CancelRequestSubmission": {"request_id": 1, "expected_request_version": 0, "reason": "SubmissionTimeout"}
        })
    );
    assert_eq!(
        serde_json::to_value(CancellationReason::PreSubmissionFailure).unwrap(),
        "PreSubmissionFailure"
    );
    assert_eq!(
        serde_json::to_value(ProtocolError::SubmissionWindowClosed).unwrap(),
        "SubmissionWindowClosed"
    );
    assert_eq!(
        serde_json::to_value(ProtocolError::RequestSubmissionCancelled).unwrap(),
        "RequestSubmissionCancelled"
    );
    assert_eq!(
        serde_json::to_value(ProtocolEventKind::RequestSubmissionCancelled(RequestId(1))).unwrap(),
        serde_json::json!({"RequestSubmissionCancelled": 1})
    );

    let mut f = Fixture::new();
    f.create(1, 1);
    f.cancel(2);
    let lifecycle =
        serde_json::to_value(f.engine.snapshot().unwrap().state.requests[&RequestId(1)].lifecycle)
            .unwrap();
    assert_eq!(
        lifecycle["Cancelled"]["submission_preparation"],
        serde_json::json!({"deadline": 11})
    );
}

fn policy() -> PolicySnapshot {
    PolicySnapshot {
        pricing_policy_version: cs_mail_primitives::PolicyVersion(1),
        protocol_version: ProtocolVersion(2),
        policy_version: PolicyVersion(1),
        privacy_profile_version: PrivacyProfileVersion(1),
        retention_policy_version: RetentionPolicyVersion(1),
        recipient_provider: ProviderRef(30),
        unit: SettlementUnit(1),
        processing_charge: Money::from_minor_units(2),
        collateral: Money::from_minor_units(8),
        submission_window: Duration(10),
        decision_window: Duration(50),
        quote_lifetime: Duration(20),
        backoff: vec![Duration(0), Duration(5)],
        financial: FinancialTerms {
            scope: cs_mail_finance::FinancialScope::new(
                [7; 32],
                cs_mail_primitives::ProviderRef(30),
                cs_mail_primitives::ProgramRef(1),
                [9; 32],
                cs_mail_primitives::ProtocolVersion(2),
            ),
            policy_version: PolicyVersion(1),
            corporate_basis_points: 300,
            maturity_delay: Duration(10),
        },
        payment_provider_key: SimulatedProcessor::new([7; 32]).verifying_key(),
        expiry_cooldown: Duration(30),
        rejection_cooldown: Duration(90),
    }
}
struct Fixture {
    engine: InMemoryEngine,
    provider: SimulatedProcessor,
    policy: PolicySnapshot,
    next_key: u128,
}
impl Fixture {
    fn new() -> Self {
        Self {
            engine: InMemoryEngine::new(
                ProtocolState::initial(
                    PrincipalRef(1),
                    ProtocolIdentity(10),
                    ProtocolIdentity(20),
                    CanonicalTime(0),
                ),
                SettlementUnit(1),
                cs_mail_finance::FinancialScope::new(
                    [7; 32],
                    cs_mail_primitives::ProviderRef(30),
                    cs_mail_primitives::ProgramRef(1),
                    [9; 32],
                    cs_mail_primitives::ProtocolVersion(2),
                ),
            )
            .unwrap(),
            provider: SimulatedProcessor::new([7; 32]),
            policy: policy(),
            next_key: 1,
        }
    }
    fn execute(
        &mut self,
        actor: ActorRef,
        command: ProtocolCommand,
        at: u64,
    ) -> Result<cs_mail_application::ExecutionOutcome, EngineError> {
        let key = IdempotencyKey(self.next_key);
        self.next_key += 1;
        self.engine.execute(
            KernelCommand::new(command, actor, OperationalKeyRef(1), key),
            CanonicalTime(at),
            self.policy.clone(),
        )
    }
    fn sender(
        &mut self,
        command: ProtocolCommand,
        at: u64,
    ) -> Result<cs_mail_application::ExecutionOutcome, EngineError> {
        let sender = self
            .engine
            .snapshot()
            .unwrap()
            .state
            .relationship
            .key
            .sender;
        self.execute(ActorRef::Sender(sender), command, at)
    }
    fn create(&mut self, id: u128, at: u64) -> RequestTerms {
        let result = self
            .sender(
                ProtocolCommand::IssueRequestTerms {
                    quote_id: QuoteId(id),
                    declaration_digest: None,
                },
                at,
            )
            .unwrap();
        let TermsOutcome::ChargeRequired(terms) = result.transition.terms_outcome.unwrap() else {
            panic!("expected new request terms")
        };
        self.sender(
            ProtocolCommand::CreateRequest {
                request_id: RequestId(id),

                message_id: MessageId(id),
                payment_method: [9; 32],
                terms: terms.clone(),
            },
            at,
        )
        .unwrap();
        *terms
    }
    fn payment(&mut self, id: u128, refund: bool, at: u64) {
        let snapshot = self.engine.snapshot().unwrap();
        let r = &snapshot.payments[&RequestId(id)];
        let op = if refund {
            r.refund().operation().unwrap().clone()
        } else {
            r.capture().clone()
        };
        let cancel =
            !refund && r.capture_status() == cs_mail_finance::CaptureStatus::CancellationRequested;
        let receipt = if cancel {
            self.provider
                .cancel_capture(&cs_mail_finance::CaptureCancellation::new(op).unwrap())
                .unwrap()
        } else {
            self.provider
                .lookup(op.id)
                .unwrap()
                .unwrap_or_else(|| self.provider.submit(&op).unwrap())
        };
        self.execute(
            ActorRef::Provider(ProviderRef(30)),
            ProtocolCommand::RecordPayment {
                request_id: RequestId(id),
                receipt,
            },
            at,
        )
        .unwrap();
    }
    fn submit(
        &mut self,
        id: u128,
        at: u64,
    ) -> Result<cs_mail_application::ExecutionOutcome, EngineError> {
        let v = self.engine.snapshot().unwrap().state.requests[&RequestId(id)].version;
        self.sender(
            ProtocolCommand::SubmitRequestToRecipient {
                request_id: RequestId(id),
                expected_request_version: v.into(),
                content_ref: ContentRef(id),
                delivery_intent_ref: DeliveryIntentRef(id),
                declaration_digest: MessageDeclarationDigest([0; 32]),
                message_valid_until: MessageValidityUntil(CanonicalTime(1000)),
            },
            at,
        )
    }
    fn submit_initial_request(&mut self) {
        self.create(1, 1);
        self.payment(1, false, 2);
        self.submit(1, 3).unwrap();
    }
    fn decision(&mut self, accept: bool, at: u64) {
        let v = self.engine.snapshot().unwrap().state.relationship.version;
        let cmd = if accept {
            ProtocolCommand::AcceptRelationship {
                expected_version: v.into(),
            }
        } else {
            ProtocolCommand::RejectRelationship {
                expected_version: v.into(),
            }
        };
        self.execute(ActorRef::Recipient(ProtocolIdentity(20)), cmd, at)
            .unwrap();
    }
    fn cancel(&mut self, at: u64) {
        let v = self.engine.snapshot().unwrap().state.requests[&RequestId(1)].version;
        self.sender(
            ProtocolCommand::CancelRequestSubmission {
                request_id: RequestId(1),
                expected_request_version: v.into(),
                reason: CancellationReason::SenderRequested,
            },
            at,
        )
        .unwrap();
    }
}
#[test]
fn one_request_one_charge_and_quote_cannot_be_reused() {
    let mut f = Fixture::new();
    let terms = f.create(1, 1);
    let before = f.engine.snapshot().unwrap();
    assert!(matches!(
        f.sender(
            ProtocolCommand::CreateRequest {
                request_id: RequestId(2),

                message_id: MessageId(2),
                payment_method: [9; 32],
                terms: Box::new(terms)
            },
            2
        ),
        Err(EngineError::Protocol(ProtocolError::RequestAlreadyExists))
    ));
    assert_eq!(f.engine.snapshot().unwrap(), before);
    let result = f
        .sender(
            ProtocolCommand::IssueRequestTerms {
                quote_id: QuoteId(2),
                declaration_digest: None,
            },
            2,
        )
        .unwrap();
    assert_eq!(
        result.transition.terms_outcome,
        Some(TermsOutcome::ExistingRequest(RequestId(1)))
    );
    assert_eq!(
        f.engine
            .outbox()
            .unwrap()
            .iter()
            .filter(|i| matches!(i, EffectIntent::ExecutePayment { .. }))
            .count(),
        1
    );
}
#[test]
fn submission_requires_verified_capture_and_policy_changes_do_not_reprice() {
    let mut f = Fixture::new();
    f.create(1, 1);
    assert!(matches!(
        f.submit(1, 2),
        Err(EngineError::Protocol(ProtocolError::PaymentNotConfirmed))
    ));
    f.policy.policy_version = PolicyVersion(2);
    f.policy.processing_charge = Money::from_minor_units(900);
    f.policy.backoff = vec![Duration(0), Duration(900)];
    f.payment(1, false, 2);
    f.submit(1, 3).unwrap();
    let s = f.engine.snapshot().unwrap();
    assert_eq!(
        s.payments[&RequestId(1)].capture().amount,
        Money::from_minor_units(10)
    );
    assert_eq!(s.history.earliest_next_submission, CanonicalTime(8));
}
#[test]
fn acceptance_records_full_refund_until_provider_confirms() {
    let mut f = Fixture::new();
    f.submit_initial_request();
    f.decision(true, 4);
    let s = f.engine.snapshot().unwrap();
    let request = &s.state.requests[&RequestId(1)];
    let refund = s.payments[&request.id].refund().operation().unwrap();
    assert!(!matches!(
        s.payments[&request.id].refund(),
        cs_mail_finance::RefundStatus::Confirmed(_)
    ));
    assert_eq!(
        s.ledger.balance(Account::RefundPayable(refund.id)),
        Money::from_minor_units(10)
    );
    f.payment(1, true, 5);
    f.payment(1, true, 6);
    assert_eq!(f.provider.operation_count(), 2);
    assert!(matches!(
        f.engine.snapshot().unwrap().payments[&RequestId(1)].refund(),
        cs_mail_finance::RefundStatus::Confirmed(_)
    ));
    assert_eq!(f.engine.total_value().unwrap(), Money::ZERO);
}
#[test]
fn rejection_exports_pending_forfeiture_and_starts_cooldown() {
    let mut f = Fixture::new();
    f.submit_initial_request();
    f.decision(false, 40);
    let s = f.engine.snapshot().unwrap();
    assert_eq!(s.history.earliest_next_submission, CanonicalTime(130));
    assert_eq!(
        s.ledger.balance(Account::ProcessingRevenue),
        Money::from_minor_units(2)
    );
    assert!(s.payments[&RequestId(1)].refund().operation().is_none());
    let program = f.engine.financial_program().unwrap();
    assert_eq!(
        program.ledger().balance(Account::PendingForfeiture(
            s.payments[&RequestId(1)].capture().id
        )),
        Money::from_minor_units(8)
    );
    assert_eq!(
        program.ledger().balance(Account::RestrictedMemberFunds),
        Money::ZERO
    );
    assert!(matches!(
        f.sender(
            ProtocolCommand::IssueRequestTerms {
                quote_id: QuoteId(2),
                declaration_digest: None
            },
            41
        ),
        Err(EngineError::Protocol(ProtocolError::BackoffActive { .. }))
    ));
}
#[test]
fn uncaptured_cancellation_voids_without_advancing_history() {
    let mut f = Fixture::new();
    f.create(1, 1);
    f.cancel(2);
    f.payment(1, false, 3);
    let s = f.engine.snapshot().unwrap();
    assert!(s.payments[&RequestId(1)].capture_voided());
    assert_eq!(s.history.level, 0);
    assert!(s.payments[&RequestId(1)].refund().operation().is_none());
    assert!(f.submit(1, 4).is_err());
}
#[test]
fn late_capture_after_cancellation_creates_full_refund_without_reopening() {
    let mut f = Fixture::new();
    f.create(1, 1);
    let op = f.engine.snapshot().unwrap().payments[&RequestId(1)]
        .capture()
        .clone();
    let receipt = f.provider.submit(&op).unwrap();
    f.cancel(2);
    f.execute(
        ActorRef::Provider(ProviderRef(30)),
        ProtocolCommand::RecordPayment {
            request_id: RequestId(1),
            receipt,
        },
        3,
    )
    .unwrap();
    f.payment(1, true, 4);
    let s = f.engine.snapshot().unwrap();
    let r = &s.state.requests[&RequestId(1)];
    assert!(matches!(r.lifecycle, RequestLifecycle::Cancelled { .. }));
    assert_eq!(
        s.payments[&r.id].refund().operation().unwrap().amount,
        Money::from_minor_units(10)
    );
    assert!(matches!(
        s.payments[&r.id].refund(),
        cs_mail_finance::RefundStatus::Confirmed(_)
    ));
    assert_eq!(s.history.level, 0);
}
#[test]
fn ambiguous_capture_response_is_reconciled_with_original_operation() {
    let mut f = Fixture::new();
    f.create(1, 1);
    let op = f.engine.snapshot().unwrap().payments[&RequestId(1)]
        .capture()
        .clone();
    f.provider.lose_next_response();
    assert_eq!(f.provider.submit(&op), Err(PaymentError::Unavailable));
    f.payment(1, false, 2);
    f.submit(1, 3).unwrap();
    assert_eq!(f.provider.operation_count(), 1);
}
#[test]
fn capture_and_refund_evidence_cannot_change_amount_or_provider() {
    let mut f = Fixture::new();
    f.create(1, 1);
    let mut op = f.engine.snapshot().unwrap().payments[&RequestId(1)]
        .capture()
        .clone();
    op.amount = Money::from_minor_units(1);
    let receipt = f.provider.submit(&op).unwrap();
    let before = f.engine.snapshot().unwrap();
    assert!(
        f.execute(
            ActorRef::Provider(ProviderRef(30)),
            ProtocolCommand::RecordPayment {
                request_id: RequestId(1),
                receipt
            },
            2
        )
        .is_err()
    );
    assert_eq!(before, f.engine.snapshot().unwrap());
}
#[test]
fn expiry_refunds_collateral_once_and_late_acceptance_only_changes_permission() {
    let mut f = Fixture::new();
    f.submit_initial_request();
    f.decision(true, 54);
    let s = f.engine.snapshot().unwrap();
    let r = &s.state.requests[&RequestId(1)];
    assert!(matches!(r.lifecycle, RequestLifecycle::Expired { .. }));
    assert_eq!(
        s.payments[&r.id].refund().operation().unwrap().amount,
        Money::from_minor_units(8)
    );
    assert_eq!(s.history.earliest_next_submission, CanonicalTime(83));
    assert_eq!(s.state.relationship.state, RelationshipState::Accepted);
    f.payment(1, true, 55);
    assert_eq!(
        f.engine
            .snapshot()
            .unwrap()
            .ledger
            .balance(Account::ProcessingRevenue),
        Money::from_minor_units(2)
    );
}
#[test]
fn followups_use_current_policy_without_changing_charge_deadline_or_history() {
    let mut f = Fixture::new();
    f.submit_initial_request();
    f.execute(
        ActorRef::Recipient(ProtocolIdentity(20)),
        ProtocolCommand::SetFollowupPolicy {
            expected_version: Version(0),
            policy: FollowupPolicy {
                version: Version(0),
                max_messages: 2,
                max_per_interval: 1,
                interval: Duration(5),
            },
        },
        4,
    )
    .unwrap();
    let initial = f.engine.snapshot().unwrap();
    let command = |message| ProtocolCommand::AdmitFollowup {
        request_id: RequestId(1),
        message_id: MessageId(message),
        content_ref: ContentRef(message),
        delivery_intent_ref: DeliveryIntentRef(message),
        declaration_digest: MessageDeclarationDigest([0; 32]),
        message_valid_until: MessageValidityUntil(CanonicalTime(100)),
        expected_policy_version: Version(1),
    };
    f.sender(command(2), 5).unwrap();
    assert!(matches!(
        f.sender(command(3), 6),
        Err(EngineError::Protocol(ProtocolError::FollowupNotAllowed))
    ));
    f.sender(command(3), 10).unwrap();
    assert!(f.sender(command(4), 15).is_err());
    let final_state = f.engine.snapshot().unwrap();
    assert_eq!(initial.history, final_state.history);
    assert_eq!(
        initial.state.requests[&RequestId(1)]
            .lifecycle
            .submission()
            .unwrap()
            .decision_deadline,
        final_state.state.requests[&RequestId(1)]
            .lifecycle
            .submission()
            .unwrap()
            .decision_deadline
    );
    assert_eq!(f.provider.operation_count(), 1);
    f.decision(true, 16);
    assert!(f.sender(command(5), 17).is_err());
}
#[test]
fn exact_replay_and_conflicting_decisions_are_atomic() {
    let mut f = Fixture::new();
    f.submit_initial_request();
    let command = KernelCommand::new(
        ProtocolCommand::AcceptRelationship {
            expected_version: Version(0),
        },
        ActorRef::Recipient(ProtocolIdentity(20)),
        OperationalKeyRef(1),
        IdempotencyKey(900),
    );
    let first = f
        .engine
        .execute(command.clone(), CanonicalTime(4), policy())
        .unwrap();
    let second = f
        .engine
        .execute(command, CanonicalTime(5), policy())
        .unwrap();
    assert_eq!(first.transition, second.transition);
    assert!(second.replayed);
    let before = f.engine.snapshot().unwrap();
    assert!(
        f.execute(
            ActorRef::Recipient(ProtocolIdentity(20)),
            ProtocolCommand::RejectRelationship {
                expected_version: Version(0)
            },
            5
        )
        .is_err()
    );
    assert_eq!(before, f.engine.snapshot().unwrap());
}
#[test]
fn unauthorized_decisions_and_invalid_policy_are_rejected() {
    let mut f = Fixture::new();
    f.submit_initial_request();
    assert!(
        f.sender(
            ProtocolCommand::AcceptRelationship {
                expected_version: Version(0)
            },
            4
        )
        .is_err()
    );
    f.policy.financial.corporate_basis_points = 10_001;
    assert!(matches!(
        f.sender(
            ProtocolCommand::IssueRequestTerms {
                quote_id: QuoteId(2),
                declaration_digest: None
            },
            5
        ),
        Err(EngineError::Protocol(ProtocolError::PolicyInvalid))
    ));
}
#[test]
fn reversal_is_compensating_and_cannot_rewrite_final_outcome() {
    let mut f = Fixture::new();
    f.submit_initial_request();
    f.decision(false, 4);
    let before = f.engine.snapshot().unwrap();
    let id = before.payments[&RequestId(1)].capture().id;
    let receipt = f.provider.reversal(id, FinancialEventId(901)).unwrap();
    assert_eq!(receipt.evidence.outcome, PaymentOutcome::Reversed);
    f.execute(
        ActorRef::Provider(ProviderRef(30)),
        ProtocolCommand::RecordPayment {
            request_id: RequestId(1),
            receipt,
        },
        5,
    )
    .unwrap();
    let after = f.engine.snapshot().unwrap();
    assert_eq!(
        before.state.requests[&RequestId(1)].lifecycle,
        after.state.requests[&RequestId(1)].lifecycle
    );
    assert_eq!(before.state.relationship, after.state.relationship);
    assert_eq!(
        after.ledger.signed_balance(Account::CorporateLossClearing),
        -10
    );
}

#[test]
fn concurrent_creations_commit_only_one_charge_intent() {
    use std::sync::{Arc, Barrier};
    let mut f = Fixture::new();
    let issued = f
        .sender(
            ProtocolCommand::IssueRequestTerms {
                quote_id: QuoteId(1),
                declaration_digest: None,
            },
            1,
        )
        .unwrap();
    let TermsOutcome::ChargeRequired(terms) = issued.transition.terms_outcome.unwrap() else {
        panic!("expected terms")
    };
    let engine = Arc::new(f.engine);
    let barrier = Arc::new(Barrier::new(2));
    let threads: Vec<_> = (1..=2)
        .map(|id| {
            let engine = engine.clone();
            let barrier = barrier.clone();
            let terms = terms.clone();
            std::thread::spawn(move || {
                barrier.wait();
                engine.execute(
                    KernelCommand::new(
                        ProtocolCommand::CreateRequest {
                            request_id: RequestId(id),

                            message_id: MessageId(id),
                            payment_method: [9; 32],
                            terms,
                        },
                        ActorRef::Sender(ProtocolIdentity(10)),
                        OperationalKeyRef(1),
                        IdempotencyKey(100 + id),
                    ),
                    CanonicalTime(2),
                    policy(),
                )
            })
        })
        .collect();
    let outcomes: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(outcomes.iter().filter(|o| o.is_ok()).count(), 1);
    assert_eq!(engine.snapshot().unwrap().state.requests.len(), 1);
    assert_eq!(
        engine
            .outbox()
            .unwrap()
            .iter()
            .filter(|i| matches!(i, EffectIntent::ExecutePayment { .. }))
            .count(),
        1
    );
}

#[test]
fn capture_confirmation_and_cancellation_converge_to_one_refund() {
    use std::sync::{Arc, Barrier};
    let mut f = Fixture::new();
    f.create(1, 1);
    let operation = f.engine.snapshot().unwrap().payments[&RequestId(1)]
        .capture()
        .clone();
    let receipt = f.provider.submit(&operation).unwrap();
    let engine = Arc::new(f.engine);
    let barrier = Arc::new(Barrier::new(2));
    let commands = [
        (
            ActorRef::Sender(ProtocolIdentity(10)),
            ProtocolCommand::CancelRequestSubmission {
                request_id: RequestId(1),
                expected_request_version: Version(0),
                reason: CancellationReason::SenderRequested,
            },
        ),
        (
            ActorRef::Provider(ProviderRef(30)),
            ProtocolCommand::RecordPayment {
                request_id: RequestId(1),
                receipt,
            },
        ),
    ];
    let threads: Vec<_> = commands
        .into_iter()
        .enumerate()
        .map(|(i, (actor, command))| {
            let engine = engine.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                engine.execute(
                    KernelCommand::new(
                        command,
                        actor,
                        OperationalKeyRef(1),
                        IdempotencyKey(900 + i as u128),
                    ),
                    CanonicalTime(2),
                    policy(),
                )
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap().unwrap();
    }
    let snapshot = engine.snapshot().unwrap();
    let request = &snapshot.state.requests[&RequestId(1)];
    assert!(matches!(
        request.lifecycle,
        RequestLifecycle::Cancelled { .. }
    ));
    assert_eq!(
        snapshot.ledger.balance(Account::RefundPayable(
            snapshot.payments[&request.id]
                .refund()
                .operation()
                .unwrap()
                .id
        )),
        Money::from_minor_units(10)
    );
    assert_eq!(snapshot.history.level, 0);
}

#[test]
fn captured_submission_preparation_block_and_message_failure_refund_without_submission() {
    let mut f = Fixture::new();
    f.create(1, 1);
    f.payment(1, false, 2);
    f.execute(
        ActorRef::Recipient(ProtocolIdentity(20)),
        ProtocolCommand::BlockRelationship {
            expected_version: Version(0),
        },
        3,
    )
    .unwrap();
    let s = f.engine.snapshot().unwrap();
    assert!(matches!(
        s.state.requests[&RequestId(1)].lifecycle,
        RequestLifecycle::Cancelled { .. }
    ));
    assert_eq!(s.history.level, 0);
    assert!(f.submit(1, 4).is_err());
    f.execute(
        ActorRef::Recipient(ProtocolIdentity(20)),
        ProtocolCommand::UnblockRelationship {
            expected_version: s.state.relationship.version.into(),
        },
        4,
    )
    .unwrap();
    assert_eq!(
        f.engine.snapshot().unwrap().state.relationship.state,
        RelationshipState::Rejected
    );
    let mut g = Fixture::new();
    g.create(1, 1);
    g.payment(1, false, 2);
    g.sender(
        ProtocolCommand::SubmitRequestToRecipient {
            request_id: RequestId(1),
            expected_request_version: Version(0),
            content_ref: ContentRef(1),
            delivery_intent_ref: DeliveryIntentRef(1),
            declaration_digest: MessageDeclarationDigest([0; 32]),
            message_valid_until: MessageValidityUntil(CanonicalTime(1)),
        },
        3,
    )
    .unwrap();
    let s = g.engine.snapshot().unwrap();
    assert!(matches!(
        s.state.requests[&RequestId(1)].lifecycle,
        RequestLifecycle::Cancelled { .. }
    ));
    assert_eq!(s.history.level, 0);
    assert!(s.payments[&RequestId(1)].refund().operation().is_some());
}

fn aliases() -> (Fixture, Fixture) {
    let store = cs_mail_application::InMemoryStore::new(cs_mail_finance::FinancialScope::new(
        [7; 32],
        cs_mail_primitives::ProviderRef(30),
        cs_mail_primitives::ProgramRef(1),
        [9; 32],
        cs_mail_primitives::ProtocolVersion(2),
    ));
    let make = |sender| Fixture {
        engine: store
            .register(
                ProtocolState::initial(
                    PrincipalRef(1),
                    ProtocolIdentity(sender),
                    ProtocolIdentity(20),
                    CanonicalTime(0),
                ),
                SettlementUnit(1),
            )
            .unwrap(),
        provider: SimulatedProcessor::new([7; 32]),
        policy: policy(),
        next_key: 1,
    };
    (make(10), make(11))
}

#[test]
fn alias_history_change_cancels_submission_preparation_without_shortening_cooldown() {
    let (mut a, mut b) = aliases();
    a.submit_initial_request();
    // B shares A's history but its own relationship and request identity.
    b.create(1, 8);
    b.payment(1, false, 8);
    a.decision(false, 9);
    let before = b.engine.snapshot().unwrap();
    assert_eq!(before.history.earliest_next_submission, CanonicalTime(99));
    let cancelled = b.submit(1, 10).unwrap();
    assert!(
        cancelled
            .transition
            .protocol_events
            .iter()
            .any(|e| e.kind == ProtocolEventKind::RequestHistoryChanged(RequestId(1)))
    );
    let after = b.engine.snapshot().unwrap();
    assert!(matches!(
        after.state.requests[&RequestId(1)].lifecycle,
        RequestLifecycle::Cancelled { .. }
    ));
    assert_eq!(after.history.level, 1);
    assert_eq!(after.history.earliest_next_submission, CanonicalTime(99));
    assert!(after.history.pending_submission.is_none());
    assert!(after.messages.is_empty());
    b.payment(1, true, 11);
    assert!(matches!(
        b.engine.snapshot().unwrap().payments[&RequestId(1)].refund(),
        cs_mail_finance::RefundStatus::Confirmed(_)
    ));
    assert_eq!(
        a.engine.financial_program().unwrap(),
        b.engine.financial_program().unwrap()
    );
}

#[test]
fn concurrent_aliases_reserve_exactly_one_pending_submission() {
    use std::sync::{Arc, Barrier};
    let (mut a, mut b) = aliases();
    let quote = |f: &mut Fixture| {
        let result = f
            .sender(
                ProtocolCommand::IssueRequestTerms {
                    quote_id: QuoteId(1),
                    declaration_digest: None,
                },
                1,
            )
            .unwrap();
        let Some(TermsOutcome::ChargeRequired(terms)) = result.transition.terms_outcome else {
            panic!("expected terms");
        };
        terms
    };
    let qa = quote(&mut a);
    let qb = quote(&mut b);
    let barrier = Arc::new(Barrier::new(2));
    let threads: Vec<_> = [(a, qa), (b, qb)]
        .into_iter()
        .map(|(mut f, terms)| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let result = f.sender(
                    ProtocolCommand::CreateRequest {
                        request_id: RequestId(1),
                        message_id: MessageId(1),
                        payment_method: [9; 32],
                        terms,
                    },
                    2,
                );
                (f, result)
            })
        })
        .collect();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|(_, r)| r.is_ok()).count(), 1);
    let winner = &results.iter().find(|(_, r)| r.is_ok()).unwrap().0;
    let shared = winner.engine.snapshot().unwrap().history;
    assert_eq!(
        shared.pending_submission.unwrap().relationship,
        winner
            .engine
            .snapshot()
            .unwrap()
            .state
            .relationship
            .key
            .reference
    );
    for (f, _) in results {
        assert_eq!(f.engine.snapshot().unwrap().history, shared);
    }
}

#[test]
fn three_messages_are_one_request_one_solicitation_and_one_refund() {
    let mut f = Fixture::new();
    f.submit_initial_request();
    f.execute(
        ActorRef::Recipient(ProtocolIdentity(20)),
        ProtocolCommand::SetFollowupPolicy {
            expected_version: Version(0),
            policy: FollowupPolicy {
                version: Version(0),
                max_messages: 2,
                max_per_interval: 2,
                interval: Duration(10),
            },
        },
        4,
    )
    .unwrap();
    let original = f.engine.snapshot().unwrap().state.requests[&RequestId(1)].clone();
    for id in 2..=3 {
        f.sender(
            ProtocolCommand::AdmitFollowup {
                request_id: RequestId(1),
                message_id: MessageId(id),
                content_ref: ContentRef(id),
                delivery_intent_ref: DeliveryIntentRef(id),
                declaration_digest: MessageDeclarationDigest([u8::try_from(id).unwrap(); 32]),
                message_valid_until: MessageValidityUntil(CanonicalTime(100)),
                expected_policy_version: Version(1),
            },
            5,
        )
        .unwrap();
    }
    let snapshot = f.engine.snapshot().unwrap();
    assert_eq!(snapshot.state.requests.len(), 1);
    assert_eq!(snapshot.state.requests[&RequestId(1)], original);
    assert_eq!(snapshot.messages.len(), 3);
    assert_eq!(snapshot.payments.len(), 1);
    assert_eq!(
        f.engine
            .outbox()
            .unwrap()
            .iter()
            .filter(|e| matches!(
                e,
                EffectIntent::EstablishRequestSolicitation {
                    request_id: RequestId(1),
                    generation: 1
                }
            ))
            .count(),
        1
    );
    f.decision(true, 6);
    let accepted = f.engine.snapshot().unwrap();
    assert!(
        matches!(accepted.state.requests[&RequestId(1)].lifecycle, RequestLifecycle::Accepted { submission, .. } if Some(submission) == original.lifecycle.submission())
    );
    assert!(matches!(
        accepted.payments[&RequestId(1)].refund(),
        cs_mail_finance::RefundStatus::Pending(_)
    ));
    f.payment(1, true, 7);
    let paid = f.engine.snapshot().unwrap();
    assert_eq!(paid.state.requests, accepted.state.requests);
    assert_eq!(paid.messages, accepted.messages);
    assert_eq!(f.provider.operation_count(), 2);
}

#[test]
fn domain_snapshots_round_trip_without_embedding_shared_or_financial_owners() {
    let mut f = Fixture::new();
    f.create(1, 1);
    let mut snapshots = vec![f.engine.snapshot().unwrap()];
    f.payment(1, false, 2);
    f.submit(1, 3).unwrap();
    snapshots.push(f.engine.snapshot().unwrap());
    f.decision(true, 4);
    snapshots.push(f.engine.snapshot().unwrap());
    f.payment(1, true, 5);
    snapshots.push(f.engine.snapshot().unwrap());
    for snapshot in snapshots {
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        let restored: SettlementSnapshot = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored, snapshot);
        let state = serde_json::to_value(&snapshot.state).unwrap();
        assert!(state.get("history").is_none());
        assert!(state.get("payments").is_none());
        assert!(state.get("messages").is_none());
        assert!(state["requests"]["1"].get("capture").is_none());
    }
    // AwaitingRecipientDecision and resolved requests must carry their submission facts.
    assert!(
        serde_json::from_value::<RequestLifecycle>(serde_json::json!("AwaitingRecipientDecision"))
            .is_err()
    );
    assert!(
        serde_json::from_value::<RequestLifecycle>(serde_json::json!({"Accepted": {"event": 1}}))
            .is_err()
    );
}

#[test]
fn initial_message_refusal_cancels_request_submission_with_void_or_full_refund() {
    use cs_mail_protocol::admission::AdmissionFailure;
    for captured in [false, true] {
        let mut f = Fixture::new();
        f.create(1, 1);
        if captured {
            f.payment(1, false, 2);
        }
        let snapshot = f.engine.snapshot().unwrap();
        let command = KernelCommand::new(
            ProtocolCommand::SubmitRequestToRecipient {
                request_id: RequestId(1),
                expected_request_version: Version(0),
                content_ref: ContentRef(1),
                delivery_intent_ref: DeliveryIntentRef(1),
                declaration_digest: MessageDeclarationDigest([0; 32]),
                message_valid_until: MessageValidityUntil(CanonicalTime(100)),
            },
            ActorRef::Sender(ProtocolIdentity(10)),
            OperationalKeyRef(1),
            IdempotencyKey(99),
        );
        let context = TransitionContext {
            now: CanonicalTime(3),
            journal_position: JournalPosition(99),
            protocol_version: ProtocolVersion(2),
            policy: f.policy,
            admission: Err(AdmissionFailure::UnsupportedCriticalExtension),
        };
        let manifest = transition(&snapshot, &command, &context).unwrap();
        assert!(matches!(
            manifest.next_state.requests[&RequestId(1)].lifecycle,
            RequestLifecycle::Cancelled { .. }
        ));
        assert_eq!(manifest.next_history.level, snapshot.history.level);
        assert!(manifest.next_history.pending_submission.is_none());
        assert!(manifest.next_messages.is_empty());
        let financials = &manifest.next_payments[&RequestId(1)];
        if captured {
            assert_eq!(
                financials.refund().operation().unwrap().amount,
                Money::from_minor_units(10)
            );
        } else {
            assert_eq!(
                financials.capture_status(),
                cs_mail_finance::CaptureStatus::CancellationRequested
            );
            assert!(financials.refund().operation().is_none());
        }
        snapshot.ledger.apply(&manifest.ledger_batch).unwrap();
        assert!(
            !manifest
                .outbox_intents
                .iter()
                .any(|e| matches!(e, EffectIntent::DeliverMessage { .. }))
        );
    }
}
#[test]
fn refused_followup_preserves_request_awaiting_decision_and_its_finances() {
    use cs_mail_protocol::admission::AdmissionFailure;
    let mut f = Fixture::new();
    f.submit_initial_request();
    f.execute(
        ActorRef::Recipient(ProtocolIdentity(20)),
        ProtocolCommand::SetFollowupPolicy {
            expected_version: Version(0),
            policy: FollowupPolicy {
                version: Version(0),
                max_messages: 2,
                max_per_interval: 2,
                interval: Duration(10),
            },
        },
        4,
    )
    .unwrap();
    let before = f.engine.snapshot().unwrap();
    let command = KernelCommand::new(
        ProtocolCommand::AdmitFollowup {
            request_id: RequestId(1),
            message_id: MessageId(2),
            content_ref: ContentRef(2),
            delivery_intent_ref: DeliveryIntentRef(2),
            declaration_digest: MessageDeclarationDigest([0; 32]),
            message_valid_until: MessageValidityUntil(CanonicalTime(100)),
            expected_policy_version: Version(1),
        },
        ActorRef::Sender(ProtocolIdentity(10)),
        OperationalKeyRef(1),
        IdempotencyKey(99),
    );
    let context = TransitionContext {
        now: CanonicalTime(5),
        journal_position: JournalPosition(99),
        protocol_version: ProtocolVersion(2),
        policy: f.policy,
        admission: Err(AdmissionFailure::UnsupportedCriticalExtension),
    };
    assert_eq!(
        transition(&before, &command, &context),
        Err(ProtocolError::AdmissionRefused(
            AdmissionFailure::UnsupportedCriticalExtension
        ))
    );
    assert_eq!(f.engine.snapshot().unwrap(), before);
}
#[test]
fn correctly_signed_financial_command_cannot_replay_in_a_different_program() {
    let base = policy().financial.scope;
    let key = SimulatedProcessor::new([7; 32]).verifying_key();
    let command = cs_mail_finance::SignedProgramCommand::sign(
        base,
        SettlementUnit(1),
        IdempotencyKey(1),
        0,
        cs_mail_finance::ProgramCommand::Enroll {
            member: MemberId(1),
            identity_digest: [4; 32],
            status: cs_mail_finance::MembershipStatus {
                opted_in: true,
                verified: true,
                suspended: false,
            },
        },
        &[7; 32],
    )
    .unwrap();
    command.verify(&key).unwrap();
    let state = ProtocolState::initial(
        PrincipalRef(1),
        ProtocolIdentity(10),
        ProtocolIdentity(20),
        CanonicalTime(0),
    );
    let original = InMemoryEngine::new(state.clone(), SettlementUnit(1), base).unwrap();
    original
        .execute_financial_command(&command, &key, CanonicalTime(1))
        .unwrap();
    for scope in [
        cs_mail_finance::FinancialScope {
            deployment_domain: [8; 32],
            ..base
        },
        cs_mail_finance::FinancialScope {
            program: ProgramRef(2),
            ..base
        },
        cs_mail_finance::FinancialScope {
            payment_account: [8; 32],
            ..base
        },
    ] {
        let other = InMemoryEngine::new(state.clone(), SettlementUnit(1), scope).unwrap();
        assert_eq!(
            other.execute_financial_command(&command, &key, CanonicalTime(1)),
            Err(EngineError::VersionConflict)
        );
    }
}
