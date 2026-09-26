//! Consequential experiments are described in `HEADLESS_CONVERSATIONS.md`.
use cs_mail_application::correspondence::{
    Command, CorrespondenceService, MessageView, PreparedMessage, Response, SignedRequest,
    SourceState,
};
use cs_mail_client::NativeClient;
use cs_mail_content::{ContentBinding, ContentCertificateDigest};
use cs_mail_correspondence::*;
use cs_mail_finance::{BankVerification, FinancialScope};
use cs_mail_primitives::*;
use cs_mail_protocol::{ActorRef, ProtocolState, RelationshipState};
use cs_mail_storage_postgres::{PostgresAccountRepository, PostgresDeployment, StorageError};
use ed25519_dalek::SigningKey;
use postgres::{Client, NoTls};
use std::sync::{
    Arc, Barrier,
    atomic::{AtomicU64, Ordering},
};

const NOW: CanonicalTime = CanonicalTime(1_000);
static NEXT_SCHEMA: AtomicU64 = AtomicU64::new(1);
fn key(n: u8) -> [u8; 32] {
    SigningKey::from_bytes(&[n; 32]).verifying_key().to_bytes()
}
fn persona(i: usize) -> ProtocolIdentity {
    ProtocolIdentity(u128::try_from(i + 1).unwrap())
}
fn relation(a: usize, b: usize) -> RelationshipRef {
    RelationshipRef::from_u128_for_test(persona(a).0 * 10 + persona(b).0)
}
fn conversation(n: u128) -> ConversationId {
    ConversationId::new(n).unwrap()
}
struct Fixture {
    url: String,
    repository: PostgresAccountRepository,
    accounts: Vec<AccountId>,
    clients: Vec<NativeClient>,
    operation: AtomicU64,
    capability: std::cell::Cell<Option<LaneId>>,
}
impl Fixture {
    fn new() -> Self {
        let base =
            std::env::var("CS_MAIL_TEST_DATABASE_URL").expect("requires isolated PostgreSQL");
        let schema = format!(
            "correspondence_{}_{}_{}",
            std::process::id(),
            NEXT_SCHEMA.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        Client::connect(&base, NoTls)
            .unwrap()
            .batch_execute(&format!("CREATE SCHEMA {schema}"))
            .unwrap();
        let url = format!("{base}?options=-csearch_path%3D{schema}");
        let mut engines = vec![];
        for a in 0..3 {
            for b in 0..3 {
                if a != b {
                    let mut state = ProtocolState::initial_scoped(
                        relation(a, b),
                        RequestHistoryRef::from_u128_for_test(persona(a).0 * 10 + persona(b).0),
                        persona(a),
                        persona(b),
                        NOW,
                    );
                    state.relationship.state = RelationshipState::Accepted;
                    engines.push(
                        PostgresDeployment::connect(&url)
                            .unwrap()
                            .relationship(format!("pair-{a}-{b}"), &state, SettlementUnit(1))
                            .unwrap(),
                    );
                }
            }
        }
        let host = &engines[0];
        let scope = FinancialScope::new(
            [7; 32],
            ProviderRef(30),
            ProgramRef(1),
            [9; 32],
            ProtocolVersion(2),
        );
        host.deployment()
            .configure_payment_arrangement(scope, SettlementUnit(1), key(7), key(77))
            .unwrap();
        host.deployment()
            .configure_financial_program(scope, SettlementUnit(1), key(42), key(7))
            .unwrap();
        let mut accounts = vec![];
        let mut clients = vec![];
        for i in 0..3 {
            let n = u8::try_from(i + 1).unwrap();
            let id = u128::from(n);
            accounts.push(
                cs_mail_test_support::enrollment::enroll(
                    host,
                    cs_mail_accounts::EnrollmentInput {
                        bank: BankVerification {
                            scope,
                            account: BillingAccountId(id),
                            member: MemberId(id),
                            person: [n; 32],
                            bank_token: [n + 8; 32],
                            unit: SettlementUnit(1),
                            version: 1,
                            signature: vec![],
                        }
                        .sign(&[77; 32])
                        .unwrap(),
                        persona: persona(i),
                        actor: ActorRef::Sender(persona(i)),
                        key_ref: OperationalKeyRef(id),
                        initial_key: key(n),
                        maximum_unresolved: 10,
                    },
                    NOW,
                )
                .unwrap(),
            );
            clients.push(NativeClient::new(
                ActorRef::Sender(persona(i)),
                OperationalKeyRef(id),
                &[n; 32],
                ContentKeyRef(id),
            ));
        }
        let repository = Self::configured_repository(&url);
        Self {
            url,
            repository,
            accounts,
            clients,
            capability: std::cell::Cell::new(None),
            operation: AtomicU64::new(1),
        }
    }
    fn configured_repository(url: &str) -> PostgresAccountRepository {
        let repository = PostgresAccountRepository::connect(url, || NOW).unwrap();
        repository
            .configure_custody(
                cs_mail_key_custody::LocalCustodian::from_secret_bytes(
                    ContentKeyRef(900),
                    &[90; 32],
                )
                .unwrap()
                .public_key(),
            )
            .unwrap();
        repository
    }
    fn request(&self, actor: usize, command: Command) -> SignedRequest {
        SignedRequest {
            product: "cs-mail/test".into(),
            account: self.accounts[actor],
            persona: persona(actor),
            key: OperationalKeyRef(persona(actor).0),
            operation: IdempotencyKey(u128::from(self.operation.fetch_add(1, Ordering::Relaxed))),
            command,
            signature: vec![],
        }
        .sign(&[u8::try_from(actor + 1).unwrap(); 32])
        .unwrap()
    }
    fn call(&self, actor: usize, command: Command) -> Response {
        CorrespondenceService::new(&self.repository)
            .handle(&self.request(actor, command))
            .unwrap()
    }
    fn create(&self, id: u128, a: usize, b: usize) {
        self.call(
            a,
            Command::Create {
                id: conversation(id),
                other: persona(b),
            },
        );
    }
    fn prepare(
        &self,
        actor: usize,
        recipient: usize,
        id: u128,
        conv: u128,
        parts: Vec<DocumentPart>,
    ) -> PreparedMessage {
        let document = NativeClient::compose_correspondence(parts).unwrap();
        self.clients[actor]
            .prepare_correspondence(
                &document,
                conversation(conv),
                self.clients[recipient].content_public_key(),
                self.repository.custody_public_key().unwrap(),
                ContentBinding {
                    wire_version: WireVersion(1),
                    content_ref: ContentRef(id),
                    message_id: MessageId(id),
                    sender: persona(actor),
                    recipient: persona(recipient),
                    protocol_version: ProtocolVersion(2),
                    relationship: relation(actor, recipient),
                    content_scope: ContentScopeRef::from_u128_for_test(id),
                    sender_certificate: ContentCertificateDigest([7; 32]),
                    declarations: MessageDeclarations {
                        purpose: DeclaredPurpose::Known(KnownPurpose::Personal),
                        origin: OriginDeclaration {
                            mode: OriginMode::HumanInitiated,
                            authority: DeclarationAuthority::NativeSender(OperationalKeyRef(
                                persona(actor).0,
                            )),
                        },
                        payload_schema: None,
                    },
                    message_valid_until: MessageValidityUntil(CanonicalTime(100_000)),
                    capability: self.capability.get(),
                },
                (NOW, CanonicalTime(100_000)),
            )
            .unwrap()
    }
    fn view(&self, actor: usize, message: u128) -> MessageView {
        let Response::Message(view) = self.call(
            actor,
            Command::Fetch {
                message: MessageId(message),
            },
        ) else {
            panic!("expected retained copy")
        };
        *view
    }
    fn source(&self) -> (Document, MessageManifest, MessageSelection) {
        self.create(1, 0, 1);
        let message = self.prepare(
            0,
            1,
            100,
            1,
            vec![DocumentPart::Text("private proposal".into())],
        );
        let manifest = message.manifest.clone();
        self.call(0, Command::Send(Box::new(message)));
        let document = self.clients[1]
            .open_correspondence(&self.view(1, 100), self.clients[0].content_public_key())
            .unwrap();
        let anchor = MessageSelection::new(
            MessageId(100),
            manifest.version(),
            vec![SelectionElement::TextRange {
                block: 0,
                start: 0,
                end: 16,
            }],
        )
        .unwrap();
        (document, manifest, anchor)
    }
}

// Claim: references grant no access, while independently delivered excerpts survive
// source deletion. A leaked source or disappearing quotation falsifies the contract.
// Keep each causal sequence together so the counterexample remains reviewable.
#[allow(clippy::too_many_lines)]
#[test]
#[ignore = "requires isolated PostgreSQL"]
fn two_clients_retain_quotes_but_references_do_not_grant_or_restore_access() {
    let h = Fixture::new();
    let (source, manifest, anchor) = h.source();
    // Public versions must not fingerprint independently composed identical text;
    // otherwise the service could recover short messages by guessing plaintexts.
    let same_text = h.prepare(
        0,
        1,
        100,
        1,
        vec![DocumentPart::Text("private proposal".into())],
    );
    assert_ne!(same_text.manifest.version(), manifest.version());
    let serialized_manifest = serde_json::to_string(&manifest).unwrap();
    assert!(!serialized_manifest.contains("version_secret"));
    h.create(2, 0, 1);
    h.create(3, 0, 2);
    let quote = source.quote(&manifest, anchor.clone()).unwrap();
    let message = h.prepare(
        0,
        1,
        101,
        2,
        vec![
            DocumentPart::Reference(SourceReference(anchor.clone())),
            DocumentPart::Quotation(quote.clone()),
        ],
    );
    let signed = h.request(0, Command::Send(Box::new(message)));
    assert_eq!(
        CorrespondenceService::new(&h.repository)
            .handle(&signed)
            .unwrap(),
        Response::Committed(MessageId(101))
    );
    // Restart/retry cannot make a second message, nor retain ciphertext in the receipt.
    let restarted = PostgresAccountRepository::connect(&h.url, || NOW).unwrap();
    assert_eq!(
        CorrespondenceService::new(&restarted)
            .handle(&signed)
            .unwrap(),
        Response::Committed(MessageId(101))
    );
    let shared = h.prepare(
        0,
        2,
        102,
        3,
        vec![
            DocumentPart::Reference(SourceReference(anchor.clone())),
            DocumentPart::Share(SharedCopy(quote)),
        ],
    );
    let Response::Assessment(preview) = h.call(
        0,
        Command::Assess {
            manifest: shared.manifest.clone(),
        },
    ) else {
        panic!()
    };
    assert_eq!(preview.reuse, Ok(()));
    assert_eq!(
        preview.recipient_sources,
        vec![SourceState::AccessDenied, SourceState::AccessDenied]
    );
    h.call(0, Command::Send(Box::new(shared)));
    assert_eq!(
        h.call(
            2,
            Command::Resolve {
                source: anchor.clone()
            }
        ),
        Response::AccessDenied
    );
    h.call(
        0,
        Command::DeleteCopies {
            id: conversation(1),
        },
    );
    assert_eq!(
        h.call(
            0,
            Command::Resolve {
                source: anchor.clone()
            }
        ),
        Response::Unavailable
    );
    assert!(matches!(
        h.call(
            1,
            Command::Resolve {
                source: anchor.clone()
            }
        ),
        Response::Message(_)
    ));
    h.call(
        1,
        Command::DeleteCopies {
            id: conversation(1),
        },
    );
    assert_eq!(
        h.call(1, Command::Resolve { source: anchor }),
        Response::Unavailable
    );
    let retained = h.clients[1]
        .open_correspondence(&h.view(1, 101), h.clients[0].content_public_key())
        .unwrap();
    assert!(
        matches!(&retained.parts()[1], DocumentPart::Quotation(q) if q.blocks() == [ContentBlock::Text("private proposal".into())])
    );
    let copy = h.clients[2]
        .open_correspondence(&h.view(2, 102), h.clients[0].content_public_key())
        .unwrap();
    assert!(
        matches!(&copy.parts()[1], DocumentPart::Share(s) if s.0.blocks() == [ContentBlock::Text("private proposal".into())])
    );
    let mut db = Client::connect(&h.url, NoTls).unwrap();
    assert_eq!(
        db.query_one(
            "SELECT count(*) FROM cs_correspondence_copies WHERE message='101'",
            &[]
        )
        .unwrap()
        .get::<_, i64>(0),
        2
    );
    assert!(
        !db.query_one(
            "SELECT string_agg(receipt::text,'') FROM cs_correspondence_receipts",
            &[]
        )
        .unwrap()
        .get::<_, String>(0)
        .contains("ciphertext")
    );
    let mut tampered = h.view(1, 101);
    tampered.record.manifest = manifest;
    assert!(
        h.clients[1]
            .open_correspondence(&tampered, h.clients[0].content_public_key())
            .is_err()
    );
}

// Claim: the current policy, including ancestor policies, owns new reuse;
// stale previews, self-approved relaxation or copied intermediates cannot bypass it.
// Keep each causal sequence together so the counterexample remains reviewable.
#[allow(clippy::too_many_lines)]
#[test]
#[ignore = "requires isolated PostgreSQL"]
fn current_policy_survives_derivation_deletion_and_conflicting_approvals() {
    let h = Fixture::new();
    let (source, manifest, anchor) = h.source();
    h.create(2, 0, 1);
    h.create(3, 1, 2);
    let quote = source.quote(&manifest, anchor).unwrap();
    let intermediate = h.prepare(0, 1, 101, 2, vec![DocumentPart::Quotation(quote)]);
    let intermediate_manifest = intermediate.manifest.clone();
    h.call(0, Command::Send(Box::new(intermediate)));
    let document = h.clients[1]
        .open_correspondence(&h.view(1, 101), h.clients[0].content_public_key())
        .unwrap();
    let selection = MessageSelection::new(
        MessageId(101),
        intermediate_manifest.version(),
        vec![SelectionElement::TextRange {
            block: 0,
            start: 0,
            end: 16,
        }],
    )
    .unwrap();
    let onward = h.prepare(
        1,
        2,
        102,
        3,
        vec![DocumentPart::Quotation(
            document.quote(&intermediate_manifest, selection).unwrap(),
        )],
    );
    let Response::Assessment(preview) = h.call(
        1,
        Command::Assess {
            manifest: onward.manifest.clone(),
        },
    ) else {
        panic!()
    };
    assert_eq!(preview.reuse, Ok(()));
    h.call(
        0,
        Command::ChangePolicy {
            id: conversation(1),
            revision: 0,
            action: PolicyAction::Restrict,
        },
    );
    h.call(
        0,
        Command::DeleteCopies {
            id: conversation(1),
        },
    );
    h.call(
        1,
        Command::DeleteCopies {
            id: conversation(1),
        },
    );
    let outgoing = h.request(1, Command::Send(Box::new(onward)));
    assert!(matches!(
        CorrespondenceService::new(&h.repository).handle(&outgoing),
        Err(StorageError::Correspondence(
            CorrespondenceError::PolicyRestricted
        ))
    ));
    h.call(
        0,
        Command::ChangePolicy {
            id: conversation(1),
            revision: 1,
            action: PolicyAction::ProposeRelaxation,
        },
    );
    let self_approve = h.request(
        0,
        Command::ChangePolicy {
            id: conversation(1),
            revision: 2,
            action: PolicyAction::ApproveRelaxation,
        },
    );
    assert!(
        CorrespondenceService::new(&h.repository)
            .handle(&self_approve)
            .is_err()
    );
    let stale = h.request(
        1,
        Command::ChangePolicy {
            id: conversation(1),
            revision: 2,
            action: PolicyAction::ApproveRelaxation,
        },
    );
    h.call(
        0,
        Command::ChangePolicy {
            id: conversation(1),
            revision: 2,
            action: PolicyAction::Restrict,
        },
    );
    assert!(
        CorrespondenceService::new(&h.repository)
            .handle(&stale)
            .is_err()
    );
    h.call(
        0,
        Command::ChangePolicy {
            id: conversation(1),
            revision: 3,
            action: PolicyAction::ProposeRelaxation,
        },
    );
    h.call(
        1,
        Command::ChangePolicy {
            id: conversation(1),
            revision: 4,
            action: PolicyAction::ApproveRelaxation,
        },
    );
    assert_eq!(
        CorrespondenceService::new(&h.repository)
            .handle(&outgoing)
            .unwrap(),
        Response::Committed(MessageId(102))
    );
    // Missing policy data is uncertainty, not an implicit grant (model a damaged restore).
    let mut db = Client::connect(&h.url, NoTls).unwrap();
    db.batch_execute("ALTER TABLE cs_correspondence_messages DROP CONSTRAINT cs_correspondence_messages_conversation_fkey; DELETE FROM cs_conversations WHERE id='1'").unwrap();
    let assessment = h.request(
        1,
        Command::Assess {
            manifest: match outgoing.command {
                Command::Send(p) => p.manifest,
                _ => unreachable!(),
            },
        },
    );
    assert!(matches!(
        CorrespondenceService::new(&h.repository).handle(&assessment),
        Err(StorageError::Correspondence(
            CorrespondenceError::PolicyUnresolved
        ))
    ));
}

// Claim: concurrent policy/send and duplicate-send operations serialize; a failure
// after ciphertext insertion cannot leave a message without its durable receipt.
// Keep each causal sequence together so the counterexample remains reviewable.
#[allow(clippy::too_many_lines)]
#[test]
#[ignore = "requires isolated PostgreSQL"]
fn concurrent_commits_and_storage_failures_preserve_one_complete_delivery() {
    let h = Fixture::new();
    let (source, manifest, anchor) = h.source();
    h.create(2, 0, 2);
    let package = h.prepare(
        0,
        2,
        101,
        2,
        vec![DocumentPart::Quotation(
            source.quote(&manifest, anchor).unwrap(),
        )],
    );
    let send = h.request(0, Command::Send(Box::new(package)));
    let restriction = h.request(
        1,
        Command::ChangePolicy {
            id: conversation(1),
            revision: 0,
            action: PolicyAction::Restrict,
        },
    );
    let barrier = Arc::new(Barrier::new(3));
    let handles: [_; 2] = [send.clone(), restriction].map(|request| {
        let url = h.url.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            let repository = PostgresAccountRepository::connect(&url, || NOW).unwrap();
            repository
                .configure_custody(
                    cs_mail_key_custody::LocalCustodian::from_secret_bytes(
                        ContentKeyRef(900),
                        &[90; 32],
                    )
                    .unwrap()
                    .public_key(),
                )
                .unwrap();
            barrier.wait();
            CorrespondenceService::new(&repository).handle(&request)
        })
    });
    barrier.wait();
    let [sending, restricting] = handles;
    let outcome = sending.join().unwrap();
    restricting.join().unwrap().unwrap();
    let mut db = Client::connect(&h.url, NoTls).unwrap();
    match outcome {
        Ok(Response::Committed(_)) => {
            let receipt:postgres::types::Json<cs_mail_application::correspondence::Receipt>=db.query_one("SELECT receipt FROM cs_correspondence_receipts WHERE account=$1 AND operation=$2",&[&send.account.0.to_string(),&send.operation.0.to_string()]).unwrap().get(0);
            assert_eq!(receipt.0.policy_revisions[&conversation(1)], 0);
        }
        Err(StorageError::Correspondence(CorrespondenceError::PolicyRestricted)) => {
            assert_eq!(
                db.query_one(
                    "SELECT count(*) FROM cs_correspondence_messages WHERE id='101'",
                    &[]
                )
                .unwrap()
                .get::<_, i64>(0),
                0
            );
        }
        other => panic!("unexpected race result {other:?}"),
    }
    let plain = h.prepare(
        0,
        2,
        102,
        2,
        vec![DocumentPart::Text("atomic delivery".into())],
    );
    let request = h.request(0, Command::Send(Box::new(plain)));
    db.batch_execute("CREATE FUNCTION reject_correspondence_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected failure'; END $$; CREATE TRIGGER fail_receipt BEFORE INSERT ON cs_correspondence_receipts FOR EACH ROW EXECUTE FUNCTION reject_correspondence_receipt()").unwrap();
    assert!(
        CorrespondenceService::new(&h.repository)
            .handle(&request)
            .is_err()
    );
    assert_eq!(
        db.query_one(
            "SELECT count(*) FROM cs_correspondence_messages WHERE id='102'",
            &[]
        )
        .unwrap()
        .get::<_, i64>(0),
        0
    );
    db.batch_execute("DROP TRIGGER fail_receipt ON cs_correspondence_receipts")
        .unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let url = h.url.clone();
            let request = request.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let r = PostgresAccountRepository::connect(&url, || NOW).unwrap();
                barrier.wait();
                CorrespondenceService::new(&r).handle(&request)
            })
        })
        .collect();
    barrier.wait();
    for handle in handles {
        assert_eq!(
            handle.join().unwrap().unwrap(),
            Response::Committed(MessageId(102))
        );
    }
    assert_eq!(
        db.query_one(
            "SELECT count(*) FROM cs_correspondence_copies WHERE message='102'",
            &[]
        )
        .unwrap()
        .get::<_, i64>(0),
        2
    );
    let mut altered = request;
    altered.command = Command::DeleteCopies {
        id: conversation(2),
    };
    altered = altered.sign(&[1; 32]).unwrap();
    assert!(matches!(
        CorrespondenceService::new(&h.repository).handle(&altered),
        Err(StorageError::Correspondence(CorrespondenceError::Conflict))
    ));
}

