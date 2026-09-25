use super::*;
use cs_mail_application::consent::{
    AuthorizedDecryption, ConsentCommand, ConsentResponse, ConsentService, LocalKeyCustody,
    ProcessorRegistration, ReleasedCopy, SignedConsentRequest,
};
use cs_mail_consent::{
    ConsentError, ContentScope, DecryptionActor, DecryptionPurpose, GrantId, GrantUse,
};
use cs_mail_key_custody::LocalCustodian;

fn custodian() -> LocalCustodian {
    LocalCustodian::from_secret_bytes(ContentKeyRef(900), &[90; 32]).unwrap()
}
fn grant(n: u128) -> GrantId {
    GrantId::new(n).unwrap()
}
fn scope(messages: &[u128]) -> ContentScope {
    messages
        .iter()
        .map(|m| MessageId(*m))
        .collect::<std::collections::BTreeSet<_>>()
        .try_into()
        .unwrap()
}
fn request(
    h: &Fixture,
    owner: usize,
    actor: DecryptionActor,
    secret: u8,
    command: ConsentCommand,
) -> SignedConsentRequest {
    SignedConsentRequest {
        product: "cs-mail/test".into(),
        account: h.accounts[owner],
        persona: persona(owner),
        actor,
        operation: IdempotencyKey(u128::from(h.operation.fetch_add(1, Ordering::Relaxed)) + 10_000),
        command,
        signature: vec![],
    }
    .sign(&[secret; 32])
    .unwrap()
}
fn device(h: &Fixture, owner: usize, command: ConsentCommand) -> SignedConsentRequest {
    request(
        h,
        owner,
        DecryptionActor::UserDevice(OperationalKeyRef(persona(owner).0)),
        u8::try_from(owner + 1).unwrap(),
        command,
    )
}
fn display_usage(client: &NativeClient, signing: u128) -> GrantUse {
    GrantUse {
        actor: DecryptionActor::UserDevice(OperationalKeyRef(signing)),
        purpose: DecryptionPurpose::Display,
        destination: client.content_public_key(),
    }
}
fn authorize(h: &Fixture, id: u128, usage: GrantUse) -> SignedConsentRequest {
    device(
        h,
        1,
        ConsentCommand::Authorize {
            id: grant(id),
            scope: scope(&[100]),
            usage,
            expires_at: CanonicalTime(5000),
        },
    )
}
fn read(id: u128, message: u128, purpose: DecryptionPurpose) -> ConsentCommand {
    ConsentCommand::Read {
        id: grant(id),
        message: MessageId(message),
        purpose,
    }
}

