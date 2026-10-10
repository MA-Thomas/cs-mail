//! The member request loop over one deployment: opening the relationship two addresses
//! form, receiving members' signed commands and ciphertext for it, and answering their
//! authenticated reads. Every decision stays with its owner: the kernel decides requests
//! and settlement, the application decides addresses, publications and read authority.
use crate::{CanonicalClock, IngressService, ServiceError, ServiceOutcome, ServicePolicy};
use cs_mail_application::members::{
    AccountView, RelationshipAddress, RelationshipView, authorize_read, member_history,
    publish_endpoint_key,
};
pub use cs_mail_application::members::{DeliveredContent, MemberAnswer};
use cs_mail_capabilities::SignedLaneGrant;
use cs_mail_content::EncryptedContentRecord;
use cs_mail_operations::DeploymentBootstrap;
use cs_mail_primitives::{Duration, IdempotencyKey, ProtocolIdentity};
use cs_mail_privacy::PrivacyDeriver;
use cs_mail_protocol::{ActorRef, PolicySnapshot, admission::AdmissionPolicy};
use cs_mail_security::{
    AuthoritySnapshot, EndpointKeyPublication, KeyRegistry, MemberRead, ProviderSigner,
    RecipientSigningScope, SignedCommandBytes, SignedContentKeyCertificate, SignedEndpointKey,
    SignedMemberRead, SignedRequestClasses,
};
use cs_mail_storage_postgres::{PostgresDeployment, PostgresEngine, StorageError};

/// Largest content lifetime a member may declare.
pub const MAX_CONTENT_LIFETIME: Duration = Duration(90 * 24 * 60 * 60 * 1000);
/// Version of the legacy sender identity mapping.
pub const LEGACY_IDENTITY_MAPPING_VERSION: u16 = 1;

/// Host material the gateway needs beyond the database: the provider's signing seed and
/// the derivation secrets for relationship references. None of it is ever serialized.
pub struct GatewaySecrets {
    pub provider_seed: [u8; 32],
    pub relationship_secret: [u8; 32],
    pub principal_secret: [u8; 32],
}

pub struct MemberGateway<C> {
    deployment: PostgresDeployment,
    record: DeploymentBootstrap,
    secrets: GatewaySecrets,
    clock: C,
}