// Claim: presentation scope is not contact or key authority; a mailbox listing
// cannot reveal another participant's data or make expired content available.
// A send must not overtake an earlier queued change to contact authority.
#[test]
#[ignore = "requires isolated PostgreSQL"]
#[allow(clippy::too_many_lines)] // The scenario challenges successive changes in live authority.
fn mailbox_and_live_authority_do_not_inherit_permission_from_correspondence_scope() {
    let h = Fixture::new();
    h.source();
    let Response::Mailbox {
        entries,
        next_after,
    } = h.call(1, Command::Mailbox { after: 0, limit: 1 })
    else {
        panic!()
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].1.manifest.message(), MessageId(100));
    assert!(
        matches!(h.call(2, Command::Mailbox { after: 0, limit: 100 }), Response::Mailbox { entries, .. } if entries.is_empty())
    );
    assert!(
        matches!(h.call(1, Command::Mailbox { after: next_after, limit: 1 }), Response::Mailbox { entries, .. } if entries.is_empty())
    );
    let foreign = h.request(
        2,
        Command::Policy {
            id: conversation(1),
        },
    );
    assert!(matches!(
        CorrespondenceService::new(&h.repository).handle(&foreign),
        Err(StorageError::Correspondence(
            CorrespondenceError::Unauthorized
        ))
    ));
    let mut forged = h.request(
        1,
        Command::Fetch {
            message: MessageId(100),
        },
    );
    forged.signature[0] ^= 1;
    assert!(matches!(
        CorrespondenceService::new(&h.repository).handle(&forged),
        Err(StorageError::Correspondence(
            CorrespondenceError::Unauthorized
        ))
    ));
    let forward = h.prepare(0, 1, 103, 1, vec![DocumentPart::Text("follow-up".into())]);
    let forward_request = h.request(0, Command::Send(Box::new(forward)));
    let mut db = Client::connect(&h.url, NoTls).unwrap();
    // A pending receipt stands in for an authority change awaiting processing.
    // No command is evaluated here; only the ordering barrier is exercised.
    db.execute(
        "INSERT INTO cs_received_commands
         (aggregate_key,idempotency_key,replay_fingerprint,content_available,digest,deployment,received_at)
         VALUES ('pair-0-1','pending-authority-change',$1,TRUE,$1,$1,1000)",
        &[&vec![0_u8; 32]],
    ).unwrap();
    assert!(matches!(
        CorrespondenceService::new(&h.repository).handle(&forward_request),
        Err(StorageError::PendingCommands)
    ));
    db.execute(
        "DELETE FROM cs_received_commands WHERE aggregate_key='pair-0-1' AND idempotency_key='pending-authority-change'",
        &[],
    ).unwrap();
    assert_eq!(
        CorrespondenceService::new(&h.repository)
            .handle(&forward_request)
            .unwrap(),
        Response::Committed(MessageId(103))
    );
    // Change the fixture's reverse-direction state; the forward grant and the
    // conversation still exist, but neither grants permission to send backwards.
    Client::connect(&h.url, NoTls).unwrap().batch_execute(
        "UPDATE cs_relationship_aggregates SET relationship_state=jsonb_set(relationship_state,'{relationship,state}','\"Revoked\"') WHERE aggregate_key='pair-1-0'"
    ).unwrap();
    let reply = h.prepare(1, 0, 101, 1, vec![DocumentPart::Text("reply".into())]);
    let request = h.request(1, Command::Send(Box::new(reply)));
    assert!(matches!(
        CorrespondenceService::new(&h.repository).handle(&request),
        Err(StorageError::Correspondence(
            CorrespondenceError::Unauthorized
        ))
    ));
    exercise_native_lane(&h);
    let late = PostgresAccountRepository::connect(&h.url, || CanonicalTime(100_000)).unwrap();
    assert_eq!(
        CorrespondenceService::new(&late)
            .handle(&h.request(
                1,
                Command::Fetch {
                    message: MessageId(100)
                }
            ))
            .unwrap(),
        Response::Unavailable
    );
    assert!(
        matches!(CorrespondenceService::new(&late).handle(&h.request(1,Command::Mailbox { after:0, limit:100 })).unwrap(), Response::Mailbox { entries, .. } if entries.is_empty())
    );
    h.repository
        .revoke_account_key(h.accounts[0], OperationalKeyRef(1), Version(0), NOW)
        .unwrap();
    assert!(matches!(
        CorrespondenceService::new(&h.repository).handle(&h.request(
            0,
            Command::Policy {
                id: conversation(1)
            }
        )),
        Err(StorageError::Correspondence(
            CorrespondenceError::Unauthorized
        ))
    ));
}