// Claim: identity-authorized replacement keys plus new consent restore retained
// content without an old device secret. Authentication alone or old grants must
// not do so; the wrong device must not open the newly sealed response.
#[test]
#[ignore = "requires isolated PostgreSQL"]
#[allow(clippy::too_many_lines)] // Preserve the full loss/recovery/consent causal sequence.
fn replacement_device_recovers_only_after_fresh_scoped_consent() {
    let h = Fixture::new();
    h.source();
    let custody = custodian();
    let service = ConsentService::new(&h.repository, &custody);
    assert!(matches!(
        service.handle(&device(&h, 1, read(1, 100, DecryptionPurpose::Display))),
        Err(StorageError::Consent(ConsentError::Unauthorized))
    ));
    service
        .handle(&authorize(&h, 1, display_usage(&h.clients[1], 2)))
        .unwrap();
    let replacement = NativeClient::new(
        ActorRef::Sender(persona(1)),
        OperationalKeyRef(12),
        &[12; 32],
        ContentKeyRef(12),
    );
    assert!(
        replacement
            .open_correspondence(&h.view(1, 100), h.clients[0].content_public_key())
            .is_err()
    );
    // Exercise the production verification/application boundary with a test issuer.
    let event = identity_contract::changes::SignedSecurityEvent::sign(
        identity_contract::changes::SecurityEvent {
            id: "recover-device".into(),
            issuer: "https://identity.example.test".into(),
            product: "cs-mail/test".into(),
            account: h.accounts[1].0.to_string(),
            subject_ref: "fixture-subject-2".to_string().try_into().unwrap(),
            security_version: 2,
            occurred_at: 1,
            policy: "cs-mail.account-change.v1".into(),
            evidence_ref: "verified-recovery-ceremony".into(),
            change: identity_contract::changes::IdentityChange::RecoverDevice {
                key_reference: "12".into(),
                persona: "2".into(),
            },
            initial_key: key(12),
            bank_digest: [0; 32],
        },
        &[80; 32],
    )
    .unwrap();
    let accounts =
        cs_mail_application::accounts::operations::AccountService::new(&h.repository, &|| NOW);
    accounts.apply_identity_change(&event, None).unwrap();
    // Existing account recovery suspends service until the recovered manager resumes it.
    let resumed = cs_mail_accounts::control::SignedAccountCommand {
        account: h.accounts[1],
        operational_key: OperationalKeyRef(12),
        expected_revision: 1,
        idempotency_key: IdempotencyKey(50000),
        product: "cs-mail/test".into(),
        command: cs_mail_accounts::control::AccountCommand::SetStatus(
            cs_mail_accounts::control::AccountStatus::Active,
        ),
        signature: vec![],
    }
    .sign(&[12; 32])
    .unwrap();
    accounts.execute_command(&resumed).unwrap();
    assert!(matches!(
        service.handle(&device(&h, 1, read(1, 100, DecryptionPurpose::Display))),
        Err(StorageError::Consent(ConsentError::Unauthorized))
    ));
    let new_request = |command| {
        request(
            &h,
            1,
            DecryptionActor::UserDevice(OperationalKeyRef(12)),
            12,
            command,
        )
    };
    assert!(
        service
            .handle(&new_request(read(1, 100, DecryptionPurpose::Display)))
            .is_err()
    );
    service
        .handle(&new_request(ConsentCommand::Authorize {
            id: grant(2),
            scope: scope(&[100]),
            usage: display_usage(&replacement, 12),
            expires_at: CanonicalTime(5000),
        }))
        .unwrap();
    // Restart both persistence and local custody, restoring only the host's root.
    let restarted = PostgresAccountRepository::connect(&h.url, || NOW).unwrap();
    let restored = custodian();
    let reading = new_request(read(2, 100, DecryptionPurpose::Display));
    let ConsentResponse::Released(released) = ConsentService::new(&restarted, &restored)
        .handle(&reading)
        .unwrap()
    else {
        panic!()
    };
    let document = replacement
        .open_released(&released, restored.public_key())
        .unwrap();
    assert_eq!(
        document.parts(),
        [DocumentPart::Text("private proposal".into())]
    );
    assert!(
        h.clients[1]
            .open_released(&released, restored.public_key())
            .is_err()
    );
    assert_eq!(
        ConsentService::new(&restarted, &restored)
            .handle(&reading)
            .unwrap(),
        ConsentResponse::AlreadyReleased {
            grant: grant(2),
            message: MessageId(100)
        }
    );
    let mut altered = reading.clone();
    altered.command = read(2, 101, DecryptionPurpose::Display);
    altered = altered.sign(&[12; 32]).unwrap();
    assert!(matches!(
        ConsentService::new(&restarted, &restored).handle(&altered),
        Err(StorageError::Consent(ConsentError::Conflict))
    ));
    let mut db = Client::connect(&h.url, NoTls).unwrap();
    let audit: String = db
        .query_one(
            "SELECT string_agg(record::text,'') FROM cs_consent_receipts",
            &[],
        )
        .unwrap()
        .get(0);
    assert!(!audit.contains("ciphertext") && !audit.contains("private proposal"));
}

