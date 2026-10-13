//! Agreed experiments: cross-address edits, partial saves, unlisting visibility and
//! replay after response loss. Counterexamples would violate ownership or publish
//! information/terms different from the member's durable decision.
use cs_mail_accounts::{
    EnrollmentInput,
    control::{AccountCommand, AccountStatus, SignedAccountCommand},
    directory::*,
};
use cs_mail_application::{accounts::operations::AccountService, directory::*};
use cs_mail_finance::{BankVerification, FinancialScope};
use cs_mail_primitives::*;
use cs_mail_protocol::{
    ActorRef,
    pricing::{RecipientRequestClasses, RequestClass},
};
use cs_mail_security::{CommandSigner, RecipientSigningScope};
use cs_mail_storage_postgres::PostgresDeployment;
use cs_mail_test_support as support;
use postgres::{Client, NoTls};
use std::sync::{
    Arc, Barrier,
    atomic::{AtomicU64, Ordering},
};
static NEXT_SCHEMA: AtomicU64 = AtomicU64::new(1);

const AT: CanonicalTime = CanonicalTime(44_582_400_000);
const UNIT: SettlementUnit = SettlementUnit(1);
fn scope() -> FinancialScope {
    FinancialScope::new(
        [7; 32],
        ProviderRef(30),
        ProgramRef(1),
        [9; 32],
        ProtocolVersion(2),
    )
}
fn signing_scope() -> RecipientSigningScope {
    RecipientSigningScope {
        deployment_domain: [7; 32],
        provider: ProviderRef(30),
    }
}
struct Fixture {
    url: String,
    deployment: PostgresDeployment,
    accounts: Vec<AccountId>,
}
impl Fixture {
    fn new() -> Self {
        let base =
            std::env::var("CS_MAIL_TEST_DATABASE_URL").expect("isolated PostgreSQL required");
        let schema = format!(
            "cs_directory_{}_{}_{}",
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
        let deployment = PostgresDeployment::connect(&url).unwrap();
        support::bootstrap(&deployment, scope(), UNIT).unwrap();
        support::operator(
            &deployment,
            cs_mail_operations::OperatorCommand::PublishRequestPricing(
                support::fixture_pricing(UNIT).unwrap(),
            ),
            AT,
        )
        .unwrap();
        let accounts = (1_u8..=2)
            .map(|n| {
                let actor = ActorRef::Recipient(ProtocolIdentity(u128::from(n)));
                let signer = CommandSigner::from_secret_bytes(
                    actor,
                    OperationalKeyRef(u128::from(n)),
                    &[n; 32],
                );
                let account = support::enrollment::enroll(
                    &deployment,
                    EnrollmentInput {
                        bank: BankVerification {
                            scope: scope(),
                            account: BillingAccountId(u128::from(n)),
                            member: MemberId(u128::from(n)),
                            bank_token: [n + 40; 32],
                            unit: UNIT,
                            version: 1,
                            signature: vec![],
                        }
                        .sign(&[77; 32])
                        .unwrap(),
                        persona: ProtocolIdentity(u128::from(n)),
                        actor,
                        key_ref: OperationalKeyRef(u128::from(n)),
                        initial_key: signer.verifying_key_bytes(),
                        maximum_unresolved: 10,
                    },
                    AT,
                )
                .unwrap();
                support::purchase_service(
                    &deployment,
                    scope(),
                    UNIT,
                    BillingAccountId(u128::from(n)),
                    OperationalKeyRef(u128::from(n)),
                    AT,
                )
                .unwrap();
                account
            })
            .collect();
        Self {
            url,
            deployment,
            accounts,
        }
    }
    fn send(
        &self,
        request: &SignedRequest,
    ) -> Result<Response, cs_mail_storage_postgres::StorageError> {
        DirectoryService(&self.deployment).handle(
            request,
            signing_scope(),
            Some("csqd-mail.com"),
            || AT,
        )
    }
    fn search(&self, text: &str) -> Page {
        self.deployment
            .directory_search(&DirectoryQuery::new(text.into(), 0).unwrap(), || AT)
            .unwrap()
    }
    fn status(&self, status: AccountStatus, revision: u64) {
        let signed = SignedAccountCommand {
            account: self.accounts[0],
            operational_key: OperationalKeyRef(1),
            expected_revision: revision,
            idempotency_key: IdempotencyKey(200 + u128::from(revision)),
            product: "cs-mail/test".into(),
            command: AccountCommand::SetStatus(status),
            signature: vec![],
        }
        .sign(&[1; 32])
        .unwrap();
        let repo = self.deployment.accounts(|| AT);
        AccountService::new(&repo, &|| AT)
            .execute_command(&signed)
            .unwrap();
    }
}
fn publication(
    address: u8,
    expected: u64,
    operation: u128,
    listed: bool,
    name: &str,
) -> SignedRequest {
    let address_id = ProtocolIdentity(u128::from(address));
    let key = OperationalKeyRef(u128::from(address));
    let classes = RecipientRequestClasses::new(
        address_id,
        expected + 1,
        vec![
            RequestClass::new(
                RequestClassId(1),
                "Research collaboration".into(),
                Money::from_minor_units(8),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let classes =
        CommandSigner::from_secret_bytes(ActorRef::Recipient(address_id), key, &[address; 32])
            .sign_request_classes(signing_scope(), classes)
            .unwrap();
    SignedRequest {
        scope: signing_scope(),
        address: address_id,
        key,
        operation: IdempotencyKey(operation),
        command: Command::Publish {
            mailbox: Some(
                cs_mail_accounts::directory::MemberAddress::try_from(format!(
                    "person-{address}@csqd-mail.com"
                ))
                .unwrap(),
            ),
            expected: DirectoryRevision(expected),
            profile: PublicProfile::new(
                name.into(),
                "Recipient-authored public description".into(),
                None,
            )
            .unwrap(),
            visibility: if listed {
                DiscoveryVisibility::Listed
            } else {
                DiscoveryVisibility::Unlisted
            },
            classes: Box::new(classes),
        },
        signature: vec![],
    }
    .sign(&[address; 32])
    .unwrap()
}
fn revision(response: Response) -> u64 {
    match response {
        Response::Published(r) => r.revision.0,
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn another_address_cannot_publish_configuration() {
    let f = Fixture::new();
    let mut foreign = publication(2, 0, 1, true, "Private target");
    foreign.key = OperationalKeyRef(1);
    foreign = foreign.sign(&[1; 32]).unwrap();
    assert!(f.send(&foreign).is_err());
    let add = SignedAccountCommand {
        account: f.accounts[0],
        operational_key: OperationalKeyRef(1),
        expected_revision: 0,
        idempotency_key: IdempotencyKey(100),
        product: "cs-mail/test".into(),
        command: AccountCommand::AddPersona(ProtocolIdentity(3)),
        signature: vec![],
    }
    .sign(&[1; 32])
    .unwrap();
    let repo = f.deployment.accounts(|| AT);
    AccountService::new(&repo, &|| AT)
        .execute_command(&add)
        .unwrap();
    let mut same_account = publication(3, 0, 3, true, "Another address of same account");
    same_account.key = OperationalKeyRef(1);
    same_account = same_account.sign(&[1; 32]).unwrap();
    assert!(f.send(&same_account).is_err());

    assert!(f.search("").entries.is_empty());
    assert!(
        f.deployment
            .request_classes(ProtocolIdentity(2))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        revision(f.send(&publication(2, 0, 2, true, "Actual owner")).unwrap()),
        1
    );
    // The same cross-address authority conjecture includes exclusive mailbox claims.
    let mut duplicate = publication(1, 0, 101, true, "Conflicting owner");
    if let Command::Publish { mailbox, .. } = &mut duplicate.command {
        *mailbox = Some(
            cs_mail_accounts::directory::MemberAddress::try_from(
                "person-2@csqd-mail.com".to_owned(),
            )
            .unwrap(),
        );
    }
    duplicate = duplicate.sign(&[1; 32]).unwrap();
    assert!(matches!(
        f.send(&duplicate).unwrap(),
        Response::Refused {
            error: Error::Conflict,
            ..
        }
    ));
    assert!(
        f.deployment
            .request_classes(ProtocolIdentity(1))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        f.deployment
            .directory_lookup(
                cs_mail_accounts::directory::DirectoryReference::Mailbox(
                    cs_mail_accounts::directory::MemberAddress::try_from(
                        "PERSON-2@CSQD-MAIL.COM".to_owned()
                    )
                    .unwrap()
                ),
                || AT
            )
            .unwrap()
            .unwrap()
            .entry
            .address(),
        ProtocolIdentity(2)
    );
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn receipt_failure_rolls_back_profile_and_classes_and_concurrent_edits_conflict() {
    let f = Fixture::new();
    let request = publication(1, 0, 10, true, "Original");
    let mut sql = Client::connect(&f.url, NoTls).unwrap();
    sql.batch_execute("CREATE FUNCTION fail_directory() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$; CREATE TRIGGER fail_directory BEFORE INSERT ON cs_directory_commands FOR EACH ROW EXECUTE FUNCTION fail_directory()").unwrap();
    assert!(f.send(&request).is_err());
    assert!(f.search("").entries.is_empty());
    assert!(
        f.deployment
            .request_classes(ProtocolIdentity(1))
            .unwrap()
            .is_none()
    );
    sql.batch_execute("DROP TRIGGER fail_directory ON cs_directory_commands")
        .unwrap();
    assert_eq!(revision(f.send(&request).unwrap()), 1);
    let barrier = Arc::new(Barrier::new(2));
    let handles = (20..22)
        .map(|id| {
            let url = f.url.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let d = PostgresDeployment::connect_existing(&url).unwrap();
                barrier.wait();
                DirectoryService(&d)
                    .handle(
                        &publication(1, 1, id, true, "Concurrent edit"),
                        signing_scope(),
                        Some("csqd-mail.com"),
                        || AT,
                    )
                    .unwrap()
            })
        })
        .collect::<Vec<_>>();
    let responses = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        responses
            .iter()
            .filter(|r| matches!(r, Response::Published(_)))
            .count(),
        1
    );
    assert_eq!(
        responses
            .iter()
            .filter(|r| matches!(
                r,
                Response::Refused {
                    error: Error::Conflict,
                    ..
                }
            ))
            .count(),
        1
    );
    assert_eq!(
        f.search("").entries[0].entry.revision(),
        DirectoryRevision(2)
    );
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn unlisting_excludes_search_but_direct_lookup_and_expired_profile_survive() {
    let f = Fixture::new();
    f.send(&publication(1, 0, 30, true, "Listed person"))
        .unwrap();
    assert_eq!(f.search("listed").entries.len(), 1);
    let reopened = PostgresDeployment::connect_existing(&f.url).unwrap();
    DirectoryService(&reopened)
        .handle(
            &publication(1, 1, 31, false, "Listed person"),
            signing_scope(),
            Some("csqd-mail.com"),
            || AT,
        )
        .unwrap();
    // These reads begin after unlisting commits, on a separate connection.
    assert!(f.search("listed").entries.is_empty());
    assert!(f.search("1").entries.is_empty());
    assert!(f.search("").entries.is_empty());
    let direct = f
        .deployment
        .directory_lookup(
            cs_mail_accounts::directory::DirectoryReference::Protocol(ProtocolIdentity(1)),
            || AT,
        )
        .unwrap()
        .unwrap();
    assert_eq!(direct.entry.visibility(), DiscoveryVisibility::Unlisted);
    assert!(direct.covered);
    let expired = f
        .deployment
        .directory_lookup(
            cs_mail_accounts::directory::DirectoryReference::Protocol(ProtocolIdentity(1)),
            || CanonicalTime(AT.0 + 40_000_000_000),
        )
        .unwrap()
        .unwrap();
    assert!(!expired.covered);
    assert_eq!(expired.offer.unwrap().classes.len(), 1);
    f.status(AccountStatus::Suspended, 0);
    assert!(
        f.deployment
            .directory_lookup(
                cs_mail_accounts::directory::DirectoryReference::Protocol(ProtocolIdentity(1)),
                || AT
            )
            .unwrap()
            .is_none()
    );
    f.status(AccountStatus::Active, 1);
    assert_eq!(
        f.deployment
            .directory_lookup(
                cs_mail_accounts::directory::DirectoryReference::Protocol(ProtocolIdentity(1)),
                || AT
            )
            .unwrap()
            .unwrap()
            .entry
            .revision(),
        DirectoryRevision(2)
    );
    f.status(AccountStatus::Closed, 2);
    assert!(
        f.deployment
            .directory_lookup(
                cs_mail_accounts::directory::DirectoryReference::Protocol(ProtocolIdentity(1)),
                || AT
            )
            .unwrap()
            .is_none()
    );
}

#[test]
#[ignore = "requires CS_MAIL_TEST_DATABASE_URL"]
fn lost_response_replay_preserves_later_edits_and_altered_retry_is_refused() {
    let f = Fixture::new();
    let original = publication(1, 0, 40, true, "Earlier");
    let first = f.send(&original).unwrap(); // Caller loses this response.
    assert_eq!(revision(first), 1);
    f.send(&publication(1, 1, 41, false, "Later")).unwrap();
    let d = PostgresDeployment::connect_existing(&f.url).unwrap();
    assert_eq!(
        revision(
            DirectoryService(&d)
                .handle(&original, signing_scope(), Some("csqd-mail.com"), || {
                    CanonicalTime(AT.0 + 999_999)
                })
                .unwrap()
        ),
        1
    );
    assert!(
        f.send(&publication(1, 0, 40, true, "Altered retry"))
            .is_err()
    );
    let direct = f
        .deployment
        .directory_lookup(
            cs_mail_accounts::directory::DirectoryReference::Protocol(ProtocolIdentity(1)),
            || AT,
        )
        .unwrap()
        .unwrap();
    assert_eq!(direct.entry.profile().display_name(), "Later");
    assert_eq!(direct.entry.revision(), DirectoryRevision(2));
    let stale = publication(1, 0, 42, true, "Stale edit");
    let response = f.send(&stale).unwrap();
    assert!(matches!(
        response,
        Response::Refused {
            error: Error::Conflict,
            ..
        }
    ));
    assert!(matches!(
        f.send(&stale).unwrap(),
        Response::Refused {
            error: Error::Conflict,
            ..
        }
    ));
}