// Claim: a relationship-level lane permits native delivery across conversations,
// consumes allowance only with commit, and does not turn replay into a second send.
fn exercise_native_lane(h: &Fixture) {
    use cs_mail_capabilities::*;
    use ed25519_dalek::Signer;
    let grant = LaneGrant {
        id: LaneId(800),
        subject: LaneSubject::Native(cs_mail_privacy::ScopedHandle([8; 32])),
        sender: persona(1),
        recipient: persona(0),
        protocol_version: ProtocolVersion(2),
        deployment_domain: [7; 32],
        intended_provider: ProviderRef(30),
        recipient_operational_key: OperationalKeyRef(1),
        purpose: DeclaredPurpose::Known(KnownPurpose::Personal),
        origin: Some(OriginMode::HumanInitiated),
        declaration_authority: DeclarationAuthorityConstraint::NativeSender,
        lifetime: Duration(1000),
        rate_limit: RateLimit {
            max_messages: 1,
            interval: Duration(100),
        },
        mode: LaneMode::Expiring,
        issued_at: NOW,
        not_before: NOW,
        not_after: CanonicalTime(10_000),
        version: Version(1),
    };
    let signed = SignedLaneGrant {
        signature: SigningKey::from_bytes(&[1; 32])
            .sign(&grant.signing_bytes().unwrap())
            .to_bytes(),
        grant,
    };
    let lane = Lane::from_verified_grant(&signed, &key(1)).unwrap();
    let mut db = Client::connect(&h.url, NoTls).unwrap();
    db.execute("INSERT INTO cs_capability_lanes(aggregate_key,lane_id,lane,updated_at) VALUES('pair-1-0','800',$1,1000)", &[&postgres::types::Json(&lane)]).unwrap();
    db.execute("UPDATE cs_relationship_aggregates SET relationship_state=jsonb_set(relationship_state,'{relationship,state}',$1) WHERE aggregate_key='pair-1-0'", &[&postgres::types::Json(RelationshipState::ExpressLane(LaneId(800)))]).unwrap();
    h.create(50, 1, 0);
    let no_capability = h.prepare(
        1,
        0,
        801,
        50,
        vec![DocumentPart::Text("missing capability".into())],
    );
    assert!(
        CorrespondenceService::new(&h.repository)
            .handle(&h.request(1, Command::Send(Box::new(no_capability))))
            .is_err()
    );
    h.capability.set(Some(LaneId(800)));
    let package = h.prepare(1, 0, 802, 50, vec![DocumentPart::Text("permitted".into())]);
    let request = h.request(1, Command::Send(Box::new(package)));
    let service = CorrespondenceService::new(&h.repository);
    assert_eq!(
        service.handle(&request).unwrap(),
        Response::Committed(MessageId(802))
    );
    assert_eq!(
        service.handle(&request).unwrap(),
        Response::Committed(MessageId(802))
    );
    let used = db
        .query_one(
            "SELECT lane FROM cs_capability_lanes WHERE lane_id='800'",
            &[],
        )
        .unwrap()
        .get::<_, postgres::types::Json<Lane>>(0)
        .0;
    assert_eq!(used.messages_in_rate_window, 1);
    assert!(matches!(
        h.view(0, 802).record.admission,
        cs_mail_protocol::AdmissionBasis::ExpressLane {
            lane: LaneId(800),
            ..
        }
    ));
    let excess = h.prepare(
        1,
        0,
        803,
        1,
        vec![DocumentPart::Text("over allowance".into())],
    );
    assert!(
        service
            .handle(&h.request(1, Command::Send(Box::new(excess))))
            .is_err()
    );
    h.capability.set(None);
}

