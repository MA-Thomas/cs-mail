//! Consent-gated recovery and processing. Account recovery establishes authority,
//! while this separate application owns permission to use retained ciphertext.
use crate::{
    accounts::{AccountClock, operations::AccountState},
    correspondence::MessageRecord,
};
use cs_mail_consent::{
    ConsentError, ContentScope, DecryptionActor, DecryptionGrant, DecryptionPurpose, GrantId,
    GrantTerms, GrantUse,
};
use cs_mail_content::{EncryptedContentRecord, EndpointPublicKey};
use cs_mail_correspondence::MessageManifest;
use cs_mail_primitives::{
    AccountId, CanonicalTime, IdempotencyKey, MessageId, OperationalKeyRef, ProtocolIdentity,
};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Separate recovery envelope per retained owner; never a mailbox private key.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryCopy {
    pub sender: EndpointPublicKey,
    pub ciphertext: EncryptedContentRecord,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessorRegistration {
    pub key: OperationalKeyRef,
    pub verifying_key: [u8; 32],
    pub usage: GrantUse,
    pub active: bool,
}
impl ProcessorRegistration {
    /// # Errors
    /// Registration is trusted deployment configuration, never a content grant.
    pub fn validate(&self) -> Result<(), ConsentError> {
        self.usage.validate()?;
        VerifyingKey::from_bytes(&self.verifying_key).map_err(|_| ConsentError::Invalid)?;
        if self.usage.actor != DecryptionActor::CsqdProcessor(self.key) {
            return Err(ConsentError::Invalid);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ConsentCommand {
    Authorize {
        id: GrantId,
        scope: ContentScope,
        usage: GrantUse,
        expires_at: CanonicalTime,
    },
    Revoke {
        id: GrantId,
    },
    Inspect {
        id: GrantId,
    },
    Read {
        id: GrantId,
        message: MessageId,
        purpose: DecryptionPurpose,
    },
}
impl ConsentCommand {
    pub const fn grant_id(&self) -> GrantId {
        match self {
            Self::Authorize { id, .. }
            | Self::Revoke { id }
            | Self::Inspect { id }
            | Self::Read { id, .. } => *id,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedConsentRequest {
    pub product: String,
    pub account: AccountId,
    pub persona: ProtocolIdentity,
    pub actor: DecryptionActor,
    pub operation: IdempotencyKey,
    pub command: ConsentCommand,
    pub signature: Vec<u8>,
}
impl SignedConsentRequest {
    fn bytes(&self) -> Result<Vec<u8>, ConsentError> {
        if self.product.is_empty()
            || self.account.0 == 0
            || self.persona.0 == 0
            || self.operation.0 == 0
        {
            return Err(ConsentError::Invalid);
        }
        serde_json::to_vec(&(
            "cs-mail/consent-command/v1",
            &self.product,
            self.account,
            self.persona,
            self.actor,
            self.operation,
            &self.command,
        ))
        .map_err(|_| ConsentError::Invalid)
    }
    /// # Errors
    /// Rejects invalid signing context.
    pub fn sign(mut self, secret: &[u8; 32]) -> Result<Self, ConsentError> {
        self.signature = SigningKey::from_bytes(secret)
            .sign(&self.bytes()?)
            .to_bytes()
            .to_vec();
        Ok(self)
    }
    /// # Errors
    /// Rejects tampered commands or invalid keys.
    pub fn verify(&self, key: &[u8; 32]) -> Result<(), ConsentError> {
        VerifyingKey::from_bytes(key)
            .map_err(|_| ConsentError::Unauthorized)?
            .verify_strict(
                &self.bytes()?,
                &Signature::from_slice(&self.signature).map_err(|_| ConsentError::Unauthorized)?,
            )
            .map_err(|_| ConsentError::Unauthorized)
    }
    /// # Errors
    /// Rejects invalid serialization. Audit records contain only the digest and signature.
    pub fn digest(&self) -> Result<[u8; 32], ConsentError> {
        Ok(Sha256::digest(self.bytes()?).into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReleasedCopy {
    pub manifest: MessageManifest,
    pub ciphertext: EncryptedContentRecord,
    pub custodian: EndpointPublicKey,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ConsentOutcome {
    Granted(GrantId),
    Revoked(GrantId),
    Released { grant: GrantId, message: MessageId },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ConsentResponse {
    Outcome(ConsentOutcome),
    Grant(DecryptionGrant),
    Released(Box<ReleasedCopy>),
    /// Historical commitment only: no cached ciphertext or new disclosure.
    AlreadyReleased {
        grant: GrantId,
        message: MessageId,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsentReceipt {
    pub account: AccountId,
    pub persona: ProtocolIdentity,
    pub actor: DecryptionActor,
    pub operation: IdempotencyKey,
    pub digest: [u8; 32],
    pub signature: Vec<u8>,
    pub verifying_key: [u8; 32],
    pub authorized_at: CanonicalTime,
    pub outcome: ConsentOutcome,
}
pub struct RetainedRecovery {
    pub record: MessageRecord,
    pub recovery: RecoveryCopy,
    pub expires_at: CanonicalTime,
}
pub struct ConsentContext {
    pub authority: AccountState,
    pub custody_key: EndpointPublicKey,
    pub processor: Option<ProcessorRegistration>,
    pub grant: Option<DecryptionGrant>,
    pub retained: BTreeMap<MessageId, RetainedRecovery>,
    pub prior: Option<ConsentReceipt>,
}
pub struct ConsentEffects {
    pub grant: Option<DecryptionGrant>,
    pub receipt: ConsentReceipt,
}
pub struct ConsentDecision {
    response: ConsentResponse,
    effects: Option<ConsentEffects>,
}
impl ConsentDecision {
    pub fn into_parts(self) -> (ConsentResponse, Option<ConsentEffects>) {
        (self.response, self.effects)
    }
}

/// Constructed only after live authorization and entitlement, and consumed locally
/// under the persistence protection. It is not a transferable or durable permit.
pub struct AuthorizedDecryption<'a> {
    source: &'a RecoveryCopy,
    manifest: &'a MessageManifest,
    destination: EndpointPublicKey,
    at: CanonicalTime,
    expires_at: CanonicalTime,
}
impl AuthorizedDecryption<'_> {
    pub const fn source(&self) -> &RecoveryCopy {
        self.source
    }
    pub const fn manifest(&self) -> &MessageManifest {
        self.manifest
    }
    pub const fn destination(&self) -> EndpointPublicKey {
        self.destination
    }
    pub const fn at(&self) -> CanonicalTime {
        self.at
    }
    pub const fn expires_at(&self) -> CanonicalTime {
        self.expires_at
    }
}
/// Local crypto only: implementations MUST NOT call remote services, retain a
/// permit, or disclose plaintext. A remote custodian requires a separate protocol.
pub trait LocalKeyCustody {
    fn public_key(&self) -> EndpointPublicKey;
    /// # Errors
    /// Rejects key mismatch, invalid ciphertext, substituted document or binding.
    fn release(
        &self,
        authorization: AuthorizedDecryption<'_>,
    ) -> Result<ReleasedCopy, ConsentError>;
}
/// Protect account/keys, consent, copy retention, custody config and operation ID
/// until receipt commit. No released ciphertext may escape a failed transaction.
pub trait ConsentStore {
    type Error: From<ConsentError>;
    /// # Errors
    /// Rolls back every effect on decision or persistence failure.
    fn consent_transaction(
        &self,
        request: &SignedConsentRequest,
        decide: impl FnOnce(ConsentContext, &dyn AccountClock) -> Result<ConsentDecision, Self::Error>,
    ) -> Result<ConsentResponse, Self::Error>;
}
pub struct ConsentService<'a, R, K> {
    repository: &'a R,
    custodian: &'a K,
}
impl<'a, R: ConsentStore, K: LocalKeyCustody> ConsentService<'a, R, K> {
    pub const fn new(repository: &'a R, custodian: &'a K) -> Self {
        Self {
            repository,
            custodian,
        }
    }
    /// # Errors
    /// Rejects missing current authority, consent, content or exact retry identity.
    pub fn handle(&self, request: &SignedConsentRequest) -> Result<ConsentResponse, R::Error> {
        self.repository
            .consent_transaction(request, |context, clock| {
                decide(request, &context, self.custodian, clock.now()).map_err(Into::into)
            })
    }
}
/// Authenticate before private content lookup, and again after all protected reads.
/// # Errors
/// Processors must be registered and active; registration never grants content access.
pub fn authenticate(
    request: &SignedConsentRequest,
    authority: &AccountState,
    processor: Option<&ProcessorRegistration>,
    at: CanonicalTime,
) -> Result<[u8; 32], ConsentError> {
    let key = match request.actor {
        DecryptionActor::UserDevice(k) => authority
            .persona_key(&request.product, request.persona, k, at)
            .map_err(|_| ConsentError::Unauthorized)?,
        DecryptionActor::CsqdProcessor(k) => {
            if authority.product != request.product
                || !authority.control.allows_service()
                || !authority.control.personas().contains(&request.persona)
            {
                return Err(ConsentError::Unauthorized);
            }
            let p = processor
                .filter(|p| p.key == k && p.active)
                .ok_or(ConsentError::Unauthorized)?;
            p.validate()?;
            p.verifying_key
        }
    };
    request.verify(&key)?;
    Ok(key)
}
fn owned_grant<'a>(
    r: &SignedConsentRequest,
    c: &'a ConsentContext,
) -> Result<&'a DecryptionGrant, ConsentError> {
    c.grant
        .as_ref()
        .filter(|g| {
            g.terms().account == r.account
                && g.terms().persona == r.persona
                && g.terms().id == r.command.grant_id()
        })
        .ok_or(ConsentError::Unauthorized)
}
fn entitled<'a>(
    c: &'a ConsentContext,
    r: &SignedConsentRequest,
    message: MessageId,
    now: CanonicalTime,
) -> Result<&'a RetainedRecovery, ConsentError> {
    let retained = c
        .retained
        .get(&message)
        .filter(|v| v.expires_at > now && v.recovery.ciphertext.expires_at > now)
        .ok_or(ConsentError::Unavailable)?;
    let copy = &retained.recovery;
    if retained.record.manifest.message() != message
        || copy.ciphertext.binding.message_id != message
        || copy.ciphertext.binding.recipient != r.persona
        || copy.ciphertext.binding.sender != retained.record.author
        || copy.sender.reference != copy.ciphertext.envelope.sender_key
        || copy.ciphertext.envelope.recipient_key != c.custody_key.reference
    {
        return Err(ConsentError::Invalid);
    }
    Ok(retained)
}
fn check_processor(c: &ConsentContext, usage: &GrantUse) -> Result<(), ConsentError> {
    if let DecryptionActor::CsqdProcessor(k) = usage.actor {
        let p = c
            .processor
            .as_ref()
            .filter(|p| p.active && p.key == k && p.usage == *usage)
            .ok_or(ConsentError::Unauthorized)?;
        p.validate()?;
    }
    Ok(())
}
fn issue(
    r: &SignedConsentRequest,
    c: &ConsentContext,
    now: CanonicalTime,
) -> Result<DecryptionGrant, ConsentError> {
    let DecryptionActor::UserDevice(issuer) = r.actor else {
        return Err(ConsentError::Unauthorized);
    };
    let ConsentCommand::Authorize {
        id,
        scope,
        usage,
        expires_at,
    } = &r.command
    else {
        return Err(ConsentError::Invalid);
    };
    if c.grant.is_some() {
        return Err(ConsentError::Conflict);
    }
    check_processor(c, usage)?;
    for message in scope.messages() {
        entitled(c, r, *message, now)?;
    }
    DecryptionGrant::issue(GrantTerms {
        id: *id,
        account: r.account,
        persona: r.persona,
        issuer,
        scope: scope.clone(),
        usage: usage.clone(),
        issued_at: now,
        expires_at: *expires_at,
    })
}
fn read<K: LocalKeyCustody>(
    r: &SignedConsentRequest,
    c: &ConsentContext,
    custodian: &K,
    now: CanonicalTime,
) -> Result<ReleasedCopy, ConsentError> {
    let ConsentCommand::Read {
        message, purpose, ..
    } = &r.command
    else {
        return Err(ConsentError::Invalid);
    };
    let grant = owned_grant(r, c)?;
    // Recovery revokes the old signing keys, invalidating their existing grants.
    c.authority
        .persona_key(&r.product, r.persona, grant.terms().issuer, now)
        .map_err(|_| ConsentError::Unauthorized)?;
    grant.authorize(r.actor, purpose, *message, now)?;
    check_processor(c, &grant.terms().usage)?;
    let retained = entitled(c, r, *message, now)?;
    if c.custody_key != custodian.public_key() {
        return Err(ConsentError::Unavailable);
    }
    let expires_at = grant
        .terms()
        .expires_at
        .min(retained.expires_at)
        .min(retained.recovery.ciphertext.expires_at);
    let released = custodian.release(AuthorizedDecryption {
        source: &retained.recovery,
        manifest: &retained.record.manifest,
        destination: grant.terms().usage.destination,
        at: now,
        expires_at,
    })?;
    if released.manifest != retained.record.manifest
        || released.custodian != c.custody_key
        || released.ciphertext.binding != retained.recovery.ciphertext.binding
        || released.ciphertext.envelope.recipient_key != grant.terms().usage.destination.reference
        || released.ciphertext.expires_at != expires_at
    {
        return Err(ConsentError::Invalid);
    }
    Ok(released)
}
fn decide<K: LocalKeyCustody>(
    r: &SignedConsentRequest,
    c: &ConsentContext,
    custodian: &K,
    now: CanonicalTime,
) -> Result<ConsentDecision, ConsentError> {
    let verifying_key = authenticate(r, &c.authority, c.processor.as_ref(), now)?;
    let digest = r.digest()?;
    if let Some(prior) = &c.prior {
        if prior.digest != digest
            || prior.account != r.account
            || prior.persona != r.persona
            || prior.actor != r.actor
            || prior.operation != r.operation
        {
            return Err(ConsentError::Conflict);
        }
        let matches_command = match (&r.command, &prior.outcome) {
            (ConsentCommand::Authorize { id, .. }, ConsentOutcome::Granted(g))
            | (ConsentCommand::Revoke { id }, ConsentOutcome::Revoked(g)) => id == g,
            (
                ConsentCommand::Read { id, message, .. },
                ConsentOutcome::Released { grant, message: m },
            ) => id == grant && message == m,
            _ => false,
        };
        if !matches_command
            || prior.verifying_key != verifying_key
            || prior.signature != r.signature
        {
            return Err(ConsentError::Invalid);
        }
        let response = match prior.outcome {
            ConsentOutcome::Released { grant, message } => {
                ConsentResponse::AlreadyReleased { grant, message }
            }
            _ => ConsentResponse::Outcome(prior.outcome.clone()),
        };
        return Ok(ConsentDecision {
            response,
            effects: None,
        });
    }
    if let ConsentCommand::Inspect { .. } = r.command {
        if !matches!(r.actor, DecryptionActor::UserDevice(_)) {
            return Err(ConsentError::Unauthorized);
        }
        return Ok(ConsentDecision {
            response: ConsentResponse::Grant(owned_grant(r, c)?.clone()),
            effects: None,
        });
    }
    let (response, outcome, grant) = match &r.command {
        ConsentCommand::Authorize { id, .. } => {
            let grant = issue(r, c, now)?;
            (
                ConsentResponse::Outcome(ConsentOutcome::Granted(*id)),
                ConsentOutcome::Granted(*id),
                Some(grant),
            )
        }
        ConsentCommand::Revoke { id } => {
            if !matches!(r.actor, DecryptionActor::UserDevice(_)) {
                return Err(ConsentError::Unauthorized);
            }
            let grant = owned_grant(r, c)?.revoke(now)?;
            (
                ConsentResponse::Outcome(ConsentOutcome::Revoked(*id)),
                ConsentOutcome::Revoked(*id),
                Some(grant),
            )
        }
        ConsentCommand::Read { id, message, .. } => {
            let released = read(r, c, custodian, now)?;
            (
                ConsentResponse::Released(Box::new(released)),
                ConsentOutcome::Released {
                    grant: *id,
                    message: *message,
                },
                None,
            )
        }
        ConsentCommand::Inspect { .. } => return Err(ConsentError::Invalid),
    };
    Ok(ConsentDecision {
        response,
        effects: Some(ConsentEffects {
            grant,
            receipt: ConsentReceipt {
                account: r.account,
                persona: r.persona,
                actor: r.actor,
                operation: r.operation,
                digest,
                signature: r.signature.clone(),
                verifying_key,
                authorized_at: now,
                outcome,
            },
        }),
    })
}
