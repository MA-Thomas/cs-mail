//! Agreed conjectures: local persistence does not disclose credentials or plaintext;
//! interruptions never replace an unresolved operation or release data after custody denial.
use super::{
    Decision, Deployment, DeviceStore, EnrollmentDetails, Error, LoginStart, MemberClient, Pending,
    Profile, State, Transport, login, storage,
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
        "provider_key":ProviderSigner::from_secret_bytes(p::ProviderRef(30),p::OperationalKeyRef(3),&[3;32]).verifying_key_bytes(),
        "browser_login":{"issuer":"http://127.0.0.1:8482","client_id":"cs-mail-apple","redirect_uri":login::CALLBACK},
        "policy":{"protocol_version":2,"policy_version":1,"privacy_profile_version":1,"retention_policy_version":1,"recipient_provider":30,"unit":1,
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
        format: 1,
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
            decision: Decision::Accept,
            route: "v1/relationships/1/2/commands".into(),
            command,
            relationship: p::RelationshipRef::from_u128_for_test(44),
        }),
        content: std::collections::BTreeMap::new(),
        last_decision: None,
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
    response: Vec<u8>,
    store: Arc<Store>,
    deny_during_fetch: AtomicBool,
}
impl Transport for ContentNetwork {
    fn request(&self, _: &str, _: &str, _: Option<&[u8]>) -> Result<Vec<u8>, Error> {
        if self.deny_during_fetch.load(Ordering::SeqCst) {
            self.store.denied.store(true, Ordering::SeqCst);
        }
        Ok(self.response.clone())
    }
    fn identity_request(&self, _: &str, _: Option<&[(&str, &str)]>) -> Result<Vec<u8>, Error> {
        Err(Error::Offline)
    }
}
fn content_fixture(keys: &storage::Secrets) -> Vec<u8> {
    use cs_mail_application::members::{
        DeliveredContent, MemberAnswer, RelationshipHandle, RelationshipView,
    };
    use cs_mail_content::{ContentBinding, ContentKeyCertificate, EndpointSecretKey};
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
    let certificate = CommandSigner::from_secret_bytes(
        cs_mail_protocol::ActorRef::Sender(handle.sender),
        p::OperationalKeyRef(11),
        &[11; 32],
    )
    .sign_content_key_certificate(ContentKeyCertificate {
        wire_version: p::WireVersion(1),
        protocol_version: p::ProtocolVersion(2),
        deployment_domain: [7; 32],
        intended_provider: p::ProviderRef(30),
        relationship: handle.relationship,
        owner: handle.sender,
        operational_key: p::OperationalKeyRef(11),
        content_key_version: p::ContentKeyVersion(1),
        key: public,
        valid_from: p::CanonicalTime(1),
        valid_until: p::CanonicalTime(1000),
    })
    .unwrap();
    let binding = ContentBinding {
        wire_version: p::WireVersion(1),
        content_ref: p::ContentRef(1),
        message_id: p::MessageId(1),
        sender: handle.sender,
        recipient: handle.recipient,
        protocol_version: p::ProtocolVersion(2),
        relationship: handle.relationship,
        content_scope: handle.content_scope,
        sender_certificate: cs_mail_content::content_key_certificate_digest(
            &certificate.certificate,
        ),
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
    };
    let record = cs_mail_content::encrypt(
        &sender,
        recipient,
        binding,
        b"private-message-canary",
        p::CanonicalTime(1),
        p::CanonicalTime(1000),
    )
    .unwrap();
    let view = RelationshipView {
        handle,
        guest: None,
        state: cs_mail_protocol::RelationshipState::Accepted,
        version: p::RelationshipVersion(1),
        followup_policy: cs_mail_protocol::FollowupPolicy::default(),
        lane: None,
        requests: Vec::new(),
        external_conversations: Vec::new(),
    };
    serde_json::to_vec(&MemberAnswer::Receiving {
        handle,
        view: Some(view),
        content: vec![DeliveredContent {
            record,
            certificate: Some(certificate),
        }],
    })
    .unwrap()
}
#[test]
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
        format: 1,
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
        setup: None,
    };
    storage::save(store.as_ref(), &keys, &state).unwrap();
    let network = Arc::new(ContentNetwork {
        response: content_fixture(&keys),
        store: store.clone(),
        deny_during_fetch: AtomicBool::new(false),
    });
    let client = MemberClient::new(
        store.clone(),
        network.clone(),
        Arc::new(|| p::CanonicalTime(100)),
    );
    assert_eq!(
        client.conversation("1").unwrap().messages[0].text,
        "private-message-canary"
    );
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
