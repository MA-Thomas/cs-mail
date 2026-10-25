//! Agreed conjectures: local persistence does not disclose credentials or plaintext;
//! interruptions never replace an unresolved operation or release data after custody denial.
use super::{
    AddressChoice, AddressMutation, AddressPolicy, AddressUpdateResult, Deployment, DeviceStore,
    EnrollmentDetails, Error, LoginStart, MemberClient, Pending, PendingAddressAction, Profile,
    State, Transport, login, storage,
};
use cs_mail_primitives as p;
use cs_mail_security::{CommandSigner, ProviderSigner, ReceiptKind, ReceiptPayload, SigningScope};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

#[derive(Clone, Default, Debug, PartialEq, Eq)]
struct RetainedData {
    revision: u64,
    metadata: Option<Vec<u8>>,
    indexes: std::collections::BTreeMap<super::RetainedRecordId, Vec<u8>>,
    copies: std::collections::BTreeMap<super::RetainedRecordId, Vec<u8>>,
}
#[derive(Default)]
struct Store {
    retained: Mutex<RetainedData>,
    copy_reads: AtomicU64,
    index_reads: AtomicU64,
    retained_bytes_written: AtomicU64,
    lose_retained_ack: AtomicBool,
    key: Mutex<Option<Vec<u8>>>,
    cache: Mutex<Option<Vec<u8>>>,
    workspace: Mutex<Option<Vec<u8>>>,
    personal: Mutex<Option<Vec<u8>>>,
    denied: AtomicBool,
    fail_save: AtomicBool,
}
impl DeviceStore for Store {
    fn retained_state(&self) -> Result<(u64, Option<Vec<u8>>), Error> {
        let d = self.retained.lock().unwrap();
        Ok((d.revision, d.metadata.clone()))
    }
    fn retained_indexes(
        &self,
        after: Option<super::RetainedRecordId>,
        limit: u16,
    ) -> Result<Vec<super::RetainedIndexRow>, Error> {
        self.index_reads.fetch_add(1, Ordering::SeqCst);
        let d = self.retained.lock().unwrap();
        Ok(d.indexes
            .range((
                after.map_or(std::ops::Bound::Unbounded, std::ops::Bound::Excluded),
                std::ops::Bound::Unbounded,
            ))
            .take(limit as usize)
            .map(|(id, bytes)| super::RetainedIndexRow {
                id: *id,
                ciphertext: bytes.clone(),
            })
            .collect())
    }
    fn retained_copy(&self, id: super::RetainedRecordId) -> Result<Option<Vec<u8>>, Error> {
        self.copy_reads.fetch_add(1, Ordering::SeqCst);
        Ok(self.retained.lock().unwrap().copies.get(&id).cloned())
    }
    fn commit_retained(&self, change: super::RetainedCommit) -> Result<(), Error> {
        if self.fail_save.load(Ordering::SeqCst) {
            return Err(Error::StorageUnavailable);
        }
        let mut d = self.retained.lock().unwrap();
        if d.revision != change.expected_revision || (d.revision == 0 && change.metadata.is_none())
        {
            return Err(Error::StorageUnavailable);
        }
        if let Some(meta) = change.metadata {
            self.retained_bytes_written
                .fetch_add(meta.len() as u64, Ordering::SeqCst);
            d.metadata = Some(meta);
        }
        for write in change.writes {
            self.retained_bytes_written
                .fetch_add(write.index.ciphertext.len() as u64, Ordering::SeqCst);
            d.indexes.insert(write.index.id, write.index.ciphertext);
            match write.copy {
                super::RetainedCopyWrite::Keep => {}
                super::RetainedCopyWrite::Remove => {
                    d.copies.remove(&write.index.id);
                }
                super::RetainedCopyWrite::Replace(bytes) => {
                    self.retained_bytes_written
                        .fetch_add(bytes.len() as u64, Ordering::SeqCst);
                    d.copies.insert(write.index.id, bytes);
                }
            }
        }
        d.revision += 1;
        if self.lose_retained_ack.swap(false, Ordering::SeqCst) {
            return Err(Error::StorageUnavailable);
        }
        Ok(())
    }
    fn load_secret(&self) -> Result<Vec<u8>, Error> {
        if self.denied.load(Ordering::SeqCst) {
            return Err(Error::KeyUnavailable);
        }
        self.key.lock().unwrap().clone().ok_or(Error::NotConfigured)
    }
    fn create_secret(&self, bytes: &[u8]) -> Result<(), Error> {
        let mut key = self.key.lock().unwrap();
        if key.is_some() || self.denied.load(Ordering::SeqCst) {
            return Err(Error::KeyUnavailable);
        }
        *key = Some(bytes.to_vec());
        Ok(())
    }
    fn load_cache(&self) -> Result<Option<Vec<u8>>, Error> {
        Ok(self.cache.lock().unwrap().clone())
    }
    fn save_cache(&self, bytes: &[u8]) -> Result<(), Error> {
        if self.fail_save.load(Ordering::SeqCst) {
            return Err(Error::StorageUnavailable);
        }
        *self.cache.lock().unwrap() = Some(bytes.to_vec());
        Ok(())
    }
    fn load_workspace(&self) -> Result<Option<Vec<u8>>, Error> {
        Ok(self.workspace.lock().unwrap().clone())
    }
    fn save_workspace(&self, bytes: &[u8]) -> Result<(), Error> {
        if self.fail_save.load(Ordering::SeqCst) {
            return Err(Error::StorageUnavailable);
        }
        *self.workspace.lock().unwrap() = Some(bytes.to_vec());
        Ok(())
    }
    fn load_personal(&self) -> Result<Option<Vec<u8>>, Error> {
        Ok(self.personal.lock().unwrap().clone())
    }
    fn save_personal(&self, bytes: &[u8]) -> Result<(), Error> {
        if self.fail_save.load(Ordering::SeqCst) {
            return Err(Error::StorageUnavailable);
        }
        *self.personal.lock().unwrap() = Some(bytes.to_vec());
        Ok(())
    }
    fn save_submission(
        &self,
        cache: &[u8],
        workspace: &[u8],
        personal: &[u8],
    ) -> Result<(), Error> {
        if self.fail_save.load(Ordering::SeqCst) {
            return Err(Error::StorageUnavailable);
        }
        let mut a = self.cache.lock().unwrap();
        let mut b = self.workspace.lock().unwrap();
        *a = Some(cache.to_vec());
        *b = Some(workspace.to_vec());
        *self.personal.lock().unwrap() = Some(personal.to_vec());
        Ok(())
    }
}
fn deployment() -> Deployment {
    let scope = json!({"deployment_domain":([7;32]),"operator":30,"program":1,"payment_account":([9;32]),"protocol_version":2});
    serde_json::from_value(json!({"scope":{"deployment_domain":([7;32]),"provider":30},"product":"cs-mail/test",
        "custody_key":cs_mail_content::EndpointSecretKey::from_secret_bytes(p::ContentKeyRef(900),&[90;32]).unwrap().1,"provider_key":ProviderSigner::from_secret_bytes(p::ProviderRef(30),p::OperationalKeyRef(3),&[3;32]).verifying_key_bytes(),
        "browser_login":{"issuer":"http://127.0.0.1:8482","client_id":"cs-mail-apple","redirect_uri":login::CALLBACK},
        "policy":{"reply_lane":{"lifetime":604_800_000,"rate_limit":{"max_messages":20,"interval":86_400_000},"purpose":{"Known":"Personal"}},"protocol_version":2,"policy_version":1,"privacy_profile_version":1,"retention_policy_version":1,"recipient_provider":30,"unit":1,
            "submission_window":120_000,"decision_window":86_400_000,"quote_lifetime":120_000,"backoff":[0,1000],"expiry_cooldown":1000,"rejection_cooldown":1000,
            "payment_provider_key":([8;32]),"financial":{"scope":scope,"policy_version":1,"corporate_basis_points":300,"maturity_delay":1000}}})).unwrap()
}
fn details() -> EnrollmentDetails {
    EnrollmentDetails {
        name: "Private Applicant Canary".into(),
        date_of_birth: "1990-01-01".into(),
        address: "Private Address Canary".into(),
        phone: "55501001".into(),
        bank_account: "private-bank-canary".into(),
    }
}
#[derive(Default)]
struct EnrollmentNetwork {
    begins: Mutex<Vec<Value>>,
    now: AtomicU64,
    identity_calls: AtomicU64,
}
impl Transport for EnrollmentNetwork {
    fn request(&self, _: &str, route: &str, body: Option<&[u8]>) -> Result<Vec<u8>, Error> {
        let value = match route {
            "v1/deployment" => serde_json::to_value(deployment()).unwrap(),
            "v1/enrollments" => {
                let body: Value = serde_json::from_slice(body.unwrap()).unwrap();
                self.begins.lock().unwrap().push(body.clone());
                json!({"intent":{"version":identity_contract::VERSION,"product":"cs-mail/test","operation":body["operation"],"account":"17",
                    "challenge":"01234567890123456789012345678901","initial_key":body["initial_key"],"enrollment_digest":([1;32]),"bank_digest":([2;32]),"created_at":100,"expires_at":200}})
            }
            r if r.ends_with("/renew") => return Err(Error::Unavailable),
            _ => json!({"outcome":"awaiting-evidence"}),
        };
        Ok(serde_json::to_vec(&value).unwrap())
    }
    fn identity_request(&self, _: &str, form: Option<&[(&str, &str)]>) -> Result<Vec<u8>, Error> {
        self.identity_calls.fetch_add(1, Ordering::SeqCst);
        assert!(
            form.is_none(),
            "expired or substituted callback must not reach token exchange"
        );
        Ok(serde_json::to_vec(&json!({"issuer":"http://127.0.0.1:8482","authorization_endpoint":"http://127.0.0.1:8482/authorize","token_endpoint":"http://127.0.0.1:8482/token","code_challenge_methods_supported":["S256"]})).unwrap())
    }
}
fn client(store: Arc<Store>, network: Arc<EnrollmentNetwork>) -> MemberClient {
    let clock = network.clone();
    MemberClient::new(
        store,
        network,
        Arc::new(move || p::CanonicalTime(clock.now.load(Ordering::SeqCst))),
    )
}

#[test]
fn interrupted_login_preserves_enrollment_and_rejects_expired_or_substituted_callbacks() {
    let store = Arc::new(Store::default());
    let network = Arc::new(EnrollmentNetwork::default());
    network.now.store(110_000, Ordering::SeqCst);
    let first = client(store.clone(), network.clone());
    assert!(matches!(
        first.begin_login("http://127.0.0.1:8480", details()),
        Ok(LoginStart::Browser(_))
    ));
    drop(first);
    let reopened = client(store.clone(), network.clone());
    let LoginStart::Browser(auth) = reopened
        .begin_login("http://127.0.0.1:8480", details())
        .unwrap()
    else {
        panic!("browser")
    };
    assert_eq!(
        network.begins.lock().unwrap().len(),
        1,
        "restart must reuse its reservation"
    );
    let url = reqwest::Url::parse(&auth.url).unwrap();
    let state = url
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .into_owned();
    assert_eq!(
        reopened
            .finish_login(&format!("{}?state=other&code=stolen", login::CALLBACK))
            .unwrap_err(),
        Error::Unauthorized
    );
    assert_eq!(network.identity_calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        reopened
            .finish_login(&format!("{}?state={state}&code=replayed", login::CALLBACK))
            .unwrap_err(),
        Error::Unauthorized
    );
    let LoginStart::Browser(auth) = reopened
        .begin_login("http://127.0.0.1:8480", details())
        .unwrap()
    else {
        panic!("browser")
    };
    let url = reqwest::Url::parse(&auth.url).unwrap();
    let state = url
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .into_owned();
    network.now.store(201_000, Ordering::SeqCst);
    assert_eq!(
        reopened
            .finish_login(&format!("{}?state={state}&code=expired", login::CALLBACK))
            .unwrap_err(),
        Error::Unauthorized
    );
    assert_eq!(network.identity_calls.load(Ordering::SeqCst), 3);
    let cache = store.load_cache().unwrap().unwrap();
    for canary in [
        "Private Applicant Canary",
        "Private Address Canary",
        "private-bank-canary",
    ] {
        assert!(!cache.windows(canary.len()).any(|w| w == canary.as_bytes()));
    }
    let keys = storage::secrets(store.as_ref()).unwrap();
    for secret in [&keys.device, &keys.recipient, &keys.content, &keys.cache] {
        assert!(!cache.windows(32).any(|w| w == secret));
    }
    store.denied.store(true, Ordering::SeqCst);
    assert_eq!(reopened.cached_inbox().unwrap_err(), Error::KeyUnavailable);
    assert!(matches!(
        reopened.begin_login("http://127.0.0.1:8480", details()),
        Err(Error::KeyUnavailable)
    ));
    store.denied.store(false, Ordering::SeqCst);
    store.cache.lock().unwrap().as_mut().unwrap()[30] ^= 1;
    assert!(matches!(
        storage::open_device(store.as_ref(), &keys),
        Err(Error::InvalidData)
    ));
}

