//! Synthetic external text and receiver evidence for controlled local experiments.
use cs_mail_application::correspondence::external::PreparedText;
use cs_mail_content::{ContentBinding, ContentCertificateDigest, EndpointSecretKey};
use cs_mail_correspondence::{ConversationId, Document, DocumentPart};
use cs_mail_guests::GuestMailbox;
use cs_mail_primitives::*;
use cs_mail_smtp_gateway::incoming::{AuthenticatedReply, ReplyEncryption};
use cs_mail_storage_postgres::external::Preparation;

pub fn text(
    preparation: &Preparation,
    conversation: ConversationId,
    message: MessageId,
    member: ProtocolIdentity,
    now: CanonicalTime,
) -> PreparedText {
    let (secret, sender_key) =
        EndpointSecretKey::from_secret_bytes(ContentKeyRef(500), &[50; 32]).unwrap();
    let document = Document::new(
        vec![DocumentPart::Text(format!(
            "external fixture {}",
            message.0
        ))],
        [51; 32],
    )
    .unwrap();
    let manifest = document.manifest(message, conversation).unwrap();
    let expires = now.checked_add(Duration(600_000)).unwrap();
    let binding = ContentBinding {
        wire_version: WireVersion(1),
        protocol_version: ProtocolVersion(2),
        content_ref: ContentRef(message.0),
        message_id: message,
        sender: member,
        recipient: member,
        relationship: preparation.relationship.relationship,
        content_scope: preparation.relationship.content_scope,
        sender_certificate: ContentCertificateDigest([0; 32]),
        message_valid_until: MessageValidityUntil(expires),
        capability: None,
        declarations: MessageDeclarations {
            purpose: DeclaredPurpose::Known(KnownPurpose::Personal),
            origin: OriginDeclaration {
                mode: OriginMode::HumanInitiated,
                authority: DeclarationAuthority::NativeSender(OperationalKeyRef(21)),
            },
            payload_schema: None,
        },
    };
    let bytes = serde_json::to_vec(&document).unwrap();
    PreparedText {
        manifest,
        sender_key,
        member_copy: cs_mail_content::encrypt(
            &secret,
            preparation.member_key,
            binding.clone(),
            &bytes,
            now,
            expires,
        )
        .unwrap(),
        relay_copy: cs_mail_content::encrypt(
            &secret,
            preparation.relay_key,
            binding,
            &bytes,
            now,
            expires,
        )
        .unwrap(),
    }
}
/// Synthetic authentication is intentionally confined to development support.
/// Parsing, binding and encryption still use the production receiver path.
pub fn reply(
    preparation: &Preparation,
    scope: cs_mail_security::RecipientSigningScope,
    conversation: ConversationId,
    id: MessageId,
    mailbox: GuestMailbox,
    now: CanonicalTime,
    lane: Option<LaneId>,
) -> AuthenticatedReply {
    reply_text(
        preparation,
        scope,
        conversation,
        id,
        mailbox,
        now,
        lane,
        &format!("external fixture {}", id.0),
    )
}
struct SyntheticVerifier(cs_mail_adapters::DomainIdentity);
impl cs_mail_adapters::DmarcVerifier for SyntheticVerifier {
    fn verify<'a>(
        &'a self,
        request: cs_mail_adapters::SmtpAuthenticationRequest<'a>,
    ) -> cs_mail_adapters::DmarcVerificationFuture<'a> {
        Box::pin(async move {
            Ok(cs_mail_adapters::VerifiedDomain::passed(
                self.0.clone(),
                Some(cs_mail_adapters::DmarcPass {
                    alignment: cs_mail_adapters::DmarcAlignment::Strict,
                }),
                None,
                request.received_at,
            )
            .unwrap())
        })
    }
}
/// Produces altered-text retries through authentication, not by mutating a proof.
#[allow(clippy::too_many_arguments)] // Explicit fixture inputs, including trusted deployment scope.
pub fn reply_text(
    preparation: &Preparation,
    scope: cs_mail_security::RecipientSigningScope,
    conversation: ConversationId,
    id: MessageId,
    mailbox: GuestMailbox,
    now: CanonicalTime,
    lane: Option<LaneId>,
    body: &str,
) -> AuthenticatedReply {
    let raw = format!(
        "From: {}\r\nMessage-ID: <fixture.{}@example.test>\r\n\r\n{body}",
        mailbox.address(),
        id.0
    );
    let domain = cs_mail_adapters::DomainIdentity::parse_ascii("mail.localhost").unwrap();
    let route = cs_mail_smtp_gateway::incoming::reply_address(conversation, &domain);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mail = runtime
        .block_on(cs_mail_smtp_gateway::incoming::authenticate(
            &SyntheticVerifier(mailbox.domain().clone()),
            cs_mail_adapters::SmtpAuthenticationRequest {
                raw_message: raw.as_bytes(),
                remote_ip: "127.0.0.1".parse().unwrap(),
                helo_identity: "example.test",
                mail_from: None,
                receiver_hostname: domain.as_str(),
                received_at: now,
            },
            &route,
            &domain,
        ))
        .unwrap();
    mail.encrypt_reply(
        ReplyEncryption {
            conversation,
            deployment: scope.deployment_domain,
            provider: scope.provider,
            member: preparation.relationship.recipient,
            relationship: preparation.relationship.relationship,
            content_scope: preparation.relationship.content_scope,
            member_key: preparation.member_key,
            lane,
        },
        &[88; 32],
        now,
    )
    .unwrap()
}

