//! Agreed conjectures: local persistence does not disclose credentials or plaintext;
//! interruptions never replace an unresolved operation or release data after custody denial.
use super::{
    Deployment, DeviceStore, EnrollmentDetails, Error, LoginStart, MemberClient, Pending, Profile,
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

#[derive(Default)]
struct Store {
    key: Mutex<Option<Vec<u8>>>,
    cache: Mutex<Option<Vec<u8>>>,
    denied: AtomicBool,
    fail_save: AtomicBool,
}
impl DeviceStore for Store {
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
        format: 3,
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
    view: cs_mail_application::members::RelationshipView,
    message: cs_mail_application::correspondence::MessageView,
    store: Arc<Store>,
    deny_during_fetch: AtomicBool,
    lose_send: AtomicBool,
    offline_after_ack: AtomicBool,
    offline: AtomicBool,
    sends: Mutex<Vec<Vec<u8>>>,
}
impl Transport for ContentNetwork {
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
                    if self.deny_during_fetch.load(Ordering::SeqCst) {
                        self.store.denied.store(true, Ordering::SeqCst);
                    }
                    Response::Message(Box::new(self.message.clone()))
                }
                Command::Send(package) => {
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
            record: MessageRecord {
                sender_key: public,
                manifest: package.manifest,
                author: handle.sender,
                admission: cs_mail_protocol::AdmissionBasis::AcceptedRelationship {
                    version: p::RelationshipVersion(1),
                },
                committed_at: p::CanonicalTime(1),
            },
            ciphertext: package.recipient_copy,
        },
        store,
        deny_during_fetch: AtomicBool::new(false),
        lose_send: AtomicBool::new(false),
        offline_after_ack: AtomicBool::new(false),
        offline: AtomicBool::new(false),
        sends: Mutex::new(vec![]),
    }
}
#[test]
#[allow(clippy::too_many_lines)] // Keep journal, replay, acknowledgement and custody observations together.
fn reading_never_persists_plaintext_and_custody_denied_during_fetch_releases_nothing() {
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
        format: 3,
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
    // A lost send response leaves only the exact ciphertext packet on disk; reopening
    // and retrying cannot allocate a new operation or persist its readable text.
    let mut state = storage::open(store.as_ref(), &keys).unwrap();
    state.sending = Some(
        client
            .prepare_send(
                &state,
                &keys,
                p::ProtocolIdentity(1),
                Some(1),
                "outgoing-private-canary",
                &[],
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
    let cs_mail_application::correspondence::Command::Send(package) = packet.command else {
        panic!()
    };
    assert_eq!(committed, package.manifest.message());
    drop(sends);
    assert_eq!(client.conversation("1").unwrap_err(), Error::Offline);
    assert!(
        storage::open(store.as_ref(), &keys)
            .unwrap()
            .sending
            .is_none()
    );
    network.offline.store(false, Ordering::SeqCst);
    let ciphertext = store.load_cache().unwrap().unwrap();
    let marker = b"private-message-canary";
    assert!(!ciphertext.windows(marker.len()).any(|part| part == marker));
    // Even the decrypted device snapshot contains only HPKE ciphertext, never display text.
    let snapshot = serde_json::to_vec(&storage::open(store.as_ref(), &keys).unwrap()).unwrap();
    assert!(!snapshot.windows(marker.len()).any(|part| part == marker));
    network.deny_during_fetch.store(true, Ordering::SeqCst);
    assert_eq!(client.conversation("1").unwrap_err(), Error::KeyUnavailable);
    assert_eq!(store.load_cache().unwrap().unwrap(), ciphertext);
}

struct DirectoryNetwork {
    sent: Mutex<Vec<Vec<u8>>>,
    lost: AtomicBool,
    fail_ack: AtomicBool,
    store: Arc<Store>,
}
impl Transport for DirectoryNetwork {
    fn request(&self, _: &str, route: &str, body: Option<&[u8]>) -> Result<Vec<u8>, Error> {
        use cs_mail_accounts::directory::DirectoryRevision;
        use cs_mail_application::directory::{Command, Receipt, Response, SignedRequest};
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
        format: 3,
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
    assert!(
        storage::open(store.as_ref(), &keys)
            .unwrap()
            .setup
            .is_none()
    );
}