struct LostResponse {
    requests: Mutex<Vec<Vec<u8>>>,
    receipt: Vec<u8>,
    lost: AtomicBool,
}
impl Transport for LostResponse {
    fn request(&self, _: &str, _: &str, body: Option<&[u8]>) -> Result<Vec<u8>, Error> {
        self.requests.lock().unwrap().push(body.unwrap().to_vec());
        if self.lost.swap(false, Ordering::SeqCst) {
            Err(Error::Offline)
        } else {
            Ok(self.receipt.clone())
        }
    }
    fn identity_request(&self, _: &str, _: Option<&[(&str, &str)]>) -> Result<Vec<u8>, Error> {
        Err(Error::Offline)
    }
}
#[test]
#[allow(clippy::too_many_lines)] // One interruption scenario, including its signed evidence fixture.
fn lost_decision_response_and_failed_local_commit_replay_exact_operation_after_reopen() {
    let store = Arc::new(Store::default());
    let keys = storage::Secrets {
        device: [4; 32],
        recipient: [5; 32],
        content: [6; 32],
        cache: [7; 32],
        operation: "test-operation".into(),
    };
    store
        .create_secret(&serde_json::to_vec(&keys).unwrap())
        .unwrap();
    let deployment = deployment();
    let command = CommandSigner::from_secret_bytes(
        cs_mail_protocol::ActorRef::Recipient(p::ProtocolIdentity(2)),
        p::OperationalKeyRef(8),
        &keys.recipient,
    )
    .sign(
        SigningScope {
            deployment_domain: [7; 32],
            intended_provider: p::ProviderRef(30),
            relationship: p::RelationshipRef::from_u128_for_test(44),
        },
        p::ProtocolVersion(2),
        p::IdempotencyKey(99),
        cs_mail_protocol::ProtocolCommand::AcceptRelationship {
            expected_version: p::Version(1),
        },
    )
    .unwrap();
    let receipt =
        ProviderSigner::from_secret_bytes(p::ProviderRef(30), p::OperationalKeyRef(3), &[3; 32])
            .sign_receipt(ReceiptPayload {
                deployment_domain: [7; 32],
                receipt_id: p::ReceiptRef(77),
                kind: ReceiptKind::RelationshipDecisionCommitted,
                relationship: p::RelationshipRef::from_u128_for_test(44),
                command_digest: cs_mail_security::CommandDigest(
                    Sha256::digest(&command.payload).into(),
                ),
                journal_position: p::JournalPosition(1),
                received_at: p::CanonicalTime(100),
                outcome_digest: cs_mail_security::OutcomeDigest([9; 32]),
                provider: p::ProviderRef(30),
                protocol_version: p::ProtocolVersion(2),
            })
            .unwrap();
    let mut state = State {
        external: None,
        format: 5,
        mailbox: super::correspondence::Mailbox::default(),
        sending: None,
        profile: Profile {
            server: "http://127.0.0.1:8480".into(),
            persona: p::ProtocolIdentity(2),
            account: p::AccountId(17),
            sender_key: p::OperationalKeyRef(7),
            recipient_key: p::OperationalKeyRef(8),
            content_key: p::ContentKeyRef(9),
            deployment,
        },
        synchronized_at: None,
        inbox: Vec::new(),
        pending: Some(Pending {
            sender: p::ProtocolIdentity(1),
            route: "v1/relationships/1/2/commands".into(),
            command: super::PendingCommand::Relationship(command),
            relationship: p::RelationshipRef::from_u128_for_test(44),
        }),
        content: std::collections::BTreeMap::new(),
        last_decision: None,
        last_send: None,
        editing_conversation: None,
        requesting: None,
        last_request: None,
        setup: None,
    };
    storage::save(store.as_ref(), &keys, &state).unwrap();
    let network = Arc::new(LostResponse {
        requests: Mutex::new(Vec::new()),
        receipt: serde_json::to_vec(&json!({"receipt":receipt})).unwrap(),
        lost: AtomicBool::new(true),
    });
    let first = MemberClient::new(
        store.clone(),
        network.clone(),
        Arc::new(|| p::CanonicalTime(100)),
    );
    assert_eq!(
        first.send_pending(&mut state, &keys).unwrap_err(),
        Error::Offline
    );
    assert!(first.cached_inbox().unwrap().pending_sender.is_some());
    drop(first);
    let reopened = MemberClient::new(
        store.clone(),
        network.clone(),
        Arc::new(|| p::CanonicalTime(100)),
    );
    let mut recovered = storage::open(store.as_ref(), &keys).unwrap();
    store.fail_save.store(true, Ordering::SeqCst);
    assert_eq!(
        reopened.send_pending(&mut recovered, &keys).unwrap_err(),
        Error::StorageUnavailable
    );
    assert!(reopened.cached_inbox().unwrap().pending_sender.is_some());
    let mut recovered = storage::open(store.as_ref(), &keys).unwrap();
    store.fail_save.store(false, Ordering::SeqCst);
    reopened.send_pending(&mut recovered, &keys).unwrap();
    assert!(reopened.cached_inbox().unwrap().pending_sender.is_none());
    let requests = network.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests.windows(2).all(|pair| pair[0] == pair[1]));
}