impl<C: CanonicalClock + Clone + 'static> MemberGateway<C> {
    /// # Errors
    /// `SigningScopeMismatch` when the provider seed is not the bootstrapped provider key.
    pub fn new(
        deployment: PostgresDeployment,
        record: DeploymentBootstrap,
        secrets: GatewaySecrets,
        clock: C,
    ) -> Result<Self, ServiceError> {
        let key = record.record().provider_key;
        if ProviderSigner::from_secret_bytes(
            record.record().provider,
            key.reference,
            &secrets.provider_seed,
        )
        .verifying_key_bytes()
            != key.verifying_key
        {
            return Err(ServiceError::SigningScopeMismatch);
        }
        Ok(Self {
            deployment,
            record,
            secrets,
            clock,
        })
    }
    pub const fn scope(&self) -> RecipientSigningScope {
        RecipientSigningScope {
            deployment_domain: self.record.record().deployment_domain,
            provider: self.record.record().provider,
        }
    }
    pub const fn deployment(&self) -> &PostgresDeployment {
        &self.deployment
    }
    pub const fn clock(&self) -> &C {
        &self.clock
    }
    pub const fn record(&self) -> &DeploymentBootstrap {
        &self.record
    }
    /// The complete policy under the current protocol policy version.
    /// # Errors
    /// `NoProtocolPolicy` until an operator has published one.
    pub fn policy(&self) -> Result<PolicySnapshot, ServiceError> {
        let policy = self
            .deployment
            .protocol_policy()?
            .ok_or(ServiceError::NoProtocolPolicy)?;
        Ok(policy
            .snapshot(self.record.deployment_terms())
            .map_err(StorageError::from)?)
    }
    pub(crate) fn deriver(&self) -> PrivacyDeriver {
        PrivacyDeriver::new(
            self.secrets.relationship_secret,
            self.secrets.principal_secret,
        )
    }
    /// Where `sender` writing to `recipient` lives. The recipient must be a member's
    /// address and the sender a registered sender: a member's address or a guest.
    /// # Errors
    /// Unknown addresses, an address writing to itself, storage failures.
    pub fn address(
        &self,
        sender: ProtocolIdentity,
        recipient: ProtocolIdentity,
    ) -> Result<RelationshipAddress, ServiceError> {
        let accounts = self.deployment.accounts(ClockAdapter(self.clock.clone()));
        accounts.persona_account(recipient)?;
        if self.deployment.sender_subject(sender)?.is_none() {
            return Err(StorageError::Identity(identity_contract::Error::Invalid).into());
        }
        Ok(RelationshipAddress::derive(
            &self.deriver(),
            &self.record.record().deployment_domain,
            sender,
            recipient,
        )
        .map_err(StorageError::from)?)
    }
    /// Opens a member's relationship, creating it under the member's shared history. A
    /// guest's relationship is opened only by the guest request flow.
    fn open(&self, address: &RelationshipAddress) -> Result<IngressService<C>, ServiceError> {
        let engine = match self.deployment.sender_subject(address.sender())? {
            Some(cs_mail_guests::SenderSubject::Member(account)) => {
                let accounts = self.deployment.accounts(ClockAdapter(self.clock.clone()));
                let history = member_history(
                    &self.deriver(),
                    &self.record.record().deployment_domain,
                    accounts.product_account(account)?.principal,
                    address.recipient(),
                );
                self.deployment.relationship(
                    address.aggregate_key(),
                    &address.initial_state(history, self.clock.now()),
                    self.record.record().unit,
                )?
            }
            _ => self
                .deployment
                .existing_relationship(address.aggregate_key())?
                .ok_or(StorageError::Identity(identity_contract::Error::Invalid))?,
        };
        self.ingress(engine)
    }
    pub(crate) fn ingress(
        &self,
        engine: PostgresEngine,
    ) -> Result<IngressService<C>, ServiceError> {
        let record = self.record.record();
        let mut registry = KeyRegistry::default();
        registry.register(
            record.provider_key.reference,
            ActorRef::Provider(record.provider),
            record.provider_key.verifying_key,
            cs_mail_primitives::CanonicalTime(0),
        )?;
        registry.register(
            record.scheduler_key.reference,
            ActorRef::Scheduler(record.provider),
            record.scheduler_key.verifying_key,
            cs_mail_primitives::CanonicalTime(0),
        )?;
        IngressService::new_with_admission_policy(
            engine,
            registry,
            self.signer(),
            self.clock.clone(),
            ServicePolicy {
                protocol: self.policy()?,
                deployment_domain: record.deployment_domain,
                max_content_lifetime: MAX_CONTENT_LIFETIME,
                legacy_identity_mapping_version: LEGACY_IDENTITY_MAPPING_VERSION,
            },
            &AdmissionPolicy::default(),
        )
    }
    /// The provider signer for receipts and quotes.
    pub fn signer(&self) -> ProviderSigner {
        ProviderSigner::from_secret_bytes(
            self.record.record().provider,
            self.record.record().provider_key.reference,
            &self.secrets.provider_seed,
        )
    }
    /// An existing relationship, prepared for work, if it exists.
    /// # Errors
    /// Storage failures.
    pub fn existing(&self, aggregate_key: &str) -> Result<Option<IngressService<C>>, ServiceError> {
        self.deployment
            .existing_relationship(aggregate_key)?
            .map(|engine| self.ingress(engine))
            .transpose()
    }
    /// Receives one signed relationship command from a participant.
    /// # Errors
    /// Authentication, protocol and storage refusals.
    pub fn submit(
        &self,
        sender: ProtocolIdentity,
        recipient: ProtocolIdentity,
        command: &SignedCommandBytes,
    ) -> Result<ServiceOutcome, ServiceError> {
        self.open(&self.address(sender, recipient)?)?
            .submit(command)
    }
    /// Stores the sender's ciphertext for a message it will submit.
    /// # Errors
    /// Invalid certificates, bindings or lifetimes; storage refusals.
    pub fn upload(
        &self,
        sender: ProtocolIdentity,
        recipient: ProtocolIdentity,
        record: &EncryptedContentRecord,
        certificate: &SignedContentKeyCertificate,
    ) -> Result<(), ServiceError> {
        self.open(&self.address(sender, recipient)?)?
            .upload_content(record, certificate)
    }
    /// Accepts a request with an express lane: the recipient's signed grant.
    /// # Errors
    /// Invalid grants and storage refusals.
    pub fn grant_lane(
        &self,
        signed: &SignedLaneGrant,
        idempotency_key: IdempotencyKey,
    ) -> Result<ServiceOutcome, ServiceError> {
        self.open(&self.address(signed.grant.sender, signed.grant.recipient)?)?
            .grant_lane(signed, idempotency_key)
    }
    /// Publishes a recipient address's request classes.
    /// # Errors
    /// The pricing service's refusals.
    pub fn publish_classes(&self, signed: &SignedRequestClasses) -> Result<(), ServiceError> {
        cs_mail_application::request_pricing::RequestPricingService::new(&self.deployment)
            .publish_classes(signed, self.scope(), || self.clock.now())?;
        Ok(())
    }
    /// What a sender may choose when approaching `recipient`.
    /// # Errors
    /// Storage failures.
    pub fn offer(
        &self,
        recipient: ProtocolIdentity,
    ) -> Result<Option<cs_mail_protocol::pricing::SenderOffer>, ServiceError> {
        Ok(
            match (
                self.deployment.request_pricing_policy()?,
                self.deployment.request_classes(recipient)?,
            ) {
                (Some(policy), Some(classes)) => Some(policy.sender_offer(&classes)),
                _ => None,
            },
        )
    }
    /// Publishes an address's endpoint content key.
    /// # Errors
    /// The publication's refusals.
    pub fn publish_endpoint_key(&self, signed: &SignedEndpointKey) -> Result<(), ServiceError> {
        publish_endpoint_key(&self.deployment, signed, self.scope(), || self.clock.now())?;
        Ok(())
    }
    /// The endpoint content key senders encrypt to for `persona`.
    /// # Errors
    /// Storage failures.
    pub fn endpoint_key(
        &self,
        persona: ProtocolIdentity,
    ) -> Result<Option<EndpointKeyPublication>, ServiceError> {
        Ok(self.deployment.endpoint_key(persona)?)
    }
    /// Answers an authenticated member read, limited to the reader's own records.
    /// # Errors
    /// `Security` for an unauthorized or stale read; storage failures.
    pub fn read(&self, signed: &SignedMemberRead) -> Result<MemberAnswer, ServiceError> {
        let now = self.clock.now();
        let mut authority = AuthoritySnapshot::new(KeyRegistry::default())?;
        let accounts = self.deployment.accounts(ClockAdapter(self.clock.clone()));
        let account = accounts.persona_account(signed.persona)?;
        let state = self.deployment.account_state(account)?;
        authority.add_account(
            account,
            state.control.personas().clone(),
            state.registry.clone(),
        )?;
        authorize_read(signed, &authority, self.scope(), now)?;
        Ok(match signed.read {
            MemberRead::Account => {
                let billing = self
                    .deployment
                    .billing_account(accounts.product_account(account)?.billing)?;
                MemberAnswer::Account(AccountView::of(account, &state, &billing, now))
            }
            MemberRead::AsSender { recipient } => {
                let address = self.address(signed.persona, recipient)?;
                MemberAnswer::Sending {
                    handle: address.public(),
                    view: self.view(&address)?,
                }
            }
            MemberRead::AsRecipient { sender } => {
                let address = self.address(sender, signed.persona)?;
                let content = match self
                    .deployment
                    .existing_relationship(address.aggregate_key())?
                {
                    // Only admitted messages reach the recipient: content stored for a
                    // request that was never submitted stays with the sender's side.
                    Some(engine) => {
                        let admitted = engine
                            .snapshot()?
                            .messages
                            .values()
                            .map(|message| message.content)
                            .collect::<std::collections::BTreeSet<_>>();
                        engine
                            .content_with_certificates()?
                            .into_iter()
                            .filter(|(record, _)| admitted.contains(&record.binding.content_ref))
                            .map(|(record, certificate)| DeliveredContent {
                                record,
                                certificate,
                            })
                            .collect()
                    }
                    None => Vec::new(),
                };
                let invited = self
                    .deployment
                    .has_external_invitation(address.public().sender, signed.persona)?;
                MemberAnswer::Receiving {
                    handle: address.public(),
                    view: self
                        .view(&address)?
                        .and_then(|view| view.for_recipient(invited)),
                    content,
                }
            }
            MemberRead::Inbox => {
                let mut views = Vec::new();
                for stored in self.deployment.relationships_to(signed.persona)? {
                    let address = self.address(stored.sender, stored.recipient)?;
                    let invited = self
                        .deployment
                        .has_external_invitation(stored.sender, signed.persona)?;
                    if let Some(view) = self
                        .view(&address)?
                        .and_then(|view| view.for_recipient(invited))
                    {
                        views.push(view);
                    }
                }
                MemberAnswer::Inbox(views)
            }
        })
    }
    pub(crate) fn view(
        &self,
        address: &RelationshipAddress,
    ) -> Result<Option<RelationshipView>, ServiceError> {
        let Some(engine) = self
            .deployment
            .existing_relationship(address.aggregate_key())?
        else {
            return Ok(None);
        };
        let mut view = RelationshipView::of(address.public(), &engine.snapshot()?);
        if let Some(cs_mail_guests::SenderSubject::Guest(mailbox)) =
            self.deployment.sender_subject(address.sender())?
        {
            view.guest = Some(mailbox);
            view.external_conversations = self
                .deployment
                .external_conversations_for(address.aggregate_key())?;
        }
        Ok(Some(view))
    }
}