/// A real persisted message pair for callback HTTP experiments: one attempt begun,
/// one still waiting. Uses public fixture secrets and the fixture service year.
#[allow(clippy::too_many_lines)]
pub fn callback_fixture(url: &str) -> cs_mail_storage_postgres::external::Repository {
    use cs_mail_application::correspondence::external as app;
    use cs_mail_finance::{BankVerification, FinancialScope};
    use cs_mail_protocol::ActorRef;
    use cs_mail_security::{
        CommandSigner, EndpointKeyPublication, RecipientSigningScope, SignedEndpointKey,
    };
    let at = CanonicalTime(44_582_400_000);
    let scope = FinancialScope::new(
        [7; 32],
        ProviderRef(30),
        ProgramRef(1),
        [9; 32],
        ProtocolVersion(2),
    );
    let unit = SettlementUnit(1);
    let deployment = cs_mail_storage_postgres::PostgresDeployment::connect(url).unwrap();
    crate::bootstrap(&deployment, scope, unit).unwrap();
    let persona = ProtocolIdentity(20);
    let key = OperationalKeyRef(21);
    let actor = ActorRef::Recipient(persona);
    crate::enrollment::enroll(
        &deployment,
        cs_mail_accounts::EnrollmentInput {
            bank: BankVerification {
                scope,
                account: BillingAccountId(2),
                member: MemberId(2),
                bank_token: [42; 32],
                unit,
                version: 1,
                signature: vec![],
            }
            .sign(&[77; 32])
            .unwrap(),
            persona,
            actor,
            key_ref: key,
            initial_key: CommandSigner::from_secret_bytes(actor, key, &[21; 32])
                .verifying_key_bytes(),
            maximum_unresolved: 10,
        },
        at,
    )
    .unwrap();
    crate::fixture_inbox(&deployment, persona, key, &[21; 32], at).unwrap();
    crate::purchase_service(&deployment, scope, unit, BillingAccountId(2), key, at).unwrap();
    let record = deployment.deployment_record().unwrap().unwrap();
    let signing_scope = RecipientSigningScope {
        deployment_domain: [7; 32],
        provider: ProviderRef(30),
    };
    let recipient_key = EndpointSecretKey::from_secret_bytes(ContentKeyRef(500), &[50; 32])
        .unwrap()
        .1;
    cs_mail_application::members::publish_endpoint_key(
        &deployment,
        &SignedEndpointKey::sign(
            signing_scope,
            key,
            EndpointKeyPublication {
                persona,
                version: 1,
                key: recipient_key,
            },
            &[21; 32],
        )
        .unwrap(),
        signing_scope,
        || at,
    )
    .unwrap();
    let repository = cs_mail_storage_postgres::external::Repository {
        accounts: deployment.accounts(move || at),
        deployment: record,
        deriver: cs_mail_privacy::PrivacyDeriver::new([5; 32], [6; 32]).into(),
        relay_key: EndpointSecretKey::from_secret_bytes(ContentKeyRef(600), &[60; 32])
            .unwrap()
            .1,
    };
    let mailbox = GuestMailbox::parse("carol@example.test").unwrap();
    let account = deployment
        .accounts(move || at)
        .persona_account(persona)
        .unwrap();
    for id in [900, 901] {
        let conversation = ConversationId::new(id).unwrap();
        let message = text(
            &repository.prepare(persona, &mailbox).unwrap(),
            conversation,
            MessageId(id),
            persona,
            at,
        );
        let signed = app::SignedCommand {
            scope: signing_scope,
            product: "cs-mail/test".into(),
            account,
            persona,
            key,
            conversation,
            operation: IdempotencyKey(id),
            signature: vec![],
            command: app::Command::Start {
                receiving_alias: cs_mail_primitives::AliasId::new(persona.0).unwrap(),
                mailbox: mailbox.clone(),
                disclose_to_external_mail: true,
                message: Box::new(message),
            },
        }
        .sign(&[21; 32])
        .unwrap();
        app::Service(&repository).handle(&signed).unwrap();
    }
    repository.begin_dispatch(MessageId(900)).unwrap().unwrap();
    repository
}