struct ContentNetwork {
    fail_mailbox_page: AtomicBool,
    view: cs_mail_application::members::RelationshipView,
    message: cs_mail_application::correspondence::MessageView,
    store: Arc<Store>,
    deny_during_fetch: AtomicBool,
    fetch_gate: Mutex<Option<Arc<(std::sync::Barrier, std::sync::Barrier)>>>,
    lose_send: AtomicBool,
    offline_after_ack: AtomicBool,
    offline: AtomicBool,
    sends: Mutex<Vec<Vec<u8>>>,
}
impl Transport for ContentNetwork {
    #[allow(clippy::too_many_lines)] // Keep the existing protocol fixture's routes and injected page failure together.
    fn request(&self, _: &str, route: &str, body: Option<&[u8]>) -> Result<Vec<u8>, Error> {
        use cs_mail_application::{
            correspondence::{Command, Response, SignedRequest},
            members::MemberAnswer,
        };
        if self.offline.load(Ordering::SeqCst) {
            return Err(Error::Offline);
        }
        let value = if route == "v1/correspondence/commands" {
            let request: SignedRequest = serde_json::from_slice(body.unwrap()).unwrap();
            let response = match request.command {
                Command::Mailbox { after, .. }
                    if after > 0 && self.fail_mailbox_page.load(Ordering::SeqCst) =>
                {
                    return Err(Error::Offline);
                }
                Command::Mailbox { after, .. } => Response::Mailbox {
                    entries: if after == 0 {
                        vec![(1, self.message.record.clone())]
                    } else {
                        vec![]
                    },
                    next_after: 1,
                },
                Command::Policy { id } => {
                    Response::Conversation(cs_mail_correspondence::Conversation::new(
                        id,
                        cs_mail_correspondence::CorrespondenceRef::new(
                            p::ProtocolIdentity(1),
                            p::ProtocolIdentity(2),
                        )
                        .unwrap(),
                    ))
                }
                Command::Fetch { .. } => {
                    if let Some(gate) = self.fetch_gate.lock().unwrap().clone() {
                        gate.0.wait();
                        gate.1.wait();
                    }
                    if self.deny_during_fetch.load(Ordering::SeqCst) {
                        self.store.denied.store(true, Ordering::SeqCst);
                    }
                    Response::Message(Box::new(self.message.clone()))
                }
                Command::Send { package, .. } | Command::SendToAlias { package, .. } => {
                    self.sends.lock().unwrap().push(body.unwrap().to_vec());
                    if self.lose_send.swap(false, Ordering::SeqCst) {
                        return Err(Error::Offline);
                    }
                    if self.offline_after_ack.load(Ordering::SeqCst) {
                        self.offline.store(true, Ordering::SeqCst);
                    }
                    Response::Committed(package.manifest.message())
                }
                _ => panic!("unexpected correspondence command"),
            };
            serde_json::to_value(response).unwrap()
        } else if route == "v1/deployment" {
            serde_json::to_value(deployment()).unwrap()
        } else if route.ends_with("/correspondence-alias") {
            fixture_public_alias(route)
        } else if route.ends_with("/endpoint-key") {
            serde_json::to_value(cs_mail_security::EndpointKeyPublication {
                persona: p::ProtocolIdentity(1),
                version: 1,
                key: self.message.record.sender_key,
            })
            .unwrap()
        } else {
            assert_eq!(route, "v1/members/reads");
            let read: cs_mail_security::SignedMemberRead =
                serde_json::from_slice(body.unwrap()).unwrap();
            let answer = match read.read {
                cs_mail_security::MemberRead::AsRecipient { .. } => MemberAnswer::Receiving {
                    handle: self.view.handle,
                    view: Some(self.view.clone()),
                    content: vec![],
                },
                cs_mail_security::MemberRead::AsSender { .. } => {
                    let mut view = self.view.clone();
                    std::mem::swap(&mut view.handle.sender, &mut view.handle.recipient);
                    MemberAnswer::Sending {
                        handle: view.handle,
                        view: Some(view),
                    }
                }
                cs_mail_security::MemberRead::Inbox => MemberAnswer::Inbox(vec![self.view.clone()]),
                cs_mail_security::MemberRead::Outbox => MemberAnswer::Outbox(Vec::new()),
                cs_mail_security::MemberRead::Account => {
                    MemberAnswer::Account(cs_mail_application::members::AccountView {
                        account: p::AccountId(17),
                        personas: vec![p::ProtocolIdentity(2)],
                        status: cs_mail_accounts::control::AccountStatus::Active,
                        revision: 1,
                        keys: vec![cs_mail_application::members::KeyView {
                            reference: p::OperationalKeyRef(8),
                            actor: cs_mail_protocol::ActorRef::Recipient(p::ProtocolIdentity(2)),
                            manager: false,
                            active: true,
                        }],
                        billing: p::BillingAccountId(17),
                        billing_revision: 1,
                        financial_scope: deployment().policy.financial.scope,
                        payment_method: [0; 32],
                        covered: true,
                        contracts: 1,
                    })
                }
            };
            serde_json::to_value(answer).unwrap()
        };
        Ok(serde_json::to_vec(&value).unwrap())
    }
    fn identity_request(&self, _: &str, _: Option<&[(&str, &str)]>) -> Result<Vec<u8>, Error> {
        Err(Error::Offline)
    }
}
#[allow(clippy::too_many_lines)] // Keep the complete endpoint, assignment and ciphertext fixture visible together.
fn content_fixture(keys: &storage::Secrets, store: Arc<Store>) -> ContentNetwork {
    use crate::correspondence::{EndpointKeys, compose, prepare};
    use cs_mail_application::{
        correspondence::{MessageRecord, MessageView},
        members::{RelationshipHandle, RelationshipView},
    };
    use cs_mail_content::{ContentBinding, EndpointSecretKey};
    let handle = RelationshipHandle {
        relationship: p::RelationshipRef::from_u128_for_test(44),
        content_scope: p::ContentScopeRef::from_u128_for_test(45),
        sender: p::ProtocolIdentity(1),
        recipient: p::ProtocolIdentity(2),
    };
    let (sender, public) =
        EndpointSecretKey::from_secret_bytes(p::ContentKeyRef(10), &[10; 32]).unwrap();
    let (_, recipient) =
        EndpointSecretKey::from_secret_bytes(p::ContentKeyRef(9), &keys.content).unwrap();
    let document = compose(vec![cs_mail_correspondence::DocumentPart::Text(
        "private-message-canary".into(),
    )])
    .map(|doc| {
        doc.with_subject(
            cs_mail_correspondence::ConversationSubject::new(
                "Fixture conversation",
                cs_mail_primitives::CanonicalTime(0),
                0,
                [19; 32],
            )
            .unwrap(),
        )
    })
    .unwrap();
    let package = prepare(
        &EndpointKeys {
            owner: handle.sender,
            secret: &sender,
            public,
        },
        &document,
        cs_mail_correspondence::ConversationId::new(1).unwrap(),
        recipient,
        deployment().custody_key,
        ContentBinding {
            wire_version: p::WireVersion(1),
            content_ref: p::ContentRef(1),
            message_id: p::MessageId(1),
            sender: handle.sender,
            recipient: handle.recipient,
            protocol_version: p::ProtocolVersion(2),
            relationship: handle.relationship,
            content_scope: handle.content_scope,
            sender_certificate: cs_mail_content::ContentCertificateDigest([0; 32]),
            declarations: p::MessageDeclarations {
                purpose: p::DeclaredPurpose::Known(p::KnownPurpose::Personal),
                origin: p::OriginDeclaration {
                    mode: p::OriginMode::HumanInitiated,
                    authority: p::DeclarationAuthority::NativeSender(p::OperationalKeyRef(11)),
                },
                payload_schema: None,
            },
            message_valid_until: p::MessageValidityUntil(p::CanonicalTime(1000)),
            capability: None,
        },
        (p::CanonicalTime(1), p::CanonicalTime(1000)),
    )
    .unwrap();
    ContentNetwork {
        view: RelationshipView {
            reply_epoch: p::ReplyEpoch(0),
            display_name: None,
            handle,
            guest: None,
            state: cs_mail_protocol::RelationshipState::Accepted,
            version: p::RelationshipVersion(1),
            followup_policy: cs_mail_protocol::FollowupPolicy::default(),
            lane: None,
            requests: vec![],
            external_conversations: vec![],
        },
        message: MessageView {
            record: MessageRecord::new(cs_mail_application::correspondence::MessageRecordFields {
                receiving: fixture_assignment(
                    handle.sender,
                    handle.recipient,
                    package.manifest.conversation(),
                    package.manifest.message(),
                    p::CanonicalTime(1),
                ),
                sender_key: public,
                manifest: package.manifest,
                author: handle.sender,
                admission: cs_mail_protocol::AdmissionBasis::AcceptedRelationship {
                    version: p::RelationshipVersion(1),
                },
                committed_at: p::CanonicalTime(1),
            })
            .unwrap(),
            ciphertext: package.recipient_copy,
        },
        store,
        fail_mailbox_page: AtomicBool::new(false),
        deny_during_fetch: AtomicBool::new(false),
        fetch_gate: Mutex::new(None),
        lose_send: AtomicBool::new(false),
        offline_after_ack: AtomicBool::new(false),
        offline: AtomicBool::new(false),
        sends: Mutex::new(vec![]),
    }
}
#[test]
#[allow(clippy::too_many_lines)] // Keep journal, replay, acknowledgement and custody observations together.
fn reading_never_persists_plaintext_and_custody_denied_during_fetch_releases_nothing() {
    challenge_addressed_draft_delivery();
    let store = Arc::new(Store::default());
    let keys = storage::Secrets {
        device: [4; 32],
        recipient: [5; 32],
        content: [6; 32],
        cache: [7; 32],
        operation: "content-test".into(),
    };
    store
        .create_secret(&serde_json::to_vec(&keys).unwrap())
        .unwrap();
    let state = State {
        external: None,
        format: 5,
        mailbox: super::correspondence::Mailbox::default(),
        sending: None,
        profile: Profile {
            server: "http://127.0.0.1:8480".into(),
            persona: p::ProtocolIdentity(2),
            account: p::AccountId(17),
            sender_key: p::OperationalKeyRef(7),
            recipient_key: p::OperationalKeyRef(8),
            content_key: p::ContentKeyRef(9),
            deployment: deployment(),
        },
        synchronized_at: None,
        inbox: Vec::new(),
        pending: None,
        content: std::collections::BTreeMap::new(),
        last_decision: None,
        last_send: None,
        editing_conversation: None,
        requesting: None,
        last_request: None,
        setup: None,
    };
    storage::save(store.as_ref(), &keys, &state).unwrap();
    let network = Arc::new(content_fixture(&keys, store.clone()));
    let client = MemberClient::new(
        store.clone(),
        network.clone(),
        Arc::new(|| p::CanonicalTime(100)),
    );
    assert_eq!(
        client.conversation("1").unwrap().messages[0].text,
        "private-message-canary"
    );
    challenge_personal_reading(&client, &store, &keys);
    // Losing a later mailbox page must not persist early headers with the old cursor.
    let complete = store.load_cache().unwrap().unwrap();
    let mut empty = storage::open(store.as_ref(), &keys).unwrap();
    empty.mailbox = super::correspondence::Mailbox::default();
    storage::save(store.as_ref(), &keys, &empty).unwrap();
    network.fail_mailbox_page.store(true, Ordering::SeqCst);
    assert!(!client.refresh_mail().unwrap().native_current);
    assert!(
        storage::open(store.as_ref(), &keys)
            .unwrap()
            .mailbox
            .records
            .is_empty()
    );
    network.fail_mailbox_page.store(false, Ordering::SeqCst);
    assert!(client.refresh_mail().unwrap().native_current);
    assert_eq!(
        storage::open(store.as_ref(), &keys)
            .unwrap()
            .mailbox
            .records
            .len(),
        1
    );
    store.save_cache(&complete).unwrap();
    // Navigation exposes owned metadata without decrypting or replacing the cache.
    let before = store.load_cache().unwrap();
    let navigation = client.navigation().unwrap();
    assert_eq!(navigation.len(), 1);
    assert_eq!(navigation[0].sender, "1");
    assert!(navigation[0].sent_requests.is_empty());
    // Conjecture: each direction reflects only its own current relationship;
    // an accepted incoming grant must not invent an outgoing standing grant.
    assert!(matches!(
        navigation[0].incoming_permission,
        super::PermissionRoute::Standing
    ));
    assert!(matches!(
        navigation[0].outgoing_permission,
        super::PermissionRoute::Closed
    ));
    let mut outgoing = cs_mail_application::members::OutgoingRelationshipView {
        relationship: network.view.clone(),
        recipient_name: Some("Other member".into()),
    };
    outgoing.relationship.handle.sender = p::ProtocolIdentity(2);
    outgoing.relationship.handle.recipient = p::ProtocolIdentity(1);
    let paired = super::navigation::project(
        p::ProtocolIdentity(2),
        &[network.view.clone()],
        &[outgoing.clone()],
        p::CanonicalTime(100),
    )
    .unwrap();
    assert!(matches!(
        paired[0].incoming_permission,
        super::PermissionRoute::Standing
    ));
    assert!(matches!(
        paired[0].outgoing_permission,
        super::PermissionRoute::Standing
    ));
    outgoing.relationship.state = cs_mail_protocol::RelationshipState::Revoked;
    let paired = super::navigation::project(
        p::ProtocolIdentity(2),
        &[network.view.clone()],
        &[outgoing.clone()],
        p::CanonicalTime(100),
    )
    .unwrap();
    assert!(matches!(
        paired[0].incoming_permission,
        super::PermissionRoute::Standing
    ));
    assert!(matches!(
        paired[0].outgoing_permission,
        super::PermissionRoute::Closed
    ));
    outgoing.relationship.state = cs_mail_protocol::RelationshipState::Accepted;
    let outgoing_only = super::navigation::project(
        p::ProtocolIdentity(2),
        &[],
        &[outgoing],
        p::CanonicalTime(100),
    )
    .unwrap();
    assert!(matches!(
        outgoing_only[0].incoming_permission,
        super::PermissionRoute::Closed
    ));
    assert!(matches!(
        outgoing_only[0].outgoing_permission,
        super::PermissionRoute::Standing
    ));

    assert_eq!(store.load_cache().unwrap(), before);
    assert!(matches!(
        super::navigation::project(
            p::ProtocolIdentity(3),
            &[network.view.clone()],
            &[],
            p::CanonicalTime(100)
        ),
        Err(Error::InvalidData)
    ));
    // A lost send response leaves only the exact ciphertext packet on disk; reopening
    // and retrying cannot allocate a new operation or persist its readable text.
    let mut state = storage::open(store.as_ref(), &keys).unwrap();
    state.sending = Some(
        client
            .prepare_send(
                &state,
                &keys,
                super::correspondence::SendWriting {
                    alias: None,
                    other: p::ProtocolIdentity(1),
                    conversation: Some(1),
                    text: "outgoing-private-canary",
                    sources: &[],
                    draft: None,
                },
            )
            .unwrap(),
    );
    storage::save(store.as_ref(), &keys, &state).unwrap();
    let serialized = serde_json::to_vec(&state).unwrap();
    assert!(
        !serialized
            .windows(b"outgoing-private-canary".len())
            .any(|part| part == b"outgoing-private-canary")
    );
    // Observation must not retry the pending message.
    client.refresh_mail().unwrap();
    assert!(network.sends.lock().unwrap().is_empty());
    network.lose_send.store(true, Ordering::SeqCst);
    assert_eq!(
        client.transmit_send(&mut state, &keys).unwrap_err(),
        Error::Offline
    );
    let mut reopened = storage::open(store.as_ref(), &keys).unwrap();
    assert!(reopened.sending.is_some());
    client.transmit_send(&mut reopened, &keys).unwrap();
    assert!(
        storage::open(store.as_ref(), &keys)
            .unwrap()
            .sending
            .is_none()
    );
    let sends = network.sends.lock().unwrap();
    assert_eq!(sends.len(), 2);
    assert_eq!(sends[0], sends[1]);
    drop(sends);
    // A known delivery followed by a lost read must remain a known send outcome;
    // reporting it as a failed send would invite the user to compose a duplicate.
    network.offline_after_ack.store(true, Ordering::SeqCst);
    let committed = client
        .send_message("1", Some(1), "known-delivery-canary", &[])
        .unwrap();
    let sends = network.sends.lock().unwrap();
    let packet: cs_mail_application::correspondence::SignedRequest =
        serde_json::from_slice(sends.last().unwrap()).unwrap();
    let cs_mail_application::correspondence::Command::Send { package, .. } = packet.command else {
        panic!()
    };
    assert_eq!(committed, package.manifest.message());
    drop(sends);
    // Conjecture: retained correspondence remains searchable and readable after offline reopen.
    let offline = MemberClient::new(
        store.clone(),
        network.clone(),
        Arc::new(|| p::CanonicalTime(100)),
    );
    assert_eq!(
        offline.cached_conversation("1").unwrap().messages[0].text,
        "private-message-canary"
    );
    assert!(
        !offline
            .search_mail("private-message-canary", super::InboxScope::All, 0, 25)
            .unwrap()
            .matches
            .is_empty()
    );
    assert!(
        !offline
            .search_mail("Fixture conversation", super::InboxScope::All, 0, 25)
            .unwrap()
            .matches
            .is_empty()
    );
    let before_index_reads = store.index_reads.load(Ordering::SeqCst);
    let before_copy_reads = store.copy_reads.load(Ordering::SeqCst);
    let empty = offline
        .search_mail("wordthatdoesnotexist", super::InboxScope::All, 0, 25)
        .unwrap();
    assert!(empty.matches.is_empty());
    assert_eq!(
        store.copy_reads.load(Ordering::SeqCst),
        before_copy_reads,
        "no-match search opened message copies"
    );
    assert_eq!(
        store.index_reads.load(Ordering::SeqCst),
        before_index_reads,
        "warm search reconstructed its index"
    );
    let original_copies = store.retained.lock().unwrap().copies.clone();
    offline.rebuild_search_index().unwrap();
    assert_eq!(
        store.retained.lock().unwrap().copies,
        original_copies,
        "index rebuilding rewrote retained ciphertext copies"
    );

    assert!(
        !offline
            .search_mail("private-message-canary", super::InboxScope::All, 0, 25)
            .unwrap()
            .matches
            .is_empty()
    );
    // Conjecture: losing a local commit acknowledgement cannot strand the live index at an old revision.
    let committed_revision = store.retained.lock().unwrap().revision;
    store.lose_retained_ack.store(true, Ordering::SeqCst);
    assert_eq!(
        offline.rebuild_search_index().unwrap_err(),
        Error::StorageUnavailable
    );
    assert_eq!(
        store.retained.lock().unwrap().revision,
        committed_revision + 1
    );
    assert!(
        offline.retained.lock().unwrap().is_none(),
        "uncertain commit retained a stale index"
    );
    assert_eq!(
        offline
            .search_mail("private-message-canary", super::InboxScope::All, 0, 25)
            .unwrap()
            .matches
            .len(),
        1
    );
    let prior_retained = store.retained.lock().unwrap().clone();
    let active = storage::open(store.as_ref(), &keys).unwrap();
    let mut removed = super::retained::Retained::load(&offline, &keys, &active).unwrap();
    removed.exclude(super::retained::CopyKey {
        thread: super::MailKey::Native(cs_mail_correspondence::ConversationId::new(1).unwrap()),
        message: p::MessageId(1),
    });
    removed.save(&offline, &keys).unwrap();
    drop(removed);
    offline.rebuild_search_index().unwrap();
    assert!(
        offline
            .search_mail("private-message-canary", super::InboxScope::All, 0, 25)
            .unwrap()
            .matches
            .is_empty(),
        "rebuilding revived an excluded copy"
    );
    let checkpoint = store.retained.lock().unwrap().clone();
    store.fail_save.store(true, Ordering::SeqCst);
    assert_eq!(
        offline.rebuild_search_index().unwrap_err(),
        Error::StorageUnavailable
    );
    assert_eq!(
        store.retained.lock().unwrap().clone(),
        checkpoint,
        "failed rebuild changed its checkpoint"
    );
    store.fail_save.store(false, Ordering::SeqCst);
    *store.retained.lock().unwrap() = prior_retained;
    let expired = MemberClient::new(
        store.clone(),
        network.clone(),
        Arc::new(|| p::CanonicalTime(u64::MAX)),
    );
    assert!(
        expired
            .search_mail("private-message-canary", super::InboxScope::All, 0, 25)
            .unwrap()
            .matches
            .is_empty()
    );
    let default =
        cs_mail_correspondence::ConversationSubject::new("  ", p::CanonicalTime(0), 0, [31; 32])
            .unwrap();
    assert_eq!(default.text(), "Default subject: January-1-1970, 12:00 AM");
    assert_eq!(
        cs_mail_correspondence::ConversationSubject::new(
            "",
            p::CanonicalTime(0),
            -18_000,
            [32; 32]
        )
        .unwrap()
        .text(),
        "Default subject: December-31-1969, 7:00 PM"
    );
    assert!(
        storage::open(store.as_ref(), &keys)
            .unwrap()
            .sending
            .is_none()
    );
    network.offline.store(false, Ordering::SeqCst);
    let history = client.outgoing_work().unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].status, super::WorkStatus::Committed);
    let ciphertext = store.load_cache().unwrap().unwrap();
    let marker = b"private-message-canary";
    assert!(!ciphertext.windows(marker.len()).any(|part| part == marker));
    // Even the decrypted device snapshot contains only HPKE ciphertext, never display text.
    let snapshot = serde_json::to_vec(&storage::open(store.as_ref(), &keys).unwrap()).unwrap();
    assert!(!snapshot.windows(marker.len()).any(|part| part == marker));
    let retained_bytes = store.retained.lock().unwrap().clone();
    for bytes in retained_bytes
        .indexes
        .values()
        .chain(retained_bytes.copies.values())
    {
        assert!(!bytes.windows(marker.len()).any(|part| part == marker));
    }
    challenge_index_scale(&store, &keys);
    // Conjecture: a blocked download cannot block local queries or repopulate a locked session.
    let isolated = Arc::new(Store::default());
    *isolated.key.lock().unwrap() = store.key.lock().unwrap().clone();
    *isolated.cache.lock().unwrap() = store.cache.lock().unwrap().clone();
    let concurrent = Arc::new(MemberClient::new(
        isolated,
        network.clone(),
        Arc::new(|| p::CanonicalTime(100)),
    ));
    let gate = Arc::new((std::sync::Barrier::new(2), std::sync::Barrier::new(2)));
    *network.fetch_gate.lock().unwrap() = Some(gate.clone());
    let background = concurrent.clone();
    let download = std::thread::spawn(move || background.retain_mail(1));
    gate.0.wait();
    let (tx, rx) = std::sync::mpsc::channel();
    let querying = concurrent.clone();
    let query = std::thread::spawn(move || {
        tx.send(querying.search_mail("absentword", super::InboxScope::All, 0, 10))
            .unwrap();
    });
    let outcome = rx.recv_timeout(std::time::Duration::from_secs(2));
    if outcome.is_ok() {
        concurrent.clear_inboxes().unwrap();
    }
    gate.1.wait();
    let download_outcome = download.join().unwrap();
    query.join().unwrap();
    *network.fetch_gate.lock().unwrap() = None;
    assert!(
        outcome.is_ok(),
        "blocked network download blocked local search"
    );
    assert!(outcome.unwrap().unwrap().matches.is_empty());
    assert_eq!(download_outcome.unwrap_err(), Error::Unavailable);
    assert!(
        concurrent.retained.lock().unwrap().is_none(),
        "late download restored a locked index"
    );

    // Force a fresh download in this fixture: denied custody must prevent its checkpoint.
    *store.retained.lock().unwrap() = RetainedData::default();
    client.clear_inboxes().unwrap();
    network.deny_during_fetch.store(true, Ordering::SeqCst);
    assert_eq!(client.conversation("1").unwrap_err(), Error::KeyUnavailable);
    assert_eq!(store.load_cache().unwrap().unwrap(), ciphertext);
}

