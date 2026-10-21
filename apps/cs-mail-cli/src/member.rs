//! One member's actions. Each builds and signs a command or read, records it in the
//! journal before sending, and renders the service's answer.
use crate::api::{Api, decode};
use crate::render;
use crate::state::{JournalEntry, MemberState, Store};
use cs_mail_application::accounts::EnrollmentIdentifiers;
use cs_mail_application::members::{AccountView, MemberAnswer, RelationshipView, RequestView};
use cs_mail_capabilities::{
    DeclarationAuthorityConstraint, LaneMode, LaneSubject, RateLimit, SenderScopedGrant,
    SignedLaneGrant,
};
use cs_mail_content::{
    ContentBinding, ContentKeyCertificate, EndpointPublicKey, EndpointSecretKey,
    content_key_certificate_digest, message_declaration_digest,
};
use cs_mail_primitives::{
    CanonicalTime, ContentKeyRef, ContentKeyVersion, ContentRef, DeclarationAuthority,
    DeclaredPurpose, DeliveryIntentRef, Duration, IdempotencyKey, KnownPurpose, LaneId,
    MessageDeclarations, MessageId, MessageValidityUntil, OperationalKeyRef, OriginDeclaration,
    OriginMode, ProtocolIdentity, QuoteId, RequestClassId, RequestId, Version, WireVersion,
};
use cs_mail_protocol::{
    ActorRef, CancellationReason, FollowupPolicy, PolicySnapshot, ProtocolCommand, RequestLifecycle,
};
use cs_mail_security::{
    CommandSigner, EndpointKeyPublication, MemberRead, RecipientSigningScope, SignedContactTerms,
    SignedEndpointKey, SignedMemberRead, SigningScope,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;
mod external;
#[cfg(test)]
mod journal_tests;
mod native;

/// How long uploaded content and message validity last.
const CONTENT_LIFETIME: Duration = Duration(30 * 24 * 60 * 60 * 1000);
/// How long `request` waits for the charge's funding to become final.
const FUNDING_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Clone, Copy)]
pub enum Decision {
    Accept,
    /// An express lane lasting this many milliseconds.
    Lane(u64),
    Reject,
    Block,
    Unblock,
    Revoke,
}

#[derive(Deserialize)]
struct DeploymentInfo {
    custody_key: EndpointPublicKey,
    scope: RecipientSigningScope,
    product: String,
    policy: PolicySnapshot,
}

#[derive(Clone, Copy)]
struct RequestMessageIds {
    request: RequestId,
    message: MessageId,
    content: ContentRef,
    conversation: cs_mail_correspondence::ConversationId,
}

pub struct Member {
    store: Store,
    api: Api,
}

fn random<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0; N];
    getrandom::getrandom(&mut bytes).map_err(|e| format!("no system randomness: {e}"))?;
    Ok(bytes)
}
/// A fresh nonzero identifier.
fn identifier() -> Result<u128, String> {
    Ok(u128::from_be_bytes(random()?) | 1)
}
fn now() -> CanonicalTime {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    CanonicalTime(u64::try_from(millis).unwrap_or(u64::MAX))
}
fn read_json(path: &Path, what: &str) -> Result<Value, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {what}: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid {what}: {e}"))
}
fn public(secret: &[u8; 32]) -> [u8; 32] {
    ed25519_dalek::SigningKey::from_bytes(secret)
        .verifying_key()
        .to_bytes()
}
fn print(value: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
    );
}

pub fn init(home: &Path, server: String) -> Result<(), String> {
    Api::new(&server)?;
    let store = Store::create(
        home,
        MemberState {
            server,
            device_secret: random()?,
            recipient_secret: random()?,
            content_secret: random()?,
            content_key: identifier()?,
            enrollment: None,
            account: None,
            journal: std::collections::BTreeMap::new(),
            values: std::collections::BTreeMap::new(),
        },
    )?;
    println!(
        "{}",
        json!({ "created": Store::file(home), "device_key": public(&store.state.device_secret) })
    );
    Ok(())
}