// Claim: a registered processor and a user's display grant never imply processing
// consent. A processing grant cannot extend to another message, purpose or owner.
#[test]
#[ignore = "requires isolated PostgreSQL"]
#[allow(clippy::too_many_lines)] // The same grant is challenged along independent authority dimensions.
fn processing_requires_its_own_exact_scope_and_active_user_consent() {
    let h = Fixture::new();
    h.source();
    let custody = custodian();
    let processor = NativeClient::new(
        ActorRef::Sender(persona(1)),
        OperationalKeyRef(70),
        &[70; 32],
        ContentKeyRef(700),
    );
    let purpose = DecryptionPurpose::CsqdProcessing {
        operation: "extract-user-requested-summary".into(),
    };
    let usage = GrantUse {
        actor: DecryptionActor::CsqdProcessor(OperationalKeyRef(70)),
        purpose: purpose.clone(),
        destination: processor.content_public_key(),
    };
    h.repository
        .configure_content_processor(&ProcessorRegistration {
            key: OperationalKeyRef(70),
            verifying_key: key(70),
            usage: usage.clone(),
            active: true,
        })
        .unwrap();
    let service = ConsentService::new(&h.repository, &custody);
    service
        .handle(&authorize(&h, 1, display_usage(&h.clients[1], 2)))
        .unwrap();
    let worker = |command| {
        request(
            &h,
            1,
            DecryptionActor::CsqdProcessor(OperationalKeyRef(70)),
            70,
            command,
        )
    };
    assert!(matches!(
        service.handle(&worker(read(1, 100, purpose.clone()))),
        Err(StorageError::Consent(ConsentError::ScopeDenied))
    ));
    service.handle(&authorize(&h, 2, usage.clone())).unwrap();
    assert!(matches!(
        service.handle(&worker(read(2, 101, purpose.clone()))),
        Err(StorageError::Consent(ConsentError::ScopeDenied))
    ));
    assert!(matches!(
        service.handle(&worker(read(
            2,
            100,
            DecryptionPurpose::CsqdProcessing {
                operation: "unrelated-training".into()
            }
        ))),
        Err(StorageError::Consent(ConsentError::ScopeDenied))
    ));
    let foreign = request(
        &h,
        0,
        DecryptionActor::CsqdProcessor(OperationalKeyRef(70)),
        70,
        read(2, 100, purpose.clone()),
    );
    assert!(matches!(
        service.handle(&foreign),
        Err(StorageError::Consent(ConsentError::Unauthorized))
    ));
    let ConsentResponse::Released(released) = service
        .handle(&worker(read(2, 100, purpose.clone())))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        processor
            .open_released(&released, custody.public_key())
            .unwrap()
            .parts(),
        [DocumentPart::Text("private proposal".into())]
    );
    assert!(
        h.clients[1]
            .open_released(&released, custody.public_key())
            .is_err()
    );
    service
        .handle(&device(&h, 1, ConsentCommand::Revoke { id: grant(2) }))
        .unwrap();
    assert!(matches!(
        service.handle(&worker(read(2, 100, purpose.clone()))),
        Err(StorageError::Consent(ConsentError::Revoked))
    ));
    let late = PostgresAccountRepository::connect(&h.url, || CanonicalTime(5000)).unwrap();
    assert!(matches!(
        ConsentService::new(&late, &custody).handle(&device(
            &h,
            1,
            read(1, 100, DecryptionPurpose::Display)
        )),
        Err(StorageError::Consent(ConsentError::Expired))
    ));
    // Content lifetime is independent of the grant lifetime.
    service
        .handle(&device(
            &h,
            1,
            ConsentCommand::Authorize {
                id: grant(4),
                scope: scope(&[100]),
                usage: display_usage(&h.clients[1], 2),
                expires_at: CanonicalTime(200_000),
            },
        ))
        .unwrap();
    let expired_content =
        PostgresAccountRepository::connect(&h.url, || CanonicalTime(100_000)).unwrap();
    assert!(matches!(
        ConsentService::new(&expired_content, &custody).handle(&device(
            &h,
            1,
            read(4, 100, DecryptionPurpose::Display)
        )),
        Err(StorageError::Consent(ConsentError::Unavailable))
    ));
    service.handle(&authorize(&h, 3, usage)).unwrap();
    h.repository
        .disable_content_processor(OperationalKeyRef(70))
        .unwrap();
    assert!(matches!(
        service.handle(&worker(read(3, 100, purpose))),
        Err(StorageError::Consent(ConsentError::Unauthorized))
    ));
    h.call(
        1,
        Command::DeleteCopies {
            id: conversation(1),
        },
    );
    assert!(matches!(
        service.handle(&device(&h, 1, read(1, 100, DecryptionPurpose::Display))),
        Err(StorageError::Consent(ConsentError::Unavailable))
    ));
    let mut db = Client::connect(&h.url, NoTls).unwrap();
    assert_eq!(
        db.query_one(
            "SELECT count(*) FROM cs_correspondence_recovery WHERE owner='2'",
            &[]
        )
        .unwrap()
        .get::<_, i64>(0),
        0
    );
    // The sender's independent copy remains recoverable after the recipient deletes theirs.
    assert_eq!(
        db.query_one(
            "SELECT count(*) FROM cs_correspondence_recovery WHERE owner='1'",
            &[]
        )
        .unwrap()
        .get::<_, i64>(0),
        1
    );
}