// Existing custody/recovery experiment also challenges snapshot-read races and
// reversible organization. A later arrival becoming read, or permission/cache
// mutation from organization, would falsify the personal-state boundary.
#[allow(clippy::too_many_lines)] // One read-state scenario across later arrival, organization and restoration.
fn challenge_personal_reading(client: &MemberClient, store: &Arc<Store>, keys: &storage::Secrets) {
    use super::{MailAction, MailFolder};
    let item = client.mail_items().unwrap().remove(0);
    assert_eq!(item.unread, 1);
    let original = store.load_cache().unwrap();
    client
        .organize_mail(
            item.key,
            super::InboxScope::All,
            MailAction::Read(item.messages.clone()),
        )
        .unwrap();
    assert_eq!(client.mail_items().unwrap()[0].unread, 0);
    let mut arrival = storage::open(store.as_ref(), keys).unwrap();
    let mut record: cs_mail_application::correspondence::MessageRecordFields =
        arrival.mailbox.records[0].1.clone().into();
    let doc = cs_mail_correspondence::Document::new(
        vec![cs_mail_correspondence::DocumentPart::Text(
            "later arrival".into(),
        )],
        [2; 32],
    )
    .unwrap();
    record.manifest = doc
        .manifest(p::MessageId(998), record.manifest.conversation())
        .unwrap();
    record.receiving = fixture_assignment(
        record.author,
        record.receiving.recipient(),
        record.manifest.conversation(),
        record.manifest.message(),
        record.committed_at,
    );
    arrival.mailbox.records.push((
        2,
        cs_mail_application::correspondence::MessageRecord::new(record).unwrap(),
    ));
    storage::save(store.as_ref(), keys, &arrival).unwrap();
    client
        .organize_mail(
            item.key,
            super::InboxScope::All,
            MailAction::Read(item.messages.clone()),
        )
        .unwrap();
    assert_eq!(
        client.mail_items().unwrap()[0].unread,
        1,
        "old displayed snapshot read a later arrival"
    );
    client
        .organize_mail(
            item.key,
            super::InboxScope::All,
            MailAction::Move(MailFolder::Archive),
        )
        .unwrap();
    client
        .organize_mail(
            item.key,
            super::InboxScope::All,
            MailAction::Move(MailFolder::Trash),
        )
        .unwrap();
    client
        .organize_mail(item.key, super::InboxScope::All, MailAction::Restore)
        .unwrap();
    assert_eq!(client.mail_items().unwrap()[0].folder, MailFolder::Archive);
    let before = store.load_personal().unwrap();
    store.fail_save.store(true, Ordering::SeqCst);
    assert!(matches!(
        client.organize_mail(item.key, super::InboxScope::All, MailAction::Flag(true)),
        Err(Error::StorageUnavailable)
    ));
    store.fail_save.store(false, Ordering::SeqCst);
    assert_eq!(before, store.load_personal().unwrap());
    store.save_cache(original.as_ref().unwrap()).unwrap();
    assert_eq!(client.mail_items().unwrap()[0].folder, MailFolder::Archive);
    assert_eq!(client.mail_items().unwrap()[0].unread, 0);
    let page = client
        .search_mail("private-message-canary", super::InboxScope::All, 0, 1)
        .unwrap();
    assert_eq!(page.matches.len(), 1);
    client.clear_search().unwrap();
    assert!(matches!(
        client.search_mail("private-message-canary", super::InboxScope::All, 1, 1),
        Err(Error::InvalidData)
    ));
    let bytes = store.load_personal().unwrap().unwrap();
    assert!(
        !bytes
            .windows(b"private-message-canary".len())
            .any(|w| w == b"private-message-canary")
    );
    let mut malformed: Value = serde_json::from_slice(
        &storage::decrypt(keys, &bytes, b"cs-mail/personal-mailbox/v1").unwrap(),
    )
    .unwrap();
    malformed["threads"][0]["before_trash"] = json!("Trash");
    store
        .save_personal(&storage::encrypt(keys, &malformed, b"cs-mail/personal-mailbox/v1").unwrap())
        .unwrap();
    assert!(matches!(client.mail_items(), Err(Error::InvalidData)));
    store.save_personal(&bytes).unwrap();
}