/// Adapts the service clock to the account repository's clock.
#[derive(Clone)]
struct ClockAdapter<C>(C);
impl<C: CanonicalClock + 'static> cs_mail_application::accounts::AccountClock for ClockAdapter<C> {
    fn now(&self) -> cs_mail_primitives::CanonicalTime {
        self.0.now()
    }
}

/// Delivery within the deployment: members read ciphertext from the store, so delivering
/// a committed message means only that its ciphertext is present, which the delivery
/// batch has already checked. Submission never depends on this completing.
pub struct StoredMailbox;
impl cs_mail_worker::DeliverySink for StoredMailbox {
    type Error = core::convert::Infallible;
    fn publish(
        &mut self,
        _item: &cs_mail_storage_postgres::WorkItem,
        _content: Option<&EncryptedContentRecord>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// What one pass over the relationships did.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RelationshipWorkReport {
    pub relationships: usize,
    pub schedules: usize,
    pub payments: usize,
    pub deliveries: usize,
    pub failures: usize,
}

impl<C: CanonicalClock + Clone + 'static> MemberGateway<C> {
    /// One bounded pass of relationship work: received commands in receipt order, then,
    /// for each relationship with due work, artifact signing, deadlines, request payments
    /// against `processor`, and delivery. Every item is claimed with a fenced lease, so an
    /// interrupted pass is recovered by the next.
    /// # Errors
    /// Storage failures that stop the whole pass; per-relationship failures are counted.
    pub fn run_relationship_work<P: cs_mail_finance::PaymentProcessor>(
        &self,
        processor: &mut P,
        lease: Duration,
        batch: i64,
    ) -> Result<RelationshipWorkReport, ServiceError> {
        let mut report = RelationshipWorkReport::default();
        let policy = self.policy()?;
        let signer = self.signer();
        let record = self.record.record();
        if let Some(key) = self.deployment.received_pending()?
            && let Some(engine) = self.deployment.existing_relationship(&key)?
            && cs_mail_worker::run_received_batch(&engine, &signer, self.clock.now(), lease, batch)
                .is_err()
        {
            report.failures += 1;
        }
        for key in self
            .deployment
            .relationships_with_work(self.clock.now(), batch)?
        {
            let Some(engine) = self.deployment.existing_relationship(&key)? else {
                continue;
            };
            report.relationships += 1;
            let mut failed = cs_mail_worker::run_received_batch(
                &engine,
                &signer,
                self.clock.now(),
                lease,
                batch,
            )
            .is_err();
            match cs_mail_worker::run_schedule_batch(
                &engine,
                self.clock.now(),
                lease,
                batch,
                record.provider,
                record.scheduler_key.reference,
                &policy,
            ) {
                Ok(done) => report.schedules += done.completed,
                Err(_) => failed = true,
            }
            match cs_mail_worker::run_payment_batch(
                &engine,
                processor,
                self.clock.now(),
                lease,
                batch,
                &policy,
            ) {
                Ok(done) => report.payments += done.completed,
                Err(_) => failed = true,
            }
            match cs_mail_worker::deliver_batch(
                &engine,
                &mut StoredMailbox,
                self.clock.now(),
                lease,
                batch,
            ) {
                Ok(done) => report.deliveries += done.completed,
                Err(_) => failed = true,
            }
            if failed {
                report.failures += 1;
            }
        }
        Ok(report)
    }
}
