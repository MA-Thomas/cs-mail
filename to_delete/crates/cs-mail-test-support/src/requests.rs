//! Member request messages as a sender's client stages them: documents in the request's
//! conversation, certified by the sender's content key and recoverable by the custodian.
use cs_mail_application::correspondence::RequestDocument;
use cs_mail_client::correspondence::{EndpointKeys, compose, prepare};
use cs_mail_content::{
    ContentBinding, ContentKeyCertificate, EndpointSecretKey, content_key_certificate_digest,
};
use cs_mail_correspondence::{ConversationId, DocumentPart};
use cs_mail_primitives::{
    CanonicalTime, ContentKeyRef, ContentKeyVersion, ContentRef, ContentScopeRef,
    MessageDeclarations, MessageId, MessageValidityUntil, OperationalKeyRef, RequestId,
    WireVersion,
};
use cs_mail_protocol::{ActorRef, PolicySnapshot};
use cs_mail_security::CommandSigner;
use cs_mail_storage_postgres::{PostgresEngine, StorageError};

/// The fixtures' custody content key seed.
pub const FIXTURE_CUSTODY_SECRET: [u8; 32] = [13; 32];
/// The fixtures' custody content key reference.
pub const FIXTURE_CUSTODY_KEY: ContentKeyRef = ContentKeyRef(13);

/// One request message to stage.
pub struct StagedMessage<'a> {
    pub request: RequestId,
    pub message: MessageId,
    pub content: ContentRef,
    pub conversation: u128,
    /// The sender's operational key and its signing secret.
    pub sender_key: OperationalKeyRef,
    pub sender_secret: [u8; 32],
    pub declarations: MessageDeclarations,
    pub created: CanonicalTime,
    pub valid_until: MessageValidityUntil,
    pub text: &'a str,
}

/// Stages one request message in `engine`'s relationship, configuring the fixtures'
/// custodian first.
/// # Errors
/// The staging refusals of `store_request_document`.
/// # Panics
/// On invalid fixture keys or document content.
pub fn stage_request_message(
    engine: &PostgresEngine,
    policy: &PolicySnapshot,
    deployment_domain: [u8; 32],
    staged: &StagedMessage<'_>,
) -> Result<RequestDocument, StorageError> {
    let key = engine.snapshot()?.state.relationship.key;
    let (_, custody) =
        EndpointSecretKey::from_secret_bytes(FIXTURE_CUSTODY_KEY, &FIXTURE_CUSTODY_SECRET)
            .expect("fixture custody key");
    let at = staged.created;
    engine
        .deployment()
        .accounts(move || at)
        .configure_custody(custody)?;
    let (sender_content, sender_public) =
        EndpointSecretKey::from_secret_bytes(ContentKeyRef(2), &staged.sender_secret)
            .expect("fixture sender content key");
    let (_, recipient_public) = EndpointSecretKey::from_secret_bytes(ContentKeyRef(1), &[41; 32])
        .expect("fixture recipient content key");
    let certificate = CommandSigner::from_secret_bytes(
        ActorRef::Sender(key.sender),
        staged.sender_key,
        &staged.sender_secret,
    )
    .sign_content_key_certificate(ContentKeyCertificate {
        wire_version: WireVersion(1),
        protocol_version: policy.protocol_version,
        deployment_domain,
        intended_provider: policy.recipient_provider,
        relationship: key.reference,
        owner: key.sender,
        operational_key: staged.sender_key,
        content_key_version: ContentKeyVersion(1),
        key: sender_public,
        valid_from: staged.created,
        valid_until: staged.valid_until.0,
    })
    .expect("fixture certificate");
    let document =
        compose(vec![DocumentPart::Text(staged.text.to_owned())]).expect("fixture document");
    let package = prepare(
        &EndpointKeys {
            owner: key.sender,
            secret: &sender_content,
            public: sender_public,
        },
        &document,
        ConversationId::new(staged.conversation).expect("fixture conversation"),
        recipient_public,
        custody,
        ContentBinding {
            wire_version: WireVersion(1),
            content_ref: staged.content,
            message_id: staged.message,
            sender: key.sender,
            recipient: key.recipient,
            protocol_version: policy.protocol_version,
            relationship: key.reference,
            content_scope: ContentScopeRef::from_u128_for_test(staged.content.0),
            sender_certificate: content_key_certificate_digest(&certificate.certificate),
            declarations: staged.declarations.clone(),
            message_valid_until: staged.valid_until,
            capability: None,
        },
        (staged.created, staged.valid_until.0),
    )
    .expect("fixture package");
    let document = RequestDocument {
        request: staged.request,
        content_ref: staged.content,
        package,
    };
    engine.store_request_document(
        &document,
        &certificate,
        || staged.created,
        policy,
        deployment_domain,
    )?;
    Ok(document)
}