struct DirectoryNetwork {
    alias_tamper: AtomicBool,
    alias_refuse: AtomicBool,
    sent: Mutex<Vec<Vec<u8>>>,
    lost: AtomicBool,
    fail_ack: AtomicBool,
    store: Arc<Store>,
}
impl DirectoryNetwork {
    fn recipient_lookup(reference: &str) -> Result<Vec<u8>, Error> {
        use cs_mail_accounts::directory::{
            DirectoryEntry, DirectoryRevision, DiscoveryVisibility, PublicProfile,
        };
        if reference == "network-unavailable@example.test" {
            return Err(Error::Offline);
        }
        if reference != "registered@native.test" {
            return Err(Error::AddressUnavailable);
        }
        let entry = DirectoryEntry::replace(
            None,
            p::ProtocolIdentity(3),
            p::AliasId::new(77).unwrap(),
            DirectoryRevision(0),
            PublicProfile::new("Recipient".into(), String::new(), None).unwrap(),
            DiscoveryVisibility::Unlisted,
            Some(reference.to_owned().try_into().unwrap()),
        )
        .unwrap();
        Ok(
            serde_json::to_vec(&cs_mail_application::directory::Approach {
                entry,
                offer: None,
                covered: true,
            })
            .unwrap(),
        )
    }
    fn inspect_recipient_alias(request: &cs_mail_application::aliases::SignedCommand) -> Vec<u8> {
        use cs_mail_accounts::aliases::{
            Alias, AliasConfiguration, AliasDescription, ReceivingPolicy,
        };
        use cs_mail_application::aliases::{Acknowledgement, Outcome, Receipt};
        let alias = Alias::create(
            p::AliasId::new(1).unwrap(),
            request.owner,
            "owner@native.test".to_owned().try_into().unwrap(),
            AliasConfiguration {
                description: AliasDescription::try_from("Owner".to_owned()).unwrap(),
                receiving: ReceivingPolicy::Bonded,
            },
            p::CanonicalTime(100),
        )
        .unwrap();
        serde_json::to_vec(
            &Acknowledgement::sign(
                Receipt {
                    digest: request.digest().unwrap(),
                    at: p::CanonicalTime(100),
                    outcome: Outcome::Aliases(vec![alias]),
                },
                &[3; 32],
            )
            .unwrap(),
        )
        .unwrap()
    }
}
impl Transport for DirectoryNetwork {
    fn request(&self, _: &str, route: &str, body: Option<&[u8]>) -> Result<Vec<u8>, Error> {
        use cs_mail_accounts::directory::DirectoryRevision;
        use cs_mail_application::directory::{Command, Receipt, Response, SignedRequest};
        if route == "v1/directory/lookup" {
            let input: serde_json::Value = serde_json::from_slice(body.unwrap()).unwrap();
            return Self::recipient_lookup(input["reference"].as_str().unwrap());
        }
        if route == "v1/aliases/commands" {
            use cs_mail_application::aliases::{
                Acknowledgement, Command as AliasCommand, Outcome as AliasOutcome,
                Receipt as AliasReceipt, SignedCommand as AliasCommandRequest,
            };
            let bytes = body.unwrap().to_vec();
            self.sent.lock().unwrap().push(bytes.clone());
            if self.lost.swap(false, Ordering::SeqCst) {
                return Err(Error::Offline);
            }
            let request: AliasCommandRequest = serde_json::from_slice(&bytes).unwrap();
            if matches!(request.command, AliasCommand::Inspect) {
                return Ok(Self::inspect_recipient_alias(&request));
            }
            let cs_mail_application::aliases::Command::Create { id, ref creation } =
                request.command
            else {
                panic!("expected alias creation");
            };
            request
                .verify(
                    &ed25519_dalek::SigningKey::from_bytes(&[5; 32])
                        .verifying_key()
                        .to_bytes(),
                )
                .unwrap();
            assert!(matches!(request.command, AliasCommand::Create { .. }));
            let address = match &creation.address {
                cs_mail_application::aliases::AddressSelection::Generated { address }
                | cs_mail_application::aliases::AddressSelection::Chosen { address } => {
                    address.clone()
                }
            };
            let alias = cs_mail_accounts::aliases::Alias::create(
                id,
                request.owner,
                address,
                creation.configuration.clone(),
                p::CanonicalTime(100),
            )
            .unwrap();
            let alias = if alias.policy() == cs_mail_accounts::aliases::ReceivingPolicy::AliasScoped
            {
                alias
                    .with_permission(
                        alias
                            .permission()
                            .open(0.into(), p::CanonicalTime(100))
                            .unwrap(),
                    )
                    .unwrap()
            } else {
                alias
            };
            let mut acknowledgement = Acknowledgement::sign(
                AliasReceipt {
                    digest: request.digest().unwrap(),
                    at: p::CanonicalTime(100),
                    outcome: if self.alias_refuse.swap(false, Ordering::SeqCst) {
                        AliasOutcome::Refused(cs_mail_accounts::aliases::AliasError::Conflict)
                    } else {
                        AliasOutcome::Changed(vec![alias])
                    },
                },
                &[3; 32],
            )
            .unwrap();
            if self.alias_tamper.swap(false, Ordering::SeqCst) {
                acknowledgement.signature[0] ^= 1;
            }
            if self.fail_ack.swap(false, Ordering::SeqCst) {
                self.store.fail_save.store(true, Ordering::SeqCst);
            }
            return Ok(serde_json::to_vec(&acknowledgement).unwrap());
        }
        assert_eq!(route, "v1/directory/commands");
        let bytes = body.unwrap().to_vec();
        self.sent.lock().unwrap().push(bytes.clone());
        if self.lost.swap(false, Ordering::SeqCst) {
            return Err(Error::Offline);
        }
        let request: SignedRequest = serde_json::from_slice(&bytes).unwrap();
        if self.fail_ack.swap(false, Ordering::SeqCst) {
            self.store.fail_save.store(true, Ordering::SeqCst);
        }
        let Command::Publish { expected, .. } = request.command else {
            panic!("expected publication")
        };
        Ok(serde_json::to_vec(&Response::Published(Receipt {
            digest: request.digest().unwrap(),
            revision: DirectoryRevision(expected.0 + 1),
            at: p::CanonicalTime(100),
        }))
        .unwrap())
    }
    fn identity_request(&self, _: &str, _: Option<&[(&str, &str)]>) -> Result<Vec<u8>, Error> {
        Err(Error::Offline)
    }
}

// Agreed recovery conjecture: response loss and a failed local acknowledgement commit
// cannot replace an address edit's exact operation or leave its public text in SQLite.
#[test]
fn directory_edit_reopens_and_replays_exact_signed_bytes_after_lost_acknowledgement() {
    use cs_mail_accounts::directory::{DirectoryRevision, DiscoveryVisibility, PublicProfile};
    let store = Arc::new(Store::default());
    let keys = storage::Secrets {
        device: [4; 32],
        recipient: [5; 32],
        content: [6; 32],
        cache: [7; 32],
        operation: "directory-canary".into(),
    };
    store
        .create_secret(&serde_json::to_vec(&keys).unwrap())
        .unwrap();
    let state = State {
        external: None,
        format: 5,
        mailbox: super::correspondence::Mailbox::default(),
        sending: None,
        profile: Profile {
            server: "http://127.0.0.1:8480".into(),
            persona: p::ProtocolIdentity(2),
            account: p::AccountId(17),
            sender_key: p::OperationalKeyRef(7),
            recipient_key: p::OperationalKeyRef(8),
            content_key: p::ContentKeyRef(9),
            deployment: deployment(),
        },
        synchronized_at: None,
        inbox: Vec::new(),
        pending: None,
        content: std::collections::BTreeMap::new(),
        last_decision: None,
        last_send: None,
        editing_conversation: None,
        requesting: None,
        last_request: None,
        setup: None,
    };
    storage::save(store.as_ref(), &keys, &state).unwrap();
    let network = Arc::new(DirectoryNetwork {
        alias_tamper: AtomicBool::new(false),
        alias_refuse: AtomicBool::new(false),
        sent: Mutex::new(Vec::new()),
        lost: AtomicBool::new(true),
        fail_ack: AtomicBool::new(true),
        store: store.clone(),
    });
    let first = MemberClient::new(
        store.clone(),
        network.clone(),
        Arc::new(|| p::CanonicalTime(100)),
    );
    assert_eq!(
        first
            .publish_directory(
                p::AliasId::new(1).unwrap(),
                DirectoryRevision(0),
                1,
                PublicProfile::new(
                    "Public Profile Canary".into(),
                    "Public description".into(),
                    None
                )
                .unwrap(),
                DiscoveryVisibility::Listed,
                vec![],
                None
            )
            .unwrap_err(),
        Error::Offline
    );
    assert!(
        !store
            .load_cache()
            .unwrap()
            .unwrap()
            .windows(21)
            .any(|w| w == b"Public Profile Canary")
    );
    drop(first);
    let reopened = MemberClient::new(
        store.clone(),
        network.clone(),
        Arc::new(|| p::CanonicalTime(100)),
    );
    let mut state = storage::open(store.as_ref(), &keys).unwrap();
    assert_eq!(
        reopened.finish_directory(&mut state, &keys).unwrap_err(),
        Error::StorageUnavailable
    );
    store.fail_save.store(false, Ordering::SeqCst);
    let mut state = storage::open(store.as_ref(), &keys).unwrap();
    reopened.finish_directory(&mut state, &keys).unwrap();
    let sent = network.sent.lock().unwrap();
    assert_eq!(sent.len(), 3);
    assert!(sent.iter().all(|bytes| bytes == &sent[0]));
    assert_setup_complete(store.as_ref(), &keys);
    drop(sent);
    challenge_alias_client_recovery(&reopened, &store, &network, &keys);
    challenge_recipient_routing(&reopened);
}

fn challenge_recipient_routing(client: &MemberClient) {
    // Conjecture: resolving a recipient cannot silently downgrade an unavailable
    // native address or a failed lookup into external plaintext mail.
    assert!(
        matches!(client.composer_recipient("registered@native.test").unwrap(), super::ComposerRecipient::Native { other: p::ProtocolIdentity(3), alias, .. } if alias == p::AliasId::new(77).unwrap())
    );
    assert!(matches!(
        client.composer_recipient("missing@NATIVE.test"),
        Err(Error::AddressUnavailable)
    ));
    assert!(matches!(
        client.composer_recipient("network-unavailable@example.test"),
        Err(Error::Offline)
    ));
    assert!(
        matches!(client.composer_recipient("guest@EXAMPLE.test").unwrap(), super::ComposerRecipient::External { mailbox } if mailbox == "guest@example.test")
    );
}

// Conjecture: losing an external command acknowledgement cannot create a second
// disclosure on reopen. A different signed operation/ciphertext on retry falsifies it.
struct ExternalNetwork {
    native: ContentNetwork,
    commands: Mutex<Vec<Vec<u8>>>,
    lose: AtomicBool,
    member_key: cs_mail_content::EndpointPublicKey,
    fault: Mutex<Option<Error>>,
    tamper: AtomicBool,
    refuse: AtomicBool,
    pages: Mutex<Vec<u64>>,
}
impl Transport for ExternalNetwork {
    fn request(&self, server: &str, route: &str, body: Option<&[u8]>) -> Result<Vec<u8>, Error> {
        use cs_mail_application::correspondence::external::{
            Acknowledgement, Command, Outcome, Receipt, Response, SignedCommand,
        };
        if route.ends_with("/correspondence-alias") {
            return Ok(serde_json::to_vec(&fixture_public_alias(route)).unwrap());
        }
        if route == "v1/external/prepare" {
            let input: Value = serde_json::from_slice(body.unwrap()).unwrap();
            let mailbox =
                cs_mail_guests::GuestMailbox::parse(input["mailbox"].as_str().unwrap()).unwrap();
            let mut handle = self.native.view.handle;
            handle.sender = mailbox.synthetic_protocol_identity(&[7; 32]);
            return Ok(serde_json::to_vec(&json!({"relationship": handle, "member_key":self.member_key,"relay_key":deployment().custody_key})).unwrap());
        }
        if route == "v1/external/commands" {
            let signed: SignedCommand = serde_json::from_slice(body.unwrap()).unwrap();
            if let Command::Read { after } = signed.command {
                self.pages.lock().unwrap().push(after);
                let calls = self.commands.lock().unwrap();
                let start: SignedCommand = serde_json::from_slice(&calls[0]).unwrap();
                let Command::Start {
                    mailbox, message, ..
                } = start.command
                else {
                    panic!("first command must start");
                };
                let relationship = cs_mail_protocol::ProtocolState::initial_scoped(
                    message.member_copy.binding.relationship,
                    p::RequestHistoryRef::from_u128_for_test(1),
                    mailbox.synthetic_protocol_identity(&[7; 32]),
                    start.persona,
                    p::CanonicalTime(100),
                )
                .relationship;
                let conversation = cs_mail_correspondence::external::ExternalConversation::start(
                    start.conversation,
                    start.persona,
                    p::AliasId::new(start.persona.0).unwrap(),
                    mailbox,
                    [7; 32],
                    &relationship,
                    p::CanonicalTime(100),
                )
                .unwrap();
                let page = if after == 7 {
                    vec![cs_mail_application::correspondence::external::MessageView {
                    sequence: 8, message: cs_mail_application::correspondence::external::Message::new(cs_mail_application::correspondence::external::MessageFields {
                        manifest: message.manifest, sender_key: message.sender_key, member_copy: message.member_copy,
                        direction: cs_mail_application::correspondence::external::Direction::MemberToGuest,
                        committed_at: p::CanonicalTime(100), }).unwrap(), delivery: None, provider_events: vec![] }]
                } else {
                    vec![]
                };
                return Ok(serde_json::to_vec(&Response::Conversation {
                    conversation: Box::new(conversation),
                    relationship: Box::new(relationship),
                    messages: page,
                    next_after: if after == 0 { 7 } else { 8 },
                })
                .unwrap());
            }
            self.commands.lock().unwrap().push(body.unwrap().to_vec());
            if let Some(error) = self.fault.lock().unwrap().take() {
                return Err(error);
            }
            if self.lose.swap(false, Ordering::SeqCst) {
                return Err(Error::Offline);
            }
            let Command::Start { message, .. } = &signed.command else {
                panic!("must replay original start");
            };
            let mut acknowledgement = Acknowledgement {
                receipt: Receipt {
                    digest: signed.digest().unwrap(),
                    key: signed.key,
                    signature: signed.signature.clone(),
                    at: p::CanonicalTime(100),
                    outcome: if self.refuse.swap(false, Ordering::SeqCst) {
                        Outcome::Refused(cs_mail_correspondence::CorrespondenceError::Unauthorized)
                    } else {
                        Outcome::Queued(message.manifest.message())
                    },
                },
                signature: vec![],
            }
            .sign(&[3; 32])
            .unwrap();
            if self.tamper.swap(false, Ordering::SeqCst) {
                acknowledgement.receipt.outcome =
                    Outcome::Refused(cs_mail_correspondence::CorrespondenceError::Conflict);
            }
            return Ok(serde_json::to_vec(&Response::Outcome(acknowledgement)).unwrap());
        }
        self.native.request(server, route, body)
    }
    fn identity_request(&self, _: &str, _: Option<&[(&str, &str)]>) -> Result<Vec<u8>, Error> {
        Err(Error::Offline)
    }
}
#[test]
fn external_response_loss_reopens_exact_ciphertext_and_blocks_replacement() {
    let (store, keys, state) = external_device();
    let network = Arc::new(ExternalNetwork {
        native: content_fixture(&keys, store.clone()),
        commands: Mutex::new(vec![]),
        lose: AtomicBool::new(true),
        fault: Mutex::new(None),
        tamper: AtomicBool::new(false),
        refuse: AtomicBool::new(false),
        pages: Mutex::new(vec![]),
        member_key: super::correspondence::own_keys(&state.profile, &keys)
            .unwrap()
            .1,
    });
    // Conjectures: local writing remains available while network work owns its lock;
    // a stale editor or failed handoff cannot consume a later draft or submit anything;
    // a lost reply reopens exactly the prepared operation, never a second submission.
    let first = Arc::new(MemberClient::new(
        store.clone(),
        network.clone(),
        Arc::new(|| p::CanonicalTime(100)),
    ));
    challenge_draft_handoff(&first, &store, &keys, &state, &network);
    let original = first.external_inbox().unwrap().pending.unwrap();
    drop(first);
    let reopened = MemberClient::new(
        store.clone(),
        network.clone(),
        Arc::new(|| p::CanonicalTime(110)),
    );
    assert_eq!(
        reopened.external_inbox().unwrap().pending.as_deref(),
        Some(original.as_str())
    );
    assert!(matches!(
        reopened.send_external(None, "other@example.test", "changed", true),
        Err(Error::ExternalPending)
    ));
    assert_eq!(
        reopened.outgoing_work().unwrap()[0].status,
        super::WorkStatus::AwaitingConfirmation
    );
    challenge_external_errors(&reopened, &network, &original);
    assert_eq!(
        reopened.outgoing_work().unwrap()[0].status,
        super::WorkStatus::AwaitingConfirmation
    );
    // A known queue outcome cannot consume the submitted draft if its local commit fails.
    store.fail_save.store(true, Ordering::SeqCst);
    assert!(matches!(reopened.retry(), Err(Error::StorageUnavailable)));
    assert!(reopened.external_inbox().unwrap().pending.is_some());
    assert!(reopened.drafts().unwrap().is_empty());
    store.fail_save.store(false, Ordering::SeqCst);
    reopened.retry().unwrap();
    assert!(reopened.external_inbox().unwrap().pending.is_none());
    assert_eq!(
        reopened.outgoing_work().unwrap()[0].status,
        super::WorkStatus::Queued
    );
    {
        let calls = network.commands.lock().unwrap();
        assert_eq!(calls.len(), 5);
        assert!(calls.iter().all(|c| *c == calls[0]));
    }
    challenge_external_pages(&reopened, &network, &store, &keys, &original);
    // A known queue acknowledgement must survive a later inability to refresh views.
    network.native.offline.store(true, Ordering::SeqCst);
    let acknowledged = reopened
        .send_external(None, "guest@example.test", "acknowledged", true)
        .unwrap();
    assert_ne!(acknowledged, original);
    assert!(matches!(reopened.sync(), Err(Error::Offline)));
    assert!(reopened.external_inbox().unwrap().pending.is_none());
    assert!(
        !store
            .cache
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .windows(b"external-loss-private-canary".len())
            .any(|w| w == b"external-loss-private-canary")
    );
    challenge_multiple_drafts(&reopened);
}

