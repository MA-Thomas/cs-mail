//! Recoverable first-device enrollment. Raw login and applicant evidence is transient;
//! an encrypted reservation and stable Keychain operation survive process termination.
use super::{
    Authorization, Deployment, Error, MemberClient, Profile, State, login, storage, transport,
};
use cs_mail_accounts::control::{AccountCommand, NewOperationalKey, SignedAccountCommand};
use cs_mail_application::{accounts::EnrollmentIdentifiers, members::MemberAnswer};
use cs_mail_content::EndpointSecretKey;
use cs_mail_primitives::{ContentKeyRef, IdempotencyKey, OperationalKeyRef};
use cs_mail_protocol::ActorRef;
use cs_mail_security::MemberRead;
use cs_mail_security::{EndpointKeyPublication, SignedEndpointKey};
use ed25519_dalek::SigningKey;
use identity_contract::EnrollmentIntent;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use storage::{DeviceState, Secrets};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// The first bank adapter is explicitly the local simulated bank. A live bank must
/// supply its own verified integration request, rather than trust these applicant fields.
#[derive(Serialize, Zeroize, ZeroizeOnDrop)]
pub struct EnrollmentDetails {
    pub name: String,
    pub date_of_birth: String,
    pub address: String,
    pub phone: String,
    pub bank_account: String,
}
impl EnrollmentDetails {
    fn validate(&self) -> Result<(), Error> {
        if [
            &self.name,
            &self.date_of_birth,
            &self.address,
            &self.phone,
            &self.bank_account,
        ]
        .iter()
        .any(|v| v.trim().is_empty() || v.len() > 4096)
        {
            return Err(Error::InvalidData);
        }
        Ok(())
    }
    fn bank(&self) -> serde_json::Value {
        json!({"account": self.bank_account, "owner": {"name":self.name,"address":self.address,"phone":self.phone}})
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnrollmentStatus {
    AwaitingLogin,
    ReviewRequired,
    UnderReview,
    ReviewApproved,
    Denied,
    Ready,
}
pub enum LoginStart {
    Browser(Authorization),
    Status(EnrollmentStatus),
}
#[derive(Serialize, Deserialize)]
pub(super) struct Enrollment {
    server: String,
    deployment: Deployment,
    details_digest: [u8; 32],
    intent: Option<EnrollmentIntent>,
}
#[derive(Serialize, Deserialize)]
pub(super) enum Setup {
    Register(Option<Box<SignedAccountCommand>>),
    Publish,
    Purchase(Box<cs_mail_billing::SignedBillingCommand>),
    Directory(Box<cs_mail_application::directory::SignedRequest>),
}
#[derive(Deserialize)]
struct IntentResponse {
    intent: EnrollmentIntent,
}
#[derive(Deserialize)]
struct Outcome {
    outcome: String,
    account: Option<String>,
}
fn reference(operation: &str, role: &str) -> u128 {
    let digest = Sha256::digest(format!("cs-mail/apple-device/v1/{role}/{operation}"));
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    u128::from_be_bytes(bytes) | 1
}
impl MemberClient {
    fn enrollment_keys(&self) -> Result<Secrets, Error> {
        match storage::secrets(self.store.as_ref()) {
            Ok(keys) => Ok(keys),
            Err(Error::NotConfigured) => {
                // Cache without its key is a custody failure, never a request to overwrite it.
                if self.store.load_cache()?.is_some() {
                    return Err(Error::KeyUnavailable);
                }
                let mut keys = Secrets {
                    device: [0; 32],
                    recipient: [0; 32],
                    content: [0; 32],
                    cache: [0; 32],
                    operation: String::new(),
                };
                for key in [
                    &mut keys.device,
                    &mut keys.recipient,
                    &mut keys.content,
                    &mut keys.cache,
                ] {
                    getrandom::fill(key).map_err(|_| Error::Unavailable)?;
                }
                let mut id = [0; 16];
                getrandom::fill(&mut id).map_err(|_| Error::Unavailable)?;
                keys.operation = format!("apple-{:032x}", u128::from_be_bytes(id));
                let bytes =
                    Zeroizing::new(serde_json::to_vec(&keys).map_err(|_| Error::InvalidData)?);
                self.store.create_secret(&bytes)?;
                Ok(keys)
            }
            Err(error) => Err(error),
        }
    }
    fn refresh_enrollment_deployment(
        &self,
        server: &str,
        keys: &Secrets,
        enrollment: &mut Enrollment,
    ) -> Result<(), Error> {
        // Resume the same ownership reservation with current browser configuration.
        // Deployment identity and login issuer remain pinned across a host app cutover.
        let deployment: Deployment = self.call(server, "v1/deployment", None)?;
        if deployment.scope != enrollment.deployment.scope
            || deployment.provider_key != enrollment.deployment.provider_key
            || deployment.product != enrollment.deployment.product
            || deployment
                .browser_login
                .as_ref()
                .map(|c| (&c.issuer, &c.client_id))
                != enrollment
                    .deployment
                    .browser_login
                    .as_ref()
                    .map(|c| (&c.issuer, &c.client_id))
        {
            return Err(Error::Unauthorized);
        }
        enrollment.deployment = deployment;
        self.save_enrollment(keys, enrollment)
    }
    fn save_enrollment(&self, keys: &Secrets, enrollment: &Enrollment) -> Result<(), Error> {
        #[derive(Serialize)]
        enum Borrowed<'a> {
            Enrolling(&'a Enrollment),
        }
        storage::save_value(self.store.as_ref(), keys, &Borrowed::Enrolling(enrollment))
    }
    /// Begins a fresh browser ceremony for the same durable enrollment operation.
    /// # Errors
    /// Changed enrollment input, unavailable custody, invalid provider configuration or network failure.
    pub fn begin_login(
        &self,
        server: &str,
        details: EnrollmentDetails,
    ) -> Result<LoginStart, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        *self.login.lock().map_err(|_| Error::Unavailable)? = None;
        transport::validate_server(server)?;
        details.validate()?;
        let digest: [u8; 32] = Sha256::digest(
            Zeroizing::new(serde_json::to_vec(&details).map_err(|_| Error::InvalidData)?)
                .as_slice(),
        )
        .into();
        let keys = self.enrollment_keys()?;
        let server = server.trim_end_matches('/');
        let mut enrollment = match storage::open_device(self.store.as_ref(), &keys) {
            Ok(DeviceState::Active(_)) => return Ok(LoginStart::Status(EnrollmentStatus::Ready)),
            Ok(DeviceState::Enrolling(value)) => {
                if value.server != server || value.details_digest != digest {
                    return Err(Error::InvalidData);
                }
                *value
            }
            Err(Error::NotConfigured) => {
                let deployment: Deployment = self.call(server, "v1/deployment", None)?;
                if deployment.browser_login.is_none() {
                    return Err(Error::LoginUnavailable);
                }
                let value = Enrollment {
                    server: server.into(),
                    deployment,
                    details_digest: digest,
                    intent: None,
                };
                self.save_enrollment(&keys, &value)?;
                value
            }
            Err(error) => return Err(error),
        };
        self.refresh_enrollment_deployment(server, &keys, &mut enrollment)?;
        let config = enrollment
            .deployment
            .browser_login
            .clone()
            .ok_or(Error::LoginUnavailable)?;
        let route = format!("v1/enrollments/{}", keys.operation);
        if enrollment.intent.is_some() {
            let outcome: Outcome = self.call(server, &route, None)?;
            let status = self.apply_enrollment_outcome(&enrollment, &keys, &outcome)?;
            if !matches!(
                status,
                EnrollmentStatus::AwaitingLogin | EnrollmentStatus::ReviewApproved
            ) {
                return Ok(LoginStart::Status(status));
            }
        }
        if enrollment.intent.is_none() {
            let body = Zeroizing::new(serde_json::to_vec(&json!({"operation":keys.operation,"initial_key":SigningKey::from_bytes(&keys.device).verifying_key().to_bytes(),"bank":details.bank()})).map_err(|_| Error::InvalidData)?);
            let response: IntentResponse = self.call(server, "v1/enrollments", Some(&body))?;
            enrollment.intent = Some(response.intent);
            self.save_enrollment(&keys, &enrollment)?;
        }
        let now = i64::try_from((self.clock)().0 / 1000).map_err(|_| Error::InvalidData)?;
        let intent = enrollment.intent.as_ref().ok_or(Error::InvalidData)?;
        if intent.expires_at <= now {
            let response: IntentResponse =
                self.call(server, &format!("{route}/renew"), Some(b"{}"))?;
            if !intent.same_enrollment(&response.intent) {
                return Err(Error::InvalidData);
            }
            enrollment.intent = Some(response.intent);
            self.save_enrollment(&keys, &enrollment)?;
        }
        let intent = enrollment.intent.as_ref().ok_or(Error::InvalidData)?;
        intent.validate(now).map_err(|_| Error::Unauthorized)?;
        if intent.operation != keys.operation
            || intent.product != enrollment.deployment.product
            || intent.initial_key
                != SigningKey::from_bytes(&keys.device)
                    .verifying_key()
                    .to_bytes()
        {
            return Err(Error::InvalidData);
        }
        let (attempt, authorization) =
            login::Attempt::begin(config, intent.clone(), details, self.transport.as_ref())?;
        *self.login.lock().map_err(|_| Error::Unavailable)? = Some(attempt);
        Ok(LoginStart::Browser(authorization))
    }
    /// Consume the browser response once. After a lost completion response, reconcile;
    /// never replay the browser code or allocate a new account.
    /// # Errors
    /// Cancelled, expired or substituted browser context; server refusal or network failure.
    pub fn finish_login(&self, callback: &str) -> Result<EnrollmentStatus, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let attempt = self
            .login
            .lock()
            .map_err(|_| Error::Unavailable)?
            .take()
            .ok_or(Error::Unauthorized)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let DeviceState::Enrolling(enrollment) = storage::open_device(self.store.as_ref(), &keys)?
        else {
            return Err(Error::AlreadyConfigured);
        };
        if enrollment.intent.as_ref() != Some(&attempt.intent) {
            return Err(Error::Unauthorized);
        }
        let now = i64::try_from((self.clock)().0 / 1000).map_err(|_| Error::InvalidData)?;
        let (token, issuer, subject) = attempt.exchange(callback, now, self.transport.as_ref())?;
        let proof = attempt
            .intent
            .prove_possession(&keys.device)
            .map_err(|_| Error::InvalidData)?;
        let body = Zeroizing::new(serde_json::to_vec(&json!({"oidc_token":token.as_str(),"device_proof":proof,"login_issuer":issuer,"login_subject":subject,
            "bank":attempt.details.bank(),"applicant":{"name":attempt.details.name,"date_of_birth":attempt.details.date_of_birth,"address":attempt.details.address,"phone":attempt.details.phone}})).map_err(|_| Error::InvalidData)?);
        let outcome: Outcome = self.call(
            &enrollment.server,
            &format!("v1/enrollments/{}/complete", keys.operation),
            Some(&body),
        )?;
        self.apply_enrollment_outcome(&enrollment, &keys, &outcome)
    }
    /// # Errors
    /// Local key custody or reconciliation is unavailable.
    pub fn enrollment_status(&self) -> Result<EnrollmentStatus, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        match storage::open_device(self.store.as_ref(), &keys)? {
            DeviceState::Active(mut state) => {
                self.finish_device_setup(&mut state, &keys)?;
                Ok(EnrollmentStatus::Ready)
            }
            DeviceState::Enrolling(enrollment) => {
                if enrollment.intent.is_none() {
                    return Ok(EnrollmentStatus::AwaitingLogin);
                }
                let outcome = self.call(
                    &enrollment.server,
                    &format!("v1/enrollments/{}", keys.operation),
                    None,
                )?;
                self.apply_enrollment_outcome(&enrollment, &keys, &outcome)
            }
        }
    }
    /// Drop transient browser state and evidence; preserve the durable reservation.
    /// # Errors
    /// The client synchronization lock failed.
    pub fn cancel_login(&self) -> Result<(), Error> {
        *self.login.lock().map_err(|_| Error::Unavailable)? = None;
        Ok(())
    }
    fn apply_enrollment_outcome(
        &self,
        enrollment: &Enrollment,
        keys: &Secrets,
        outcome: &Outcome,
    ) -> Result<EnrollmentStatus, Error> {
        Ok(match outcome.outcome.as_str() {
            "awaiting-evidence" => EnrollmentStatus::AwaitingLogin,
            "review-required" => EnrollmentStatus::ReviewRequired,
            "under-review" => EnrollmentStatus::UnderReview,
            "review-approved" => EnrollmentStatus::ReviewApproved,
            "denied" => EnrollmentStatus::Denied,
            "enrolled" => {
                let intent = enrollment.intent.as_ref().ok_or(Error::InvalidData)?;
                if outcome.account.as_deref() != Some(&intent.account) {
                    return Err(Error::InvalidData);
                }
                let account = cs_mail_primitives::AccountId(
                    intent.account.parse().map_err(|_| Error::InvalidData)?,
                );
                if account.0 == 0 {
                    return Err(Error::InvalidData);
                }
                let ids = EnrollmentIdentifiers::for_operation(&keys.operation);
                let mut state = State {
                    format: 3,
                    mailbox: super::correspondence::Mailbox::default(),
                    sending: None,
                    profile: Profile {
                        server: enrollment.server.clone(),
                        persona: ids.persona,
                        account,
                        sender_key: ids.key,
                        recipient_key: OperationalKeyRef(reference(&keys.operation, "recipient")),
                        content_key: ContentKeyRef(reference(&keys.operation, "content")),
                        deployment: enrollment.deployment.clone(),
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
                    setup: Some(Setup::Register(None)),
                };
                storage::save(self.store.as_ref(), keys, &state)?;
                self.finish_device_setup(&mut state, keys)?;
                EnrollmentStatus::Ready
            }
            _ => return Err(Error::InvalidData),
        })
    }
    pub(super) fn finish_device_setup(
        &self,
        state: &mut State,
        keys: &Secrets,
    ) -> Result<(), Error> {
        if matches!(state.setup, Some(Setup::Register(_))) {
            if matches!(state.setup, Some(Setup::Register(None))) {
                let MemberAnswer::Account(account) =
                    self.read(&state.profile, keys, MemberRead::Account)?
                else {
                    return Err(Error::InvalidData);
                };
                if account.account != state.profile.account
                    || !account.personas.contains(&state.profile.persona)
                {
                    return Err(Error::Unauthorized);
                }
                let key = NewOperationalKey::prove(
                    state.profile.account,
                    &state.profile.deployment.product,
                    state.profile.recipient_key,
                    ActorRef::Recipient(state.profile.persona),
                    &keys.recipient,
                )
                .map_err(|_| Error::InvalidData)?;
                let command = SignedAccountCommand {
                    account: state.profile.account,
                    operational_key: state.profile.sender_key,
                    expected_revision: account.revision,
                    idempotency_key: IdempotencyKey(reference(&keys.operation, "register")),
                    product: state.profile.deployment.product.clone(),
                    command: AccountCommand::RegisterKey(key),
                    signature: Vec::new(),
                }
                .sign(&keys.device)
                .map_err(|_| Error::InvalidData)?;
                state.setup = Some(Setup::Register(Some(Box::new(command))));
                storage::save(self.store.as_ref(), keys, state)?;
            }
            if let Some(Setup::Register(Some(command))) = &state.setup {
                let _: serde_json::Value = self.call(
                    &state.profile.server,
                    "v1/accounts/commands",
                    Some(&serde_json::to_vec(command).map_err(|_| Error::InvalidData)?),
                )?;
                self.account_view(&state.profile, keys)?;
                state.setup = Some(Setup::Publish);
                storage::save(self.store.as_ref(), keys, state)?;
            }
        }
        if matches!(state.setup, Some(Setup::Publish)) {
            let (_, key) =
                EndpointSecretKey::from_secret_bytes(state.profile.content_key, &keys.content)
                    .map_err(|_| Error::InvalidData)?;
            let signed = SignedEndpointKey::sign(
                state.profile.deployment.scope,
                state.profile.sender_key,
                EndpointKeyPublication {
                    persona: state.profile.persona,
                    version: 1,
                    key,
                },
                &keys.device,
            )
            .map_err(|_| Error::InvalidData)?;
            let _: serde_json::Value = self.call(
                &state.profile.server,
                "v1/endpoint-keys",
                Some(&serde_json::to_vec(&signed).map_err(|_| Error::InvalidData)?),
            )?;
            // Endpoint publication is versioned and replayable. Read it back before readiness.
            let publication: EndpointKeyPublication = self.call(
                &state.profile.server,
                &format!("v1/addresses/{}/endpoint-key", state.profile.persona.0),
                None,
            )?;
            if publication != signed.publication {
                return Err(Error::InvalidData);
            }
            state.setup = None;
            storage::save(self.store.as_ref(), keys, state)?;
        }
        self.finish_onboarding(state, keys)?;
        Ok(())
    }
}