impl Member {
    pub fn open(home: &Path) -> Result<Self, String> {
        let store = Store::open(home)?;
        let api = Api::new(&store.state.server)?;
        Ok(Self { store, api })
    }
    fn state(&self) -> &MemberState {
        &self.store.state
    }
    fn identifiers(&self) -> Result<EnrollmentIdentifiers, String> {
        let operation = self
            .state()
            .enrollment
            .as_deref()
            .ok_or("not enrolled: run enroll-begin first")?;
        Ok(EnrollmentIdentifiers::for_operation(operation))
    }
    fn persona(&self) -> Result<ProtocolIdentity, String> {
        Ok(self.identifiers()?.persona)
    }
    /// The enrollment key signs as this member's sender address.
    fn sender_signer(&self) -> Result<CommandSigner, String> {
        let ids = self.identifiers()?;
        Ok(CommandSigner::from_secret_bytes(
            ActorRef::Sender(ids.persona),
            ids.key,
            &self.state().device_secret,
        ))
    }
    fn recipient_key(&self) -> Result<OperationalKeyRef, String> {
        use sha2::Digest as _;
        let persona = self.persona()?;
        let digest = sha2::Sha256::digest(format!("cs-mail-cli/recipient-key/v1/{}", persona.0));
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&digest[..16]);
        Ok(OperationalKeyRef(u128::from_be_bytes(bytes) | (1 << 127)))
    }
    fn recipient_signer(&self) -> Result<CommandSigner, String> {
        Ok(CommandSigner::from_secret_bytes(
            ActorRef::Recipient(self.persona()?),
            self.recipient_key()?,
            &self.state().recipient_secret,
        ))
    }
    fn content_key(&self) -> Result<(EndpointSecretKey, EndpointPublicKey), String> {
        EndpointSecretKey::from_secret_bytes(
            ContentKeyRef(self.state().content_key),
            &self.state().content_secret,
        )
        .map_err(|e| format!("content key: {e:?}"))
    }
    fn deployment(&self) -> Result<DeploymentInfo, String> {
        decode(
            self.api.get("v1/deployment").map_err(|e| e.to_string())?,
            "deployment",
        )
    }
    fn account_id(&self) -> Result<u128, String> {
        self.state()
            .account
            .ok_or_else(|| "not enrolled: complete enrollment first".to_owned())
    }

    /// Sends a journaled request. A step already answered returns its recorded answer; a
    /// step sent without a known answer is sent again with the same bytes.
    fn journaled(
        &mut self,
        step: &str,
        route: &str,
        body: impl FnOnce(&mut Self) -> Result<Value, String>,
        lose_answer: bool,
    ) -> Result<Value, String> {
        if let Some(entry) = self.store.state.journal.get(step) {
            if let Some(answer) = &entry.answer {
                return Ok(answer.clone());
            }
            eprintln!("step {step}: answer unknown; sending the recorded request again");
        } else {
            let body = body(self)?;
            self.store.state.journal.insert(
                step.to_owned(),
                JournalEntry {
                    route: route.to_owned(),
                    body,
                    answer: None,
                },
            );
            self.store.save()?;
        }
        let entry = self.store.state.journal[step].clone();
        match self.api.post(&entry.route, &entry.body) {
            Ok(answer) if lose_answer => Err(format!(
                "step {step}: sent, and its answer (receipt {}) discarded as if lost; \
                 run the same command or retry",
                answer["receipt"]["payload"]["receipt_id"]
            )),
            Ok(answer) => {
                if let Some(entry) = self.store.state.journal.get_mut(step) {
                    entry.answer = Some(answer.clone());
                }
                self.store.save()?;
                Ok(answer)
            }
            // Even a 4xx can follow a previous successful commit (for example, a key
            // was revoked before the retry). Only the operation's returned outcome
            // resolves this journal entry; transport errors never allocate a new ID.
            Err(failure) => Err(format!(
                "step {step}: {failure}; its outcome is unknown, run the same command or retry"
            )),
        }
    }
    /// A relationship command's answer, or the kernel's refusal as an error.
    fn accepted(step: &str, answer: &Value) -> Result<(), String> {
        if let Some(refusal) = answer["outcome"].get("Refused") {
            return Err(format!("step {step}: refused: {refusal}"));
        }
        Ok(())
    }
    fn read(&self, read: MemberRead) -> Result<MemberAnswer, String> {
        let deployment = self.deployment()?;
        let ids = self.identifiers()?;
        let signed = SignedMemberRead::sign(
            deployment.scope,
            ids.persona,
            ids.key,
            now(),
            read,
            &self.state().device_secret,
        )
        .map_err(|e| format!("cannot sign read: {e}"))?;
        decode(
            self.api
                .post("v1/members/reads", &signed)
                .map_err(|e| e.to_string())?,
            "read answer",
        )
    }
    fn sending(
        &self,
        to: u128,
    ) -> Result<
        (
            cs_mail_application::members::RelationshipHandle,
            Option<RelationshipView>,
        ),
        String,
    > {
        match self.read(MemberRead::AsSender {
            recipient: ProtocolIdentity(to),
        })? {
            MemberAnswer::Sending { handle, view } => Ok((handle, view)),
            _ => Err("unexpected read answer".into()),
        }
    }
    fn receiving(&self, from: u128) -> Result<MemberAnswer, String> {
        self.read(MemberRead::AsRecipient {
            sender: ProtocolIdentity(from),
        })
    }

    pub fn publish_directory(
        &mut self,
        name: String,
        description: String,
        mailbox: Option<String>,
        unlisted: bool,
    ) -> Result<(), String> {
        use cs_mail_accounts::directory::{
            DirectoryRevision, DiscoveryVisibility, MemberAddress, PublicProfile,
        };
        use cs_mail_application::directory::{Command, Response, SignedRequest};
        let deployment = self.deployment()?;
        let address = self.persona()?;
        let key = self.recipient_key()?;
        let inspect = SignedRequest {
            scope: deployment.scope,
            address,
            key,
            operation: IdempotencyKey(identifier()?),
            command: Command::Inspect { at: now() },
            signature: Vec::new(),
        }
        .sign(&self.state().recipient_secret)
        .map_err(|e| e.to_string())?;
        let response: Response = decode(
            self.api
                .post("v1/directory/commands", &inspect)
                .map_err(|e| e.to_string())?,
            "directory settings",
        )?;
        let Response::Settings(settings) = response else {
            return Err("unexpected directory settings".into());
        };
        let expected = settings.entry.as_ref().map_or(
            DirectoryRevision(0),
            cs_mail_accounts::directory::DirectoryEntry::revision,
        );
        let classes = settings.classes.ok_or("publish request classes first")?;
        let signed_classes = self
            .recipient_signer()?
            .sign_request_classes(deployment.scope, classes)
            .map_err(|e| e.to_string())?;
        let mailbox = mailbox
            .map(MemberAddress::try_from)
            .transpose()
            .map_err(|e| format!("mailbox: {e:?}"))?;
        let profile =
            PublicProfile::new(name, description, None).map_err(|e| format!("profile: {e:?}"))?;
        let step = format!("directory:{}", expected.0);
        let secret = self.state().recipient_secret;
        let answer = self.journaled(
            &step,
            "v1/directory/commands",
            |_| {
                let request = SignedRequest {
                    scope: deployment.scope,
                    address,
                    key,
                    operation: IdempotencyKey(identifier()?),
                    command: Command::Publish {
                        expected,
                        profile,
                        mailbox,
                        visibility: if unlisted {
                            DiscoveryVisibility::Unlisted
                        } else {
                            DiscoveryVisibility::Listed
                        },
                        classes: Box::new(signed_classes),
                    },
                    signature: Vec::new(),
                }
                .sign(&secret)
                .map_err(|e| e.to_string())?;
                serde_json::to_value(request).map_err(|e| e.to_string())
            },
            false,
        )?;
        match decode::<Response>(answer, "directory receipt")? {
            Response::Published(_) => {
                println!("directory publication recorded");
                Ok(())
            }
            _ => Err("directory publication refused".into()),
        }
    }
    pub fn enroll_begin(&mut self, bank: &Path) -> Result<(), String> {
        let operation = if let Some(operation) = &self.store.state.enrollment {
            operation.clone()
        } else {
            let operation = format!("{:032x}", identifier()?);
            self.store.state.enrollment = Some(operation.clone());
            self.store.save()?;
            operation
        };
        let answer = self
            .api
            .post(
                "v1/enrollments",
                &json!({
                    "operation": operation,
                    "initial_key": public(&self.state().device_secret),
                    "bank": read_json(bank, "bank request")?,
                }),
            )
            .map_err(|e| e.to_string())?;
        self.store
            .state
            .values
            .insert("enrollment-intent".into(), answer["intent"].clone());
        self.store.save()?;
        print(&json!({
            "operation": operation,
            "persona": self.persona()?.0.to_string(),
            "challenge": answer["intent"]["challenge"],
            "expires_at": answer["intent"]["expires_at"],
        }));
        Ok(())
    }
    pub fn enroll_complete(
        &mut self,
        oidc_token: &str,
        login_issuer: &str,
        login_subject: &str,
        applicant: &Path,
        bank: &Path,
    ) -> Result<(), String> {
        let operation = self
            .state()
            .enrollment
            .clone()
            .ok_or("run enroll-begin first")?;
        let intent: identity_contract::EnrollmentIntent = decode(
            self.store
                .state
                .values
                .get("enrollment-intent")
                .cloned()
                .ok_or("run enroll-begin first")?,
            "enrollment intent",
        )?;
        let device_proof = intent
            .prove_possession(&self.state().device_secret)
            .map_err(|e| format!("cannot prove device possession: {e:?}"))?;
        let answer = self
            .api
            .post(
                &format!("v1/enrollments/{operation}/complete"),
                &json!({
                    "oidc_token": oidc_token,
                    "device_proof": device_proof,
                    "login_issuer": login_issuer,
                    "login_subject": login_subject,
                    "bank": read_json(bank, "bank request")?,
                    "applicant": read_json(applicant, "applicant statement")?,
                }),
            )
            .map_err(|e| e.to_string())?;
        self.record_enrollment(&answer)
    }
    pub fn enroll_status(&mut self) -> Result<(), String> {
        let operation = self
            .state()
            .enrollment
            .clone()
            .ok_or("run enroll-begin first")?;
        let answer = self
            .api
            .get(&format!("v1/enrollments/{operation}"))
            .map_err(|e| e.to_string())?;
        self.record_enrollment(&answer)
    }
    fn record_enrollment(&mut self, answer: &Value) -> Result<(), String> {
        if answer["outcome"] == "enrolled" {
            let account: u128 = answer["account"]
                .as_str()
                .and_then(|a| a.parse().ok())
                .ok_or("enrolled without an account identifier")?;
            self.store.state.account = Some(account);
            self.store.save()?;
        }
        print(&json!({
            "outcome": answer["outcome"],
            "account": answer["account"],
            "persona": self.persona()?.0.to_string(),
        }));
        Ok(())
    }

    fn account_view(&self) -> Result<AccountView, String> {
        match self.read(MemberRead::Account)? {
            MemberAnswer::Account(view) => Ok(view),
            _ => Err("unexpected read answer".into()),
        }
    }
    pub fn account(&self) -> Result<(), String> {
        render::account(&self.account_view()?);
        Ok(())
    }
    pub fn whoami(&self) -> Result<(), String> {
        let (_, content) = self.content_key()?;
        print(&json!({
            "persona": self.persona().ok().map(|p| p.0.to_string()),
            "account": self.state().account.map(|a| a.to_string()),
            "device_key": public(&self.state().device_secret),
            "recipient_key": public(&self.state().recipient_secret),
            "content_key": content,
        }));
        Ok(())
    }
    pub fn register_recipient_key(&mut self) -> Result<(), String> {
        let deployment = self.deployment()?;
        let account = cs_mail_primitives::AccountId(self.account_id()?);
        let view = self.account_view()?;
        let reference = self.recipient_key()?;
        let persona = self.persona()?;
        let ids = self.identifiers()?;
        let step = format!("account:register-recipient-key:{}", reference.0);
        let answer = self.journaled(
            &step,
            "v1/accounts/commands",
            |m| {
                let key = cs_mail_accounts::control::NewOperationalKey::prove(
                    account,
                    &deployment.product,
                    reference,
                    ActorRef::Recipient(persona),
                    &m.state().recipient_secret,
                )
                .map_err(|e| format!("cannot prove key possession: {e:?}"))?;
                let signed = cs_mail_accounts::control::SignedAccountCommand {
                    account,
                    operational_key: ids.key,
                    expected_revision: view.revision,
                    idempotency_key: IdempotencyKey(identifier()?),
                    product: deployment.product.clone(),
                    command: cs_mail_accounts::control::AccountCommand::RegisterKey(key),
                    signature: Vec::new(),
                }
                .sign(&m.state().device_secret)
                .map_err(|e| format!("cannot sign: {e:?}"))?;
                serde_json::to_value(signed).map_err(|e| e.to_string())
            },
            false,
        )?;
        print(&json!({ "recipient_key": reference.0.to_string(), "account": answer }));
        Ok(())
    }
    pub fn publish_content_key(&mut self) -> Result<(), String> {
        let deployment = self.deployment()?;
        let (_, key) = self.content_key()?;
        let ids = self.identifiers()?;
        let step = format!("endpoint-key:{}", key.reference.0);
        self.journaled(
            &step,
            "v1/endpoint-keys",
            |m| {
                let signed = SignedEndpointKey::sign(
                    deployment.scope,
                    ids.key,
                    EndpointKeyPublication {
                        persona: ids.persona,
                        version: 1,
                        key,
                    },
                    &m.state().device_secret,
                )
                .map_err(|e| format!("cannot sign: {e}"))?;
                serde_json::to_value(signed).map_err(|e| e.to_string())
            },
            false,
        )?;
        print(&json!({ "published": key }));
        Ok(())
    }
    pub fn purchase(&mut self, offer: u64) -> Result<(), String> {
        let view = self.account_view()?;
        let ids = self.identifiers()?;
        let step = format!("billing:purchase:{offer}");
        let answer = self.journaled(
            &step,
            "v1/billing/commands",
            |m| {
                let signed = cs_mail_billing::SignedBillingCommand::sign(
                    view.billing,
                    ids.key,
                    view.financial_scope,
                    view.billing_revision,
                    IdempotencyKey(identifier()?),
                    cs_mail_billing::BillingCommand::PurchaseService {
                        offer: cs_mail_primitives::PolicyVersion(offer),
                    },
                    &m.state().device_secret,
                )
                .map_err(|e| format!("cannot sign: {e:?}"))?;
                serde_json::to_value(signed).map_err(|e| e.to_string())
            },
            false,
        )?;
        print(&answer);
        Ok(())
    }
    pub fn publish_classes(&mut self, version: u64, classes: &[String]) -> Result<(), String> {
        let deployment = self.deployment()?;
        let persona = self.persona()?;
        let classes = classes
            .iter()
            .map(|spec| {
                let mut parts = spec.splitn(3, '=');
                let (Some(id), Some(description), Some(collateral)) =
                    (parts.next(), parts.next(), parts.next())
                else {
                    return Err(format!("class {spec:?} is not ID=DESCRIPTION=COLLATERAL"));
                };
                cs_mail_protocol::pricing::RequestClass::new(
                    RequestClassId(id.parse().map_err(|_| format!("bad class id {id:?}"))?),
                    description.to_owned(),
                    cs_mail_primitives::Money::from_minor_units(
                        collateral
                            .parse()
                            .map_err(|_| format!("bad collateral {collateral:?}"))?,
                    ),
                )
                .map_err(|e| format!("invalid class {spec:?}: {e:?}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let classes =
            cs_mail_protocol::pricing::RecipientRequestClasses::new(persona, version, classes)
                .map_err(|e| format!("invalid classes: {e:?}"))?;
        let signed = self
            .recipient_signer()?
            .sign_request_classes(deployment.scope, classes)
            .map_err(|e| format!("cannot sign (is the recipient key registered?): {e}"))?;
        let step = format!("classes:{version}");
        let answer = self.journaled(
            &step,
            "v1/request-classes",
            |_| serde_json::to_value(signed).map_err(|e| e.to_string()),
            false,
        )?;
        print(&answer);
        Ok(())
    }
    pub fn offer(&self, to: u128) -> Result<(), String> {
        print(
            &self
                .api
                .get(&format!("v1/addresses/{to}/offer"))
                .map_err(|e| e.to_string())?,
        );
        Ok(())
    }

    fn declarations(&self) -> Result<MessageDeclarations, String> {
        Ok(MessageDeclarations {
            purpose: DeclaredPurpose::Known(KnownPurpose::Personal),
            origin: OriginDeclaration {
                mode: OriginMode::HumanInitiated,
                authority: DeclarationAuthority::NativeSender(self.identifiers()?.key),
            },
            payload_schema: None,
        })
    }
    /// Signs one relationship command as sender or recipient.
    fn command(
        signer: &CommandSigner,
        deployment: &DeploymentInfo,
        handle: &cs_mail_application::members::RelationshipHandle,
        command: ProtocolCommand,
    ) -> Result<Value, String> {
        let bytes = signer
            .sign(
                SigningScope {
                    deployment_domain: deployment.scope.deployment_domain,
                    intended_provider: deployment.scope.provider,
                    relationship: handle.relationship,
                },
                deployment.policy.protocol_version,
                IdempotencyKey(identifier()?),
                command,
            )
            .map_err(|e| format!("cannot sign: {e}"))?;
        serde_json::to_value(bytes).map_err(|e| e.to_string())
    }
    /// Journals a native request document before staging it with the service.
    fn upload(
        &mut self,
        step: &str,
        to: u128,
        ids: RequestMessageIds,
        message: &str,
    ) -> Result<MessageValidityUntil, String> {
        let route = format!("v1/relationships/{}/{to}/content", self.persona()?.0);
        self.journaled(
            step,
            &route,
            |member| {
                serde_json::to_value(member.prepare_request_document(to, &ids, message)?)
                    .map_err(|e| e.to_string())
            },
            false,
        )?;
        let packet: cs_mail_application::requests::documents::SignedRequestDocument = decode(
            self.store.state.journal[step].body.clone(),
            "recorded document",
        )?;
        Ok(packet
            .document
            .package()
            .recipient_copy
            .binding
            .message_valid_until)
    }
    fn prepare_request_document(
        &self,
        to: u128,
        ids: &RequestMessageIds,
        message: &str,
    ) -> Result<cs_mail_application::requests::documents::SignedRequestDocument, String> {
        let deployment = self.deployment()?;
        let (handle, _) = self.sending(to)?;
        let created = now();
        let valid_until = MessageValidityUntil(
            created
                .checked_add(CONTENT_LIFETIME)
                .ok_or("clock overflow")?,
        );
        let persona = self.persona()?;
        let signer = self.sender_signer()?;
        let declarations = self.declarations()?;
        let recipient_key: EndpointKeyPublication = decode(
            self.api
                .get(&format!("v1/addresses/{to}/endpoint-key"))
                .map_err(|e| format!("recipient content key: {e}"))?,
            "endpoint key",
        )?;
        let (secret, public) = self.content_key()?;
        let certificate = signer
            .sign_content_key_certificate(ContentKeyCertificate {
                wire_version: WireVersion(1),
                protocol_version: deployment.policy.protocol_version,
                deployment_domain: deployment.scope.deployment_domain,
                intended_provider: deployment.scope.provider,
                relationship: handle.relationship,
                owner: persona,
                operational_key: signer.reference(),
                content_key_version: ContentKeyVersion(1),
                key: public,
                valid_from: created,
                valid_until: valid_until.0,
            })
            .map_err(|e| format!("cannot certify content key: {e}"))?;
        let document = cs_mail_client::correspondence::compose(vec![
            cs_mail_correspondence::DocumentPart::Text(message.to_owned()),
        ])
        .map_err(|e| e.to_string())?;
        let document = document.with_subject(self.native_subject(ids.conversation)?);
        let package = cs_mail_client::correspondence::prepare(
            &cs_mail_client::correspondence::EndpointKeys {
                owner: persona,
                secret: &secret,
                public,
            },
            &document,
            ids.conversation,
            recipient_key.key,
            deployment.custody_key,
            ContentBinding {
                wire_version: WireVersion(1),
                content_ref: ids.content,
                message_id: ids.message,
                sender: persona,
                recipient: ProtocolIdentity(to),
                protocol_version: deployment.policy.protocol_version,
                relationship: handle.relationship,
                content_scope: handle.content_scope,
                sender_certificate: content_key_certificate_digest(&certificate.certificate),
                declarations,
                message_valid_until: valid_until,
                capability: None,
            },
            (created, valid_until.0),
        )
        .map_err(|e| e.to_string())?;
        let document = cs_mail_application::requests::documents::RequestDocument::new(
            ids.request,
            ids.content,
            package,
            self.native_receiving_alias(ProtocolIdentity(to))?,
        )
        .map_err(|e| e.to_string())?;
        let packet = cs_mail_application::requests::documents::SignedRequestDocument {
            scope: SigningScope {
                deployment_domain: deployment.scope.deployment_domain,
                intended_provider: deployment.scope.provider,
                relationship: handle.relationship,
            },
            key: signer.reference(),
            document,
            certificate,
            signature: Vec::new(),
        }
        .sign(&self.state().device_secret)
        .map_err(|e| e.to_string())?;
        Ok(packet)
    }

    #[allow(clippy::too_many_lines)] // One request: terms, charge, funding, upload, submission.
    pub fn request(
        &mut self,
        to: u128,
        class: u64,
        message: &str,
        lose_answer_at: Option<&str>,
        prepare_only: bool,
    ) -> Result<(), String> {
        let deployment = self.deployment()?;
        let persona = self.persona()?;
        let key = format!("request:{to}");
        let plan = if let Some(plan) = self.store.state.values.get(&key) {
            eprintln!("resuming the unfinished request to {to}");
            plan.clone()
        } else {
            let plan = json!({
                "class": class,
                "quote": identifier()?.to_string(),
                "request": identifier()?.to_string(),
                "message": identifier()?.to_string(),
                "content": identifier()?.to_string(),
                "delivery": identifier()?.to_string(),
                "conversation":identifier()?.to_string(),
            });
            self.store.state.values.insert(key.clone(), plan.clone());
            self.store.save()?;
            plan
        };
        let id = |name: &str| -> Result<u128, String> {
            plan[name]
                .as_str()
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| format!("request plan lacks {name}"))
        };
        let class = RequestClassId(u128::from(
            plan["class"].as_u64().ok_or("request plan lacks class")?,
        ));
        let request_id = RequestId(id("request")?);
        let message_id = MessageId(id("message")?);
        let content_ref = ContentRef(id("content")?);
        let route = format!("v1/relationships/{}/{to}/commands", persona.0);
        let signer = self.sender_signer()?;
        let digest = message_declaration_digest(&self.declarations()?)
            .map_err(|e| format!("declarations: {e:?}"))?;
        let (handle, _) = self.sending(to)?;
        let lose = |step: &str| lose_answer_at == Some(step);

        // 1. Terms: the class, C, S, deadlines and refunds, signed by CSQD.
        let step = format!("request:{}:issue", request_id.0);
        let quote = QuoteId(id("quote")?);
        let answer = self.journaled(
            &step,
            &route,
            |_| {
                Self::command(
                    &signer,
                    &deployment,
                    &handle,
                    ProtocolCommand::IssueRequestTerms {
                        class_id: class,
                        quote_id: quote,
                        declaration_digest: Some(digest),
                    },
                )
            },
            lose("issue"),
        )?;
        if let Err(error) = Self::accepted(&step, &answer) {
            // An authenticated terms refusal created no request or charge. Keep its
            // journal evidence, but permit a fresh approach after prerequisites change.
            self.store.state.values.remove(&key);
            self.store.save()?;
            return Err(error);
        }
        let terms: SignedContactTerms = decode(answer["signed_terms"].clone(), "signed terms")?;
        render::terms(&terms.terms);

        // 2. One conditional charge of C + S, drawn from the verified bank account.
        let payment_method = self.account_view()?.payment_method;
        let step = format!("request:{}:create", request_id.0);
        let answer = self.journaled(
            &step,
            &route,
            |_| {
                Self::command(
                    &signer,
                    &deployment,
                    &handle,
                    ProtocolCommand::CreateRequest {
                        payment_method,
                        request_id,
                        message_id,
                        terms: Box::new(terms.terms.clone()),
                    },
                )
            },
            lose("create"),
        )?;
        Self::accepted(&step, &answer)?;

        // 3. Submission waits until the charge's funding is final.
        let request = self.wait_for_funding(to, request_id)?;
        if prepare_only {
            println!("request {} is funded and not submitted", request_id.0);
            return self.status(to, false);
        }

        // 4. The initial message, encrypted to the recipient before upload.
        let valid_until = self.upload(
            &format!("request:{}:upload", request_id.0),
            to,
            RequestMessageIds {
                message: message_id,
                content: content_ref,
                request: request_id,
                conversation: cs_mail_correspondence::ConversationId::new(id("conversation")?)
                    .map_err(|e| e.to_string())?,
            },
            message,
        )?;

        // 5. Submission starts the recipient's decision window.
        let step = format!("request:{}:submit", request_id.0);
        let delivery = DeliveryIntentRef(id("delivery")?);
        let answer = self.journaled(
            &step,
            &route,
            |_| {
                Self::command(
                    &signer,
                    &deployment,
                    &handle,
                    ProtocolCommand::SubmitRequestToRecipient {
                        request_id,
                        expected_request_version: request.version.into(),
                        content_ref,
                        delivery_intent_ref: delivery,
                        declaration_digest: digest,
                        message_valid_until: valid_until,
                    },
                )
            },
            lose("submit"),
        )?;
        Self::accepted(&step, &answer)?;
        self.store.state.values.remove(&key);
        self.store.save()?;
        println!("submitted request {} to {to}", request_id.0);
        self.status(to, false)
    }
    fn wait_for_funding(&self, to: u128, id: RequestId) -> Result<RequestView, String> {
        let started = std::time::Instant::now();
        loop {
            let (_, view) = self.sending(to)?;
            let request = view
                .and_then(|v| v.requests.into_iter().find(|r| r.id == id))
                .ok_or("the request is not recorded")?;
            match request.money.map(|m| m.capture) {
                Some(cs_mail_finance::CaptureStatus::FundingFinalized) => {
                    println!("charge funded: final");
                    return Ok(request);
                }
                Some(
                    cs_mail_finance::CaptureStatus::Voided
                    | cs_mail_finance::CaptureStatus::Reversed,
                ) => return Err("the charge was not funded".into()),
                _ if !matches!(request.lifecycle, RequestLifecycle::PreparingSubmission(_)) => {
                    return Err("the request is no longer preparing its submission".into());
                }
                _ => {}
            }
            if started.elapsed() > FUNDING_WAIT {
                return Err(
                    "funding is not final yet; run the same command again to resume".into(),
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    fn current_request(view: Option<&RelationshipView>) -> Result<RequestView, String> {
        view.and_then(|v| {
            v.requests
                .iter()
                .find(|r| !r.lifecycle.is_terminal())
                .cloned()
        })
        .ok_or_else(|| "there is no unresolved request".into())
    }
    pub fn followup(&mut self, to: u128, message: &str) -> Result<(), String> {
        let deployment = self.deployment()?;
        let persona = self.persona()?;
        let (handle, view) = self.sending(to)?;
        let request = Self::current_request(view.as_ref())?;
        let policy_version = view.map(|v| v.followup_policy.version).unwrap_or_default();
        let message_id = MessageId(identifier()?);
        let content_ref = ContentRef(identifier()?);
        let valid_until = self.upload(
            &format!("followup:{}:upload", message_id.0),
            to,
            RequestMessageIds {
                message: message_id,
                content: content_ref,
                request: request.id,
                conversation: {
                    let original: cs_mail_application::requests::documents::SignedRequestDocument =
                        decode(
                            self.store
                                .state
                                .journal
                                .get(&format!("request:{}:upload", request.id.0))
                                .ok_or("original request journal unavailable")?
                                .body
                                .clone(),
                            "original request document",
                        )?;
                    original.document.package().manifest.conversation()
                },
            },
            message,
        )?;
        let signer = self.sender_signer()?;
        let digest = message_declaration_digest(&self.declarations()?)
            .map_err(|e| format!("declarations: {e:?}"))?;
        let step = format!("followup:{}:admit", message_id.0);
        let answer = self.journaled(
            &step,
            &format!("v1/relationships/{}/{to}/commands", persona.0),
            |_| {
                Self::command(
                    &signer,
                    &deployment,
                    &handle,
                    ProtocolCommand::AdmitFollowup {
                        request_id: request.id,
                        message_id,
                        content_ref,
                        delivery_intent_ref: DeliveryIntentRef(identifier()?),
                        declaration_digest: digest,
                        message_valid_until: valid_until,
                        expected_policy_version: policy_version,
                    },
                )
            },
            false,
        )?;
        Self::accepted(&step, &answer)?;
        println!("follow-up admitted under request {}", request.id.0);
        Ok(())
    }
    pub fn cancel(&mut self, to: u128) -> Result<(), String> {
        let deployment = self.deployment()?;
        let persona = self.persona()?;
        let (handle, view) = self.sending(to)?;
        let request = Self::current_request(view.as_ref())?;
        let signer = self.sender_signer()?;
        let step = format!("request:{}:cancel", request.id.0);
        let answer = self.journaled(
            &step,
            &format!("v1/relationships/{}/{to}/commands", persona.0),
            |_| {
                Self::command(
                    &signer,
                    &deployment,
                    &handle,
                    ProtocolCommand::CancelRequestSubmission {
                        request_id: request.id,
                        expected_request_version: request.version.into(),
                        reason: CancellationReason::SenderRequested,
                    },
                )
            },
            false,
        )?;
        Self::accepted(&step, &answer)?;
        self.store.state.values.remove(&format!("request:{to}"));
        self.store.save()?;
        println!("cancelled request {}", request.id.0);
        self.status(to, false)
    }

    fn recipient_view(
        &self,
        from: u128,
    ) -> Result<
        (
            cs_mail_application::members::RelationshipHandle,
            RelationshipView,
        ),
        String,
    > {
        match self.receiving(from)? {
            MemberAnswer::Receiving {
                handle,
                view: Some(view),
                ..
            } => Ok((handle, view)),
            MemberAnswer::Receiving { view: None, .. } => {
                Err(format!("{from} has not written to this address"))
            }
            _ => Err("unexpected read answer".into()),
        }
    }
    pub fn permit_followups(
        &mut self,
        from: u128,
        max_messages: u32,
        max_per_interval: u32,
        interval_ms: u64,
    ) -> Result<(), String> {
        let deployment = self.deployment()?;
        let persona = self.persona()?;
        let (handle, view) = self.recipient_view(from)?;
        let signer = self.recipient_signer()?;
        let step = format!(
            "followup-policy:{from}:{}",
            view.followup_policy.version.0 + 1
        );
        let answer = self.journaled(
            &step,
            &format!("v1/relationships/{from}/{}/commands", persona.0),
            |_| {
                Self::command(
                    &signer,
                    &deployment,
                    &handle,
                    ProtocolCommand::SetFollowupPolicy {
                        expected_version: view.followup_policy.version,
                        policy: FollowupPolicy {
                            version: view.followup_policy.version,
                            max_messages,
                            max_per_interval,
                            interval: Duration(interval_ms),
                        },
                    },
                )
            },
            false,
        )?;
        Self::accepted(&step, &answer)?;
        println!("follow-ups from {from}: up to {max_messages}");
        Ok(())
    }
    pub fn decide(&mut self, from: u128, decision: Decision) -> Result<(), String> {
        let deployment = self.deployment()?;
        let persona = self.persona()?;
        let (handle, view) = self.recipient_view(from)?;
        let expected: Version = view.version.into();
        let signer = self.recipient_signer()?;
        let route = format!("v1/relationships/{from}/{}/commands", persona.0);
        let (name, command) = match decision {
            Decision::Accept => (
                "accept",
                ProtocolCommand::AcceptRelationship {
                    expected_version: expected,
                },
            ),
            Decision::Revoke => (
                "revoke",
                ProtocolCommand::RevokeRelationship {
                    expected_version: expected,
                },
            ),
            Decision::Reject => (
                "reject",
                ProtocolCommand::RejectRelationship {
                    expected_version: expected,
                },
            ),
            Decision::Block => (
                "block",
                ProtocolCommand::BlockRelationship {
                    expected_version: expected,
                },
            ),
            Decision::Unblock => (
                "unblock",
                ProtocolCommand::UnblockRelationship {
                    expected_version: expected,
                },
            ),
            Decision::Lane(lifetime) => {
                return self.grant_lane(from, handle, &view, lifetime, false);
            }
        };
        let step = format!("decision:{from}:{name}:{}", view.version.0);
        let answer = self.journaled(
            &step,
            &route,
            |_| Self::command(&signer, &deployment, &handle, command),
            false,
        )?;
        Self::accepted(&step, &answer)?;
        println!("decision recorded: {name}");
        self.status(from, true)
    }
    fn grant_lane(
        &mut self,
        from: u128,
        handle: cs_mail_application::members::RelationshipHandle,
        view: &RelationshipView,
        lifetime: u64,
        guest: bool,
    ) -> Result<(), String> {
        let deployment = self.deployment()?;
        let persona = self.persona()?;
        let reference = self.recipient_key()?;
        let secret = self.state().recipient_secret;
        let version = view
            .lane
            .as_ref()
            .map_or(Version(1), |lane| Version(lane.grant().version.0 + 1));
        let step = format!("decision:{from}:lane:{}", version.0);
        let issued = now();
        let answer = self.journaled(
            &step,
            "v1/lanes",
            |_| {
                let grant = SenderScopedGrant {
                    id: LaneId(identifier()?),
                    // The lane is bound to this relationship's own reference.
                    subject: if guest {
                        LaneSubject::GuestMailbox(ProtocolIdentity(from))
                    } else {
                        LaneSubject::Native(cs_mail_privacy::ScopedHandle(
                            *handle.relationship.as_bytes(),
                        ))
                    },
                    sender: ProtocolIdentity(from),
                    recipient: persona,
                    protocol_version: deployment.policy.protocol_version,
                    deployment_domain: deployment.scope.deployment_domain,
                    intended_provider: deployment.scope.provider,
                    recipient_operational_key: reference,
                    purpose: if guest {
                        DeclaredPurpose::Unspecified
                    } else {
                        DeclaredPurpose::Known(KnownPurpose::Personal)
                    },
                    origin: if guest {
                        None
                    } else {
                        Some(OriginMode::HumanInitiated)
                    },
                    declaration_authority: if guest {
                        DeclarationAuthorityConstraint::GuestGateway
                    } else {
                        DeclarationAuthorityConstraint::NativeSender
                    },
                    lifetime: Duration(lifetime),
                    rate_limit: RateLimit {
                        max_messages: 20,
                        interval: Duration(24 * 60 * 60 * 1000),
                    },
                    mode: LaneMode::Expiring,
                    issued_at: issued,
                    not_before: issued,
                    not_after: issued
                        .checked_add(Duration(lifetime))
                        .ok_or("lane lifetime overflows")?,
                    version,
                };
                let signature = ed25519_dalek::Signer::sign(
                    &ed25519_dalek::SigningKey::from_bytes(&secret),
                    &grant
                        .signing_bytes()
                        .map_err(|e| format!("invalid lane grant: {e:?}"))?,
                )
                .to_bytes();
                Ok(json!({
                    "grant": SignedLaneGrant { grant, signature },
                    "idempotency_key": identifier()?,
                }))
            },
            false,
        )?;
        Self::accepted(&step, &answer)?;
        println!("decision recorded: accept with an express lane");
        self.status(from, true)
    }

    pub fn inbox(&self) -> Result<(), String> {
        let MemberAnswer::Inbox(views) = self.read(MemberRead::Inbox)? else {
            return Err("unexpected read answer".into());
        };
        if views.is_empty() {
            println!("no one has written to this address");
        }
        for view in views {
            let from = view.handle.sender.0;
            println!("from {}: {:?}", render::sender(&view), view.state);
            self.show_received(from)?;
        }
        Ok(())
    }
    fn show_received(&self, from: u128) -> Result<(), String> {
        let MemberAnswer::Receiving { view, content, .. } = self.receiving(from)? else {
            return Err("unexpected read answer".into());
        };
        if view.as_ref().is_some_and(|view| view.guest.is_none()) {
            return self.native_read(from);
        }
        let (secret, _) = self.content_key()?;
        for delivered in content {
            let text = match &delivered.certificate {
                Some(certificate)
                    if content_key_certificate_digest(&certificate.certificate)
                        == delivered.record.binding.sender_certificate =>
                {
                    cs_mail_content::decrypt(
                        &secret,
                        certificate.certificate.key,
                        &delivered.record,
                    )
                    .map_or_else(
                        |e| format!("(cannot decrypt: {e:?})"),
                        |bytes| String::from_utf8_lossy(&bytes).into_owned(),
                    )
                }
                _ => "(sender certificate unavailable)".into(),
            };
            println!(
                "  message {}: {text}",
                delivered.record.binding.message_id.0
            );
        }
        if let Some(view) = view {
            render::relationship(&view, true);
        }
        Ok(())
    }
    pub fn status(&self, with: u128, as_recipient: bool) -> Result<(), String> {
        if as_recipient {
            return self.show_received(with);
        }
        match self.sending(with)? {
            (_, Some(view)) => render::relationship(&view, false),
            (_, None) => println!("no relationship with {with} yet"),
        }
        Ok(())
    }
    pub fn retry(&mut self) -> Result<(), String> {
        let pending: Vec<String> = self
            .store
            .state
            .journal
            .iter()
            .filter(|(_, e)| e.answer.is_none())
            .map(|(k, _)| k.clone())
            .collect();
        if pending.is_empty() {
            println!("nothing to retry");
        }
        for step in pending {
            let answer = self.journaled(&step, "", |_| Err("not journaled".into()), false)?;
            println!(
                "step {step}: answered; receipt {}",
                answer["receipt"]["payload"]["receipt_id"]
            );
        }
        Ok(())
    }
}