fn challenge_multiple_drafts(client: &MemberClient) {
    // Distinct writing to the same destination must not overwrite or resume another draft.
    let target = super::DraftTarget::External { conversation: None };
    let a = client.create_draft(target.clone()).unwrap();
    let a = client
        .save_draft(
            a.id(),
            a.revision(),
            super::DraftBody::External {
                mailbox: "first@example.test".into(),
                text: "first independent draft".into(),
            },
            "",
            0,
        )
        .unwrap();
    let b = client.create_draft(target).unwrap();
    let b = client
        .save_draft(
            b.id(),
            b.revision(),
            super::DraftBody::External {
                mailbox: "second@example.test".into(),
                text: "second independent draft".into(),
            },
            "",
            0,
        )
        .unwrap();
    assert_ne!(a.id(), b.id());
    let restored = client.drafts().unwrap();
    assert_eq!(
        restored
            .iter()
            .find(|d| d.id() == a.id())
            .unwrap()
            .body()
            .text(),
        "first independent draft"
    );
    assert_eq!(
        restored
            .iter()
            .find(|d| d.id() == b.id())
            .unwrap()
            .body()
            .text(),
        "second independent draft"
    );
}

fn challenge_addressed_draft_delivery() {
    // Conjecture: an explicitly selected alias survives draft submission in the
    // signed send command rather than being replaced by the default entrance.
    let (store, keys, _) = external_device();
    let network = Arc::new(content_fixture(&keys, store.clone()));
    network.lose_send.store(false, Ordering::SeqCst);
    let client = MemberClient::new(store, network.clone(), Arc::new(|| p::CanonicalTime(100)));
    let alias = p::AliasId::new(77).unwrap();
    let draft = client
        .create_draft(super::DraftTarget::NativeAddress {
            other: p::ProtocolIdentity(1),
            alias,
        })
        .unwrap();
    let draft = client
        .save_draft(
            draft.id(),
            draft.revision(),
            super::DraftBody::Native {
                text: "addressed-draft-canary".into(),
                sources: vec![],
            },
            "Chosen route",
            0,
        )
        .unwrap();
    assert!(matches!(
        client
            .submit_draft(draft.id(), draft.revision(), false, None)
            .unwrap(),
        super::DraftSubmission::Native { .. }
    ));
    let sends = network.sends.lock().unwrap();
    let command: cs_mail_application::correspondence::SignedRequest =
        serde_json::from_slice(sends.last().unwrap()).unwrap();
    assert!(
        matches!(command.command, cs_mail_application::correspondence::Command::SendToAlias { alias: selected, .. } if selected == alias)
    );
    assert!(client.drafts().unwrap().is_empty());
}
fn challenge_included_selection_transfer(first: &MemberClient) {
    use cs_mail_correspondence::{MessageSelection, MessageVersion, SelectionElement};
    let source = super::SourceDraft {
        selection: MessageSelection::new(
            p::MessageId(1),
            MessageVersion([9; 32]),
            vec![SelectionElement::TextRange {
                block: 0,
                start: 0,
                end: 4,
            }],
        )
        .unwrap(),
        mode: super::ReuseMode::Quotation,
    };
    let draft = first
        .create_draft(super::DraftTarget::Native {
            other: p::ProtocolIdentity(2),
            conversation: None,
        })
        .unwrap();
    let draft = first
        .save_draft(
            draft.id(),
            draft.revision(),
            super::DraftBody::Native {
                text: "included-writing".into(),
                sources: vec![source],
            },
            "",
            0,
        )
        .unwrap();
    assert!(matches!(
        first.retarget_draft(
            draft.id(),
            draft.revision(),
            super::DraftTarget::External { conversation: None },
            "guest@example.test".into(),
            false
        ),
        Err(Error::InvalidData)
    ));
    let moved = first
        .retarget_draft(
            draft.id(),
            draft.revision(),
            super::DraftTarget::Native {
                other: p::ProtocolIdentity(3),
                conversation: None,
            },
            String::new(),
            false,
        )
        .unwrap();
    assert!(matches!(moved.body(), super::DraftBody::Native { sources, .. } if sources.len() == 1));
    let external = first
        .retarget_draft(
            moved.id(),
            moved.revision(),
            super::DraftTarget::External { conversation: None },
            "guest@example.test".into(),
            true,
        )
        .unwrap();
    assert_eq!(external.body().text(), "included-writing");
    first
        .discard_draft(external.id(), external.revision())
        .unwrap();
}
fn challenge_recipient_change(first: &MemberClient, store: &Store) {
    // Recipient changes must preserve one draft identity and writing, reject stale
    // editors, and never drop native material without explicit approval.
    let writing = first.create_draft(super::DraftTarget::Unaddressed).unwrap();
    let writing = first
        .save_draft(
            writing.id(),
            writing.revision(),
            super::DraftBody::Native {
                text: "recipient-change-canary".into(),
                sources: vec![],
            },
            "Native subject",
            0,
        )
        .unwrap();
    // Writing without an addressee survives encrypted reopening, but cannot send.
    let reopened = first
        .drafts()
        .unwrap()
        .into_iter()
        .find(|d| d.id() == writing.id())
        .unwrap();
    assert_eq!(reopened.body().text(), "recipient-change-canary");
    assert!(matches!(
        first.submit_draft(writing.id(), writing.revision(), false, None),
        Err(Error::InvalidData)
    ));
    store.fail_save.store(true, Ordering::SeqCst);
    assert!(matches!(
        first.retarget_draft(
            writing.id(),
            writing.revision(),
            super::DraftTarget::NativeAddress {
                other: p::ProtocolIdentity(3),
                alias: p::AliasId::new(1).unwrap()
            },
            String::new(),
            false
        ),
        Err(Error::StorageUnavailable)
    ));
    store.fail_save.store(false, Ordering::SeqCst);
    let preserved = first
        .drafts()
        .unwrap()
        .into_iter()
        .find(|d| d.id() == writing.id())
        .unwrap();
    assert_eq!(preserved.revision(), writing.revision());
    assert_eq!(preserved.target(), writing.target());
    let addressed = first
        .retarget_draft(
            writing.id(),
            writing.revision(),
            super::DraftTarget::NativeAddress {
                other: p::ProtocolIdentity(3),
                alias: p::AliasId::new(1).unwrap(),
            },
            String::new(),
            false,
        )
        .unwrap();
    assert_eq!(addressed.id(), writing.id());
    assert_eq!(addressed.subject(), "Native subject");
    assert_eq!(addressed.body().text(), "recipient-change-canary");
    assert!(matches!(
        first.retarget_draft(
            writing.id(),
            writing.revision(),
            super::DraftTarget::External { conversation: None },
            "guest@example.test".into(),
            true
        ),
        Err(Error::DraftChanged)
    ));
    let external = first
        .retarget_draft(
            addressed.id(),
            addressed.revision(),
            super::DraftTarget::External { conversation: None },
            "guest@example.test".into(),
            true,
        )
        .unwrap();
    assert_eq!(external.id(), writing.id());
    assert_eq!(external.body().text(), "recipient-change-canary");
    assert_eq!(external.subject(), "Native subject");
    challenge_fixed_reply_recipient(first);
    challenge_included_selection_transfer(first);
    first
        .discard_draft(external.id(), external.revision())
        .unwrap();
}

fn challenge_fixed_reply_recipient(first: &MemberClient) {
    let reply = first
        .create_draft(super::DraftTarget::Native {
            other: p::ProtocolIdentity(2),
            conversation: Some(cs_mail_correspondence::ConversationId::new(1).unwrap()),
        })
        .unwrap();
    assert!(matches!(
        first.retarget_draft(
            reply.id(),
            reply.revision(),
            super::DraftTarget::External { conversation: None },
            "guest@example.test".into(),
            true
        ),
        Err(Error::InvalidData)
    ));
    first.discard_draft(reply.id(), reply.revision()).unwrap();
}