// Claim: mixed quotations/share copies retain selected image bytes after source
// deletion; references still require the source and current ancestry still governs
// onward reuse. Losing an image, leaking source access or allowing a restricted
// image to escape through a quotation would falsify this contract.
#[test]
#[ignore = "requires isolated PostgreSQL"]
#[allow(clippy::too_many_lines)] // Keep the delivery/deletion/reuse causal sequence together.
fn mixed_selections_survive_copy_delivery_without_bypassing_source_authority() {
    let h = Fixture::new();
    h.create(1, 0, 1);
    h.create(2, 0, 1);
    h.create(3, 1, 2);
    let image = ImageContent::new(
        ImageFormat::Png,
        include_bytes!("../../cs-mail-correspondence/tests/fixtures/pixel.png").to_vec(),
    )
    .unwrap();
    let source = h.prepare(
        0,
        1,
        200,
        1,
        vec![
            DocumentPart::Text("before café".into()),
            DocumentPart::Image(image.clone()),
            DocumentPart::Text("after".into()),
        ],
    );
    let manifest = source.manifest.clone();
    h.call(0, Command::Send(Box::new(source)));
    let selection = MessageSelection::new(
        MessageId(200),
        manifest.version(),
        vec![
            SelectionElement::TextRange {
                block: 0,
                start: 7,
                end: 12,
            },
            SelectionElement::ImageBlock { block: 1 },
            SelectionElement::TextRange {
                block: 2,
                start: 0,
                end: 5,
            },
        ],
    )
    .unwrap();
    let Response::Message(view) = h.call(
        1,
        Command::Resolve {
            source: selection.clone(),
        },
    ) else {
        panic!()
    };
    let document = h.clients[1]
        .open_correspondence(&view, h.clients[0].content_public_key())
        .unwrap();
    let expected = vec![
        ContentBlock::Text("café".into()),
        ContentBlock::Image(image),
        ContentBlock::Text("after".into()),
    ];
    assert_eq!(document.select(&manifest, &selection).unwrap(), expected);
    let quote = document.quote(&manifest, selection.clone()).unwrap();
    let copy = h.prepare(
        0,
        1,
        201,
        2,
        vec![
            DocumentPart::Reference(SourceReference(selection.clone())),
            DocumentPart::Quotation(quote.clone()),
            DocumentPart::Share(SharedCopy(quote)),
        ],
    );
    let copy_manifest = copy.manifest.clone();
    h.call(0, Command::Send(Box::new(copy)));
    h.call(
        0,
        Command::DeleteCopies {
            id: conversation(1),
        },
    );
    h.call(
        1,
        Command::DeleteCopies {
            id: conversation(1),
        },
    );
    assert_eq!(
        h.call(1, Command::Resolve { source: selection }),
        Response::Unavailable
    );
    let retained = h.clients[1]
        .open_correspondence(&h.view(1, 201), h.clients[0].content_public_key())
        .unwrap();
    assert!(matches!(&retained.parts()[1],DocumentPart::Quotation(q) if q.blocks()==expected));
    assert!(matches!(&retained.parts()[2],DocumentPart::Share(s) if s.0.blocks()==expected));
    // The reference occupies block 0; the retained quotation's image is block 2.
    let selected_image = MessageSelection::new(
        MessageId(201),
        copy_manifest.version(),
        vec![SelectionElement::ImageBlock { block: 2 }],
    )
    .unwrap();
    let onward = h.prepare(
        1,
        2,
        202,
        3,
        vec![DocumentPart::Quotation(
            retained.quote(&copy_manifest, selected_image).unwrap(),
        )],
    );
    let Response::Assessment(preview) = h.call(
        1,
        Command::Assess {
            manifest: onward.manifest.clone(),
        },
    ) else {
        panic!()
    };
    assert_eq!(preview.reuse, Ok(()));
    h.call(
        0,
        Command::ChangePolicy {
            id: conversation(1),
            revision: 0,
            action: PolicyAction::Restrict,
        },
    );
    assert!(matches!(
        CorrespondenceService::new(&h.repository)
            .handle(&h.request(1, Command::Send(Box::new(onward)))),
        Err(StorageError::Correspondence(
            CorrespondenceError::PolicyRestricted
        ))
    ));
}

#[path = "correspondence/consent.rs"]
mod consent;