struct CountingCustodian {
    inner: LocalCustodian,
    calls: AtomicU64,
}
impl LocalKeyCustody for CountingCustodian {
    fn public_key(&self) -> cs_mail_content::EndpointPublicKey {
        self.inner.public_key()
    }
    fn release(&self, a: AuthorizedDecryption<'_>) -> Result<ReleasedCopy, ConsentError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.inner.release(a)
    }
}
// Claim: a receipt failure exposes no sealed response, and a committed retry never
// invokes key custody again. Otherwise retries could silently duplicate disclosure.
#[test]
#[ignore = "requires isolated PostgreSQL"]
fn consent_receipt_failure_rolls_back_and_replay_performs_no_new_key_use() {
    let h = Fixture::new();
    h.source();
    let custody = CountingCustodian {
        inner: custodian(),
        calls: AtomicU64::new(0),
    };
    let service = ConsentService::new(&h.repository, &custody);
    service
        .handle(&authorize(&h, 1, display_usage(&h.clients[1], 2)))
        .unwrap();
    let request = device(&h, 1, read(1, 100, DecryptionPurpose::Display));
    let mut db = Client::connect(&h.url, NoTls).unwrap();
    db.batch_execute("CREATE FUNCTION reject_consent_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected receipt failure'; END $$; CREATE TRIGGER reject_consent_receipt BEFORE INSERT ON cs_consent_receipts FOR EACH ROW EXECUTE FUNCTION reject_consent_receipt();").unwrap();
    assert!(matches!(
        service.handle(&request),
        Err(StorageError::Database(_))
    ));
    assert_eq!(
        db.query_one(
            "SELECT count(*) FROM cs_consent_receipts WHERE operation=$1",
            &[&request.operation.0.to_string()]
        )
        .unwrap()
        .get::<_, i64>(0),
        0
    );
    db.batch_execute("DROP TRIGGER reject_consent_receipt ON cs_consent_receipts")
        .unwrap();
    assert!(matches!(
        service.handle(&request).unwrap(),
        ConsentResponse::Released(_)
    ));
    let calls = custody.calls.load(Ordering::Relaxed);
    assert!(matches!(
        service.handle(&request).unwrap(),
        ConsentResponse::AlreadyReleased { .. }
    ));
    assert_eq!(custody.calls.load(Ordering::Relaxed), calls);
}

// Claim: revocation and disclosure have one protected ordering, with no stale
// authorization surviving a revocation that wins the commit boundary.
#[test]
#[ignore = "requires isolated PostgreSQL"]
fn racing_revocation_and_release_commit_in_one_order() {
    let h = Fixture::new();
    h.source();
    let custody = custodian();
    ConsentService::new(&h.repository, &custody)
        .handle(&authorize(&h, 1, display_usage(&h.clients[1], 2)))
        .unwrap();
    let read_request = device(&h, 1, read(1, 100, DecryptionPurpose::Display));
    let revoke_request = device(&h, 1, ConsentCommand::Revoke { id: grant(1) });
    let barrier = Arc::new(Barrier::new(3));
    let handles: Vec<_> = [read_request, revoke_request]
        .into_iter()
        .map(|request| {
            let barrier = barrier.clone();
            let url = h.url.clone();
            std::thread::spawn(move || {
                let r = PostgresAccountRepository::connect(&url, || NOW).unwrap();
                let k = custodian();
                barrier.wait();
                ConsentService::new(&r, &k).handle(&request)
            })
        })
        .collect();
    barrier.wait();
    let mut results = handles.into_iter();
    assert!(matches!(
        results.next().unwrap().join().unwrap(),
        Ok(ConsentResponse::Released(_)) | Err(StorageError::Consent(ConsentError::Revoked))
    ));
    assert!(results.next().unwrap().join().unwrap().is_ok());
    assert!(matches!(
        ConsentService::new(&h.repository, &custody).handle(&device(
            &h,
            1,
            read(1, 100, DecryptionPurpose::Display)
        )),
        Err(StorageError::Consent(ConsentError::Revoked))
    ));
}