fn challenge_draft_handoff(
    first: &Arc<MemberClient>,
    store: &Arc<Store>,
    keys: &storage::Secrets,
    state: &State,
    network: &ExternalNetwork,
) {
    challenge_recipient_change(first, store);
    let draft = first
        .open_draft(super::DraftTarget::External { conversation: None })
        .unwrap();
    let body = super::DraftBody::Composed {
        mailbox: "guest@example.test".into(),
        content: cs_mail_correspondence::composed::ComposedBody::new(
            cs_mail_correspondence::composed::CompositionFormat::Editable,
            "<b>external-loss-private-canary</b>".into(),
            vec![],
            vec![
                cs_mail_correspondence::composed::AttachedFile::new(
                    "private-draft-file.txt".into(),
                    b"durable-attachment-canary".to_vec(),
                )
                .unwrap(),
            ],
        )
        .unwrap(),
        sources: vec![],
    };
    let guard = first.operation.lock().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let writer = first.clone();
    let writing = std::thread::spawn(move || {
        tx.send(writer.save_draft(
            draft.id(),
            draft.revision(),
            body,
            "External subject canary",
            0,
        ))
        .unwrap();
    });
    let result = rx.recv_timeout(std::time::Duration::from_secs(2));
    drop(guard);
    writing.join().unwrap();
    let saved = result.expect("autosave waited on network lock").unwrap();
    assert!(matches!(
        first.submit_draft(saved.id(), 1.try_into().unwrap(), true, None),
        Err(Error::DraftChanged)
    ));
    store.fail_save.store(true, Ordering::SeqCst);
    assert!(matches!(
        first.submit_draft(saved.id(), saved.revision(), true, None),
        Err(Error::StorageUnavailable)
    ));
    store.fail_save.store(false, Ordering::SeqCst);
    assert_eq!(first.drafts().unwrap()[0].revision(), saved.revision());
    assert!(first.external_inbox().unwrap().pending.is_none());
    assert!(network.commands.lock().unwrap().is_empty());
    let retained = first.drafts().unwrap();
    let super::DraftBody::Composed { content, .. } = retained[0].body() else {
        panic!("formatted draft lost")
    };
    assert!(content.html().contains("<b>"));
    assert_eq!(content.files()[0].bytes(), b"durable-attachment-canary");
    let original_workspace = store.workspace.lock().unwrap().clone().unwrap();
    assert!(
        !original_workspace
            .windows(b"durable-attachment-canary".len())
            .any(|w| w == b"durable-attachment-canary")
    );
    let mut malformed: serde_json::Value = serde_json::from_slice(
        &storage::decrypt(keys, &original_workspace, b"cs-mail/member-workspace/v3").unwrap(),
    )
    .unwrap();
    malformed["drafts"][0]["revision"] = json!(0);
    *store.workspace.lock().unwrap() =
        Some(storage::encrypt(keys, &malformed, b"cs-mail/member-workspace/v3").unwrap());
    assert!(matches!(first.drafts(), Err(Error::InvalidData)));
    *store.workspace.lock().unwrap() = Some(original_workspace);
    store.denied.store(true, Ordering::SeqCst);
    assert!(matches!(first.drafts(), Err(Error::KeyUnavailable)));
    store.denied.store(false, Ordering::SeqCst);

    // A stale transport snapshot commit cannot overwrite separately persisted writing.
    storage::save(store.as_ref(), keys, state).unwrap();
    assert_eq!(
        first.drafts().unwrap()[0].body().text(),
        "external-loss-private-canary"
    );
    assert!(matches!(
        first.submit_draft(saved.id(), saved.revision(), true, None),
        Err(Error::Offline)
    ));
    assert!(first.drafts().unwrap().is_empty());
    assert!(matches!(
        first.submit_draft(saved.id(), saved.revision(), true, None),
        Err(Error::DraftChanged)
    ));
    assert!(
        !store
            .workspace
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .windows(b"external-loss-private-canary".len())
            .any(|v| v == b"external-loss-private-canary")
    );
}

fn challenge_external_errors(client: &MemberClient, network: &ExternalNetwork, original: &str) {
    // Even an error labelled refusal by a transport is not a durable acknowledgement.
    *network.fault.lock().unwrap() = Some(Error::ExternalRefused);
    assert!(matches!(client.retry(), Err(Error::ExternalRefused)));
    assert_eq!(
        client.external_inbox().unwrap().pending.as_deref(),
        Some(original)
    );
    network.tamper.store(true, Ordering::SeqCst);
    assert!(matches!(client.retry(), Err(Error::InvalidData)));
    assert_eq!(
        client.external_inbox().unwrap().pending.as_deref(),
        Some(original)
    );
}
fn challenge_external_pages(
    client: &MemberClient,
    network: &ExternalNetwork,
    store: &Arc<Store>,
    keys: &storage::Secrets,
    original: &str,
) {
    // An expired-only page advances traversal; it must not hide the later live copy.
    let mut restored = storage::open(store.as_ref(), keys).unwrap();
    let mut view = network.native.view.clone();
    view.external_conversations =
        vec![cs_mail_correspondence::ConversationId::new(original.parse().unwrap()).unwrap()];
    restored.inbox = vec![view];
    client.sync_external(&mut restored, keys).unwrap();
    assert_eq!(*network.pages.lock().unwrap(), vec![0, 7, 8]);
    let inbox = client.external_inbox().unwrap();
    assert_eq!(inbox.threads[0].messages.len(), 1);
    assert_eq!(
        inbox.threads[0].messages[0].text,
        "Subject: External subject canary\n\nexternal-loss-private-canary"
    );
    assert!(matches!(
        inbox.threads[0].permission_route,
        super::PermissionRoute::Closed
    ));
    // A label or historical lane identity alone cannot establish a current opening.
    for state in [
        cs_mail_protocol::RelationshipState::Blocked,
        cs_mail_protocol::RelationshipState::Revoked,
        cs_mail_protocol::RelationshipState::Rejected,
        cs_mail_protocol::RelationshipState::ExpressLane(p::LaneId(1)),
    ] {
        assert!(matches!(
            super::views::permission(state, None, Some(p::CanonicalTime(110))).1,
            super::PermissionRoute::Closed
        ));
    }
    let draft = client
        .open_draft(super::DraftTarget::External { conversation: None })
        .unwrap();
    let saved = client
        .save_draft(
            draft.id(),
            draft.revision(),
            super::DraftBody::External {
                mailbox: "guest@example.test".into(),
                text: "durably-refused".into(),
            },
            "",
            0,
        )
        .unwrap();
    network.refuse.store(true, Ordering::SeqCst);
    assert!(matches!(
        client.submit_draft(saved.id(), saved.revision(), true, None),
        Err(Error::ExternalRefused)
    ));
    assert!(client.external_inbox().unwrap().pending.is_none());
    let recovered = client.drafts().unwrap();
    assert_eq!(recovered[0].id(), saved.id());
    assert_eq!(recovered[0].body().text(), "durably-refused");
    assert!(recovered[0].revision().value() > saved.revision().value());
}

fn external_device() -> (Arc<Store>, storage::Secrets, State) {
    let store = Arc::new(Store::default());
    let keys = storage::Secrets {
        device: [4; 32],
        recipient: [5; 32],
        content: [6; 32],
        cache: [7; 32],
        operation: "external-loss".into(),
    };
    store
        .create_secret(&serde_json::to_vec(&keys).unwrap())
        .unwrap();
    let state = State {
        external: None,
        format: 5,
        mailbox: super::correspondence::Mailbox::default(),
        sending: None,
        profile: Profile {
            server: "http://127.0.0.1:8480".into(),
            persona: p::ProtocolIdentity(2),
            account: p::AccountId(17),
            sender_key: p::OperationalKeyRef(7),
            recipient_key: p::OperationalKeyRef(8),
            content_key: p::ContentKeyRef(9),
            deployment: deployment(),
        },
        synchronized_at: None,
        inbox: vec![],
        pending: None,
        content: std::collections::BTreeMap::new(),
        last_decision: None,
        last_send: None,
        editing_conversation: None,
        requesting: None,
        last_request: None,
        setup: None,
    };
    storage::save(store.as_ref(), &keys, &state).unwrap();
    (store, keys, state)
}

fn challenge_alias_client_recovery(
    reopened: &MemberClient,
    store: &Arc<Store>,
    network: &Arc<DirectoryNetwork>,
    keys: &storage::Secrets,
) {
    // The same recovery experiment now challenges private-alias commands:
    // response loss, substituted acknowledgements and local commit failure must
    // retain the exact signed operation in encrypted storage without replacing keys.
    let original_secret = store.load_secret().unwrap();
    network.lost.store(true, Ordering::SeqCst);
    let command = AddressMutation::Create {
        address: AddressChoice::Generated {
            address: "fixture482731@csqd-mail.com".to_owned(),
        },
        policy: AddressPolicy::AliasScoped,
        description: "Private context canary".into(),
    };
    assert_eq!(
        reopened.update_address(command).unwrap_err(),
        Error::Offline
    );
    let pending = reopened.pending_address_update().unwrap().unwrap();
    assert!(matches!(
        pending.action,
        PendingAddressAction::Create { .. }
    ));
    assert!(
        !store
            .load_cache()
            .unwrap()
            .unwrap()
            .windows(22)
            .any(|w| w == b"Private context canary")
    );
    let client = MemberClient::new(
        store.clone(),
        network.clone(),
        Arc::new(|| p::CanonicalTime(100)),
    );
    network.alias_tamper.store(true, Ordering::SeqCst);
    assert_eq!(
        client.resume_address_update().unwrap_err(),
        Error::InvalidData
    );
    assert_eq!(client.pending_address_update().unwrap().unwrap(), pending);
    network.fail_ack.store(true, Ordering::SeqCst);
    assert_eq!(
        client.resume_address_update().unwrap_err(),
        Error::StorageUnavailable
    );
    store.fail_save.store(false, Ordering::SeqCst);
    assert!(matches!(
        client.resume_address_update().unwrap(),
        Some(AddressUpdateResult::Applied { .. })
    ));
    let sent = network.sent.lock().unwrap();
    assert_eq!(sent.len(), 7);
    assert!(sent[3..].iter().all(|bytes| bytes == &sent[3]));
    assert_eq!(original_secret, store.load_secret().unwrap());
    assert!(storage::open(store.as_ref(), keys).unwrap().setup.is_none());
    drop(sent);
    network.lost.store(true, Ordering::SeqCst);
    let refused = cs_mail_application::aliases::Command::Create {
        id: p::AliasId::new(702).unwrap(),
        creation: cs_mail_application::aliases::AliasCreation {
            address: cs_mail_application::aliases::AddressSelection::Generated {
                address: "fixture482731@csqd-mail.com".to_owned().try_into().unwrap(),
            },
            configuration: cs_mail_accounts::aliases::AliasConfiguration {
                receiving: cs_mail_accounts::aliases::ReceivingPolicy::AliasScoped,
                description: "Refused context".to_owned().try_into().unwrap(),
            },
        },
    };
    assert_eq!(client.change_alias(refused).unwrap_err(), Error::Offline);
    network.alias_refuse.store(true, Ordering::SeqCst);
    assert!(matches!(
        client.resume_address_update().unwrap(),
        Some(AddressUpdateResult::Refused { .. })
    ));
    assert!(client.pending_address_update().unwrap().is_none());
    assert!(storage::open(store.as_ref(), keys).unwrap().setup.is_none());
}

fn assert_setup_complete(store: &Store, keys: &storage::Secrets) {
    assert!(storage::open(store, keys).unwrap().setup.is_none());
}

// Conjecture: index growth does not increase an unchanged-sized copy's checkpoint size,
// and a warm selective query fetches only its match. Excess bytes or unrelated copy reads falsify it.
// Optional scale experiment inside the existing retained-custody scenario, not another test case.
fn challenge_index_scale(source: &Arc<Store>, keys: &storage::Secrets) {
    if std::env::var_os("CS_MAIL_SEARCH_BENCH").is_none() {
        return;
    }
    let store = Arc::new(Store::default());
    *store.key.lock().unwrap() = source.key.lock().unwrap().clone();
    *store.cache.lock().unwrap() = source.cache.lock().unwrap().clone();
    let client = MemberClient::new(
        store.clone(),
        Arc::new(EnrollmentNetwork::default()),
        Arc::new(|| p::CanonicalTime(100)),
    );
    let state = storage::open(store.as_ref(), keys).unwrap();
    let mut index = super::retained::Retained::load(&client, keys, &state).unwrap();
    let started = std::time::Instant::now();
    let mut largest = 0;
    for n in 0..100_000u128 {
        let key = super::retained::CopyKey {
            thread: super::MailKey::External(
                cs_mail_correspondence::ConversationId::new(n + 10_000).unwrap(),
            ),
            message: p::MessageId(n + 10_000),
        };
        let before = store.retained_bytes_written.load(Ordering::SeqCst);
        index.insert(super::retained::Entry {
            key,
            native: None,
            text: format!("common needle{n:06} {}", "x".repeat(1000)),
            subject: None,
            expires: p::CanonicalTime(1_000_000),
        });
        index.save(&client, keys).unwrap();
        index.known_keys.insert(key);
        let delta = store.retained_bytes_written.load(Ordering::SeqCst) - before;
        largest = largest.max(delta);
        assert!(
            delta < 4096,
            "checkpoint size grew with mailbox instead of this message"
        );
        if [1000, 10_000, 100_000].contains(&(n + 1)) {
            drop(index);
            let before = store.copy_reads.load(Ordering::SeqCst);
            let query_started = std::time::Instant::now();
            let result = client
                .search_mail("needle000999", super::InboxScope::All, 0, 10)
                .unwrap();
            let query_time = query_started.elapsed();
            assert_eq!(result.matches.len(), 1);
            assert_eq!(store.copy_reads.load(Ordering::SeqCst) - before, 1);
            let miss_started = std::time::Instant::now();
            assert!(
                client
                    .search_mail("absentword", super::InboxScope::All, 0, 10)
                    .unwrap()
                    .matches
                    .is_empty()
            );
            let miss_time = miss_started.elapsed();
            eprintln!(
                "SEARCH SCALE messages={} selective_us={} miss_us={} largest_checkpoint_bytes={} cumulative_write_bytes={} import_elapsed_ms={}",
                n + 1,
                query_time.as_micros(),
                miss_time.as_micros(),
                largest,
                store.retained_bytes_written.load(Ordering::SeqCst),
                started.elapsed().as_millis()
            );
            index = super::retained::Retained::load(&client, keys, &state).unwrap();
        }
    }
    drop(index);
    client.clear_inboxes().unwrap();
    let reads = store.copy_reads.load(Ordering::SeqCst);
    let cold = std::time::Instant::now();
    let index = super::retained::Retained::load(&client, keys, &state).unwrap();
    assert_eq!(index.matching("needle000999").len(), 1);
    assert_eq!(
        store.copy_reads.load(Ordering::SeqCst),
        reads,
        "cold index opening read message bodies"
    );
    eprintln!(
        "SEARCH SCALE cold_index_messages=100000 cold_ms={} message_copy_reads=0",
        cold.elapsed().as_millis()
    );
}

fn fixture_assignment(
    sender: p::ProtocolIdentity,
    recipient: p::ProtocolIdentity,
    conversation: cs_mail_correspondence::ConversationId,
    message: p::MessageId,
    at: p::CanonicalTime,
) -> cs_mail_application::aliases::DeliveredRoute {
    let alias = cs_mail_accounts::aliases::Alias::create(
        p::AliasId::new(recipient.0).unwrap(),
        recipient,
        format!("fixture-{}@inbox.test", recipient.0)
            .try_into()
            .unwrap(),
        cs_mail_accounts::aliases::AliasConfiguration {
            receiving: cs_mail_accounts::aliases::ReceivingPolicy::Bonded,
            description: "Fixture route".to_owned().try_into().unwrap(),
        },
        p::CanonicalTime(0),
    )
    .unwrap();
    cs_mail_application::aliases::DeliveredRoute::native(
        &alias,
        sender,
        conversation,
        message,
        cs_mail_protocol::AdmissionBasis::AcceptedRelationship {
            version: p::RelationshipVersion(1),
        },
        at,
    )
    .unwrap()
}

fn fixture_public_alias(route: &str) -> Value {
    let owner = route.split('/').nth(2).unwrap().parse::<u128>().unwrap();
    serde_json::to_value(cs_mail_application::aliases::CorrespondenceAlias {
        owner: p::ProtocolIdentity(owner),
        id: p::AliasId::new(owner).unwrap(),
        address: format!("fixture-{owner}@inbox.test").try_into().unwrap(),
    })
    .unwrap()
}

// Conjecture: picture access requires message-specific approval committed before I/O;
// failed saves cannot disclose, unrelated IDs cannot expand scope, and reopening a
// retained picture cannot send a second request or replace account credentials.
#[test]
#[allow(clippy::too_many_lines)] // One interruption/reopen experiment with its explicit encrypted fixture.
fn email_picture_approval_is_scoped_durable_and_never_automatically_replayed() {
    use cs_mail_application::correspondence::external::{
        Direction, Message, MessageFields, MessageView,
    };
    use cs_mail_correspondence::{ConversationId, Document, DocumentPart, email::EmailContent};
    for authored in [false, true] {
        let (store, keys, mut state) = external_device();
        let credentials = store.load_secret().unwrap();
        let mailbox = cs_mail_guests::GuestMailbox::parse("notices@example.test").unwrap();
        let guest = mailbox.synthetic_protocol_identity(&[7; 32]);
        let relationship = cs_mail_protocol::ProtocolState::initial_scoped(
            p::RelationshipRef::from_u128_for_test(99),
            p::RequestHistoryRef::from_u128_for_test(1),
            guest,
            state.profile.persona,
            p::CanonicalTime(100),
        )
        .relationship;
        let id = ConversationId::new(4321).unwrap();
        let conversation = cs_mail_correspondence::external::ExternalConversation::start(
            id,
            state.profile.persona,
            p::AliasId::new(2).unwrap(),
            mailbox,
            [7; 32],
            &relationship,
            p::CanonicalTime(100),
        )
        .unwrap();
        let html = "<p>Private HTML canary</p><img src='https://example.test/picture-a'><img src='https://example.test/picture-b'>";
        let part = if authored {
            DocumentPart::Composed(
                cs_mail_correspondence::composed::ComposedBody::new(
                    cs_mail_correspondence::composed::CompositionFormat::Imported,
                    html.into(),
                    vec![],
                    vec![],
                )
                .unwrap(),
            )
        } else {
            DocumentPart::Email(
                EmailContent::from_mime(
                    format!("Content-Type: text/html\r\n\r\n{html}").into_bytes(),
                )
                .unwrap(),
            )
        };
        let doc = Document::new(vec![part], [54; 32]).unwrap();
        let message = p::MessageId(4322);
        let manifest = doc.manifest(message, id).unwrap();
        let (secret, public) = super::correspondence::own_keys(&state.profile, &keys).unwrap();
        let binding = cs_mail_content::ContentBinding {
            wire_version: p::WireVersion(1),
            protocol_version: p::ProtocolVersion(2),
            content_ref: p::ContentRef(4322),
            message_id: message,
            sender: state.profile.persona,
            recipient: state.profile.persona,
            relationship: relationship.key.reference,
            content_scope: p::ContentScopeRef::from_u128_for_test(1),
            sender_certificate: cs_mail_content::ContentCertificateDigest([0; 32]),
            declarations: p::MessageDeclarations {
                purpose: p::DeclaredPurpose::Known(p::KnownPurpose::Personal),
                origin: p::OriginDeclaration {
                    mode: p::OriginMode::HumanInitiated,
                    authority: p::DeclarationAuthority::NativeSender(state.profile.sender_key),
                },
                payload_schema: None,
            },
            message_valid_until: p::MessageValidityUntil(p::CanonicalTime(10000)),
            capability: None,
        };
        let encrypted = cs_mail_content::encrypt(
            &secret,
            public,
            binding,
            &serde_json::to_vec(&doc).unwrap(),
            p::CanonicalTime(100),
            p::CanonicalTime(10000),
        )
        .unwrap();
        let record = Message::new(MessageFields {
            manifest,
            sender_key: public,
            member_copy: encrypted,
            direction: Direction::MemberToGuest,
            committed_at: p::CanonicalTime(100),
        })
        .unwrap();
        state.external = Some(super::external::Workspace {
            pending: None,
            records: [(
                id.value(),
                super::external::Record {
                    conversation,
                    relationship,
                    messages: vec![MessageView {
                        sequence: 1,
                        message: record,
                        delivery: None,
                        provider_events: vec![],
                    }],
                },
            )]
            .into(),
        });
        storage::save(store.as_ref(), &keys, &state).unwrap();
        let client = MemberClient::new(
            store.clone(),
            Arc::new(EnrollmentNetwork::default()),
            Arc::new(|| p::CanonicalTime(110)),
        );
        let key = super::MailKey::External(id);
        let inbox = client.external_inbox().unwrap();
        let super::ReadingContent::Readable(parts) = &inbox.threads[0].messages[0].content else {
            panic!()
        };
        let super::ReadingPart::Email(email) = &parts[0] else {
            panic!()
        };
        let picture = email.pictures[0].clone();
        let calls = AtomicU64::new(0);
        let fetch = |approved: &cs_mail_correspondence::email::ApprovedPicture| {
            calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(approved.url(), picture.url);
            let saved = super::personal::Personal::load(&client, &keys, &state).unwrap();
            assert_eq!(saved.resources.len(), 1, "intent must precede the request");
            Ok((
                "image/png".into(),
                b"retained-picture-private-canary".to_vec(),
            ))
        };
        assert!(
            client
                .load_pictures_with(key, message, &["unrelated".into()], fetch)
                .is_err()
        );
        store.fail_save.store(true, Ordering::SeqCst);
        assert!(
            client
                .load_pictures_with(key, message, &[picture.id.clone()], fetch)
                .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        store.fail_save.store(false, Ordering::SeqCst);
        client
            .load_pictures_with(key, message, &[picture.id.clone()], fetch)
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        client
            .load_pictures_with(key, message, &[picture.id.clone()], fetch)
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let reopened = MemberClient::new(
            store.clone(),
            Arc::new(EnrollmentNetwork::default()),
            Arc::new(|| p::CanonicalTime(120)),
        );
        let inbox = reopened.external_inbox().unwrap();
        let super::ReadingContent::Readable(parts) = &inbox.threads[0].messages[0].content else {
            panic!()
        };
        let super::ReadingPart::Email(email) = &parts[0] else {
            panic!()
        };
        assert!(
            email
                .resources
                .iter()
                .any(|r| r.bytes == b"retained-picture-private-canary")
        );
        let other = email.pictures.iter().find(|p| p.id != picture.id).unwrap();
        let result = reopened.load_pictures_with(key, message, &[other.id.clone()], |_| {
            store.fail_save.store(true, Ordering::SeqCst);
            Ok(("image/png".into(), b"uncommitted-picture".to_vec()))
        });
        assert!(matches!(result, Err(Error::StorageUnavailable)));
        store.fail_save.store(false, Ordering::SeqCst);
        let after_loss = reopened.external_inbox().unwrap();
        let super::ReadingContent::Readable(parts) = &after_loss.threads[0].messages[0].content
        else {
            panic!()
        };
        let super::ReadingPart::Email(after_loss) = &parts[0] else {
            panic!()
        };
        assert!(
            after_loss
                .pictures
                .iter()
                .any(|p| p.id == other.id && matches!(p.status, super::PictureStatus::Attempted))
        );
        assert!(
            !after_loss
                .resources
                .iter()
                .any(|r| r.bytes == b"uncommitted-picture")
        );
        // Reading after an uncertain fetch acknowledgement never resumes retrieval.
        // The new text interpretation must repair a stale derived email index from
        // the authenticated original without replacing the original message.
        assert_eq!(reopened.index_local_mail(10).unwrap(), 1);
        let mut previous_index = super::retained::Retained::load(&reopened, &keys, &state).unwrap();
        let copy_key = super::retained::CopyKey {
            thread: key,
            message,
        };
        let mut stale_entry = previous_index
            .copy(&reopened, &keys, copy_key, p::CanonicalTime(120))
            .unwrap()
            .unwrap();
        stale_entry.text = "obsoletecollapsedwords".into();
        previous_index.insert(stale_entry);
        previous_index.save(&reopened, &keys).unwrap();
        drop(previous_index);
        let mut interpretation = super::personal::Personal::load(&reopened, &keys, &state).unwrap();
        interpretation.email_text_revision = 0;
        store
            .save_personal(&interpretation.bytes(&keys).unwrap())
            .unwrap();
        reopened.index_local_mail(10).unwrap();
        let repaired = super::retained::Retained::load(&reopened, &keys, &state).unwrap();
        let repaired = repaired
            .copy(&reopened, &keys, copy_key, p::CanonicalTime(120))
            .unwrap()
            .unwrap();
        assert!(repaired.text.contains("Private HTML canary"));
        let personal = store.load_personal().unwrap().unwrap();
        assert!(
            !personal
                .windows(b"retained-picture-private-canary".len())
                .any(|w| w == b"retained-picture-private-canary")
        );
        assert_eq!(store.load_secret().unwrap(), credentials);
    }
}
