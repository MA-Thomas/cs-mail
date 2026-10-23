//! Atomic address publication and public directory projections. No SQL or network I/O.
use cs_mail_accounts::control::AccountStatus;
use cs_mail_accounts::directory::{
    DirectoryEntry, DirectoryQuery, DirectoryRevision, DiscoveryVisibility, PublicProfile,
};
use cs_mail_primitives::{CanonicalTime, IdempotencyKey, OperationalKeyRef, ProtocolIdentity};
use cs_mail_protocol::{
    ActorRef,
    pricing::{RecipientRequestClasses, RequestPricingPolicy, SenderOffer},
};
use cs_mail_security::{AuthoritySnapshot, RecipientSigningScope, SecurityError};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Error {
    Invalid,
    Unauthorized,
    Conflict,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
impl From<cs_mail_accounts::AccountError> for Error {
    fn from(_: cs_mail_accounts::AccountError) -> Self {
        Self::Invalid
    }
}
impl From<SecurityError> for Error {
    fn from(_: SecurityError) -> Self {
        Self::Unauthorized
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Command {
    Inspect {
        at: CanonicalTime,
    },
    Publish {
        expected: DirectoryRevision,
        profile: PublicProfile,
        mailbox: Option<cs_mail_accounts::directory::MemberAddress>,
        visibility: DiscoveryVisibility,
        classes: Box<cs_mail_security::SignedRequestClasses>,
    },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignedRequest {
    pub scope: RecipientSigningScope,
    pub address: ProtocolIdentity,
    pub alias: cs_mail_primitives::AliasId,
    pub key: OperationalKeyRef,
    pub operation: IdempotencyKey,
    pub command: Command,
    pub signature: Vec<u8>,
}
impl SignedRequest {
    fn bytes(&self) -> Result<Vec<u8>, Error> {
        if self.address.0 == 0 || self.operation.0 == 0 {
            return Err(Error::Invalid);
        }
        serde_json::to_vec(&(
            "cs-mail/alias-directory/v2",
            self.scope,
            self.address,
            self.alias,
            self.key,
            self.operation,
            &self.command,
        ))
        .map_err(|_| Error::Invalid)
    }
    /// # Errors
    /// Rejects invalid command context.
    pub fn sign(mut self, secret: &[u8; 32]) -> Result<Self, Error> {
        self.signature = SigningKey::from_bytes(secret)
            .sign(&self.bytes()?)
            .to_bytes()
            .to_vec();
        Ok(self)
    }
    /// # Errors
    /// Rejects altered signatures or context.
    pub fn verify(&self, key: &[u8; 32]) -> Result<(), Error> {
        VerifyingKey::from_bytes(key)
            .map_err(|_| Error::Unauthorized)?
            .verify_strict(
                &self.bytes()?,
                &Signature::from_slice(&self.signature).map_err(|_| Error::Unauthorized)?,
            )
            .map_err(|_| Error::Unauthorized)
    }
    /// Digest binds canonical command and signature for exact replay.
    /// # Errors
    /// Rejects unencodable commands.
    pub fn digest(&self) -> Result<[u8; 32], Error> {
        Ok(Sha256::digest(serde_json::to_vec(self).map_err(|_| Error::Invalid)?).into())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Receipt {
    pub digest: [u8; 32],
    pub revision: DirectoryRevision,
    pub at: CanonicalTime,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Settings {
    pub entry: Option<DirectoryEntry>,
    pub classes: Option<RecipientRequestClasses>,
    pub policy: Option<RequestPricingPolicy>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum Response {
    Settings(Box<Settings>),
    Published(Receipt),
    Refused { digest: [u8; 32], error: Error },
}
pub struct Context {
    pub alias: cs_mail_accounts::aliases::Alias,
    pub authority: AuthoritySnapshot,
    pub current: Option<DirectoryEntry>,
    pub classes: Option<RecipientRequestClasses>,
    pub policy: Option<RequestPricingPolicy>,
    pub prior: Option<Response>,
    pub mailbox_available: bool,
}
pub struct Effects {
    pub entry: DirectoryEntry,
    pub classes: cs_mail_security::SignedRequestClasses,
    pub receipt: Receipt,
}
pub struct Decision {
    response: Response,
    effects: Option<Effects>,
    record: bool,
}
impl Decision {
    pub fn into_parts(self) -> (Response, Option<Effects>, bool) {
        (self.response, self.effects, self.record)
    }
}
pub trait Store {
    type Error: From<Error>;
    /// Protect ownership, account authority, current policy and absent entry; call once,
    /// then commit profile, classes and replay evidence together. No network work under locks.
    /// # Errors
    /// Refusal or storage failure rolls back every effect.
    fn transact_directory(
        &self,
        request: &SignedRequest,
        decide: impl FnOnce(Context) -> Result<Decision, Self::Error>,
    ) -> Result<Response, Self::Error>;
}
pub struct DirectoryService<'a, S>(pub &'a S);
impl<S: Store> DirectoryService<'_, S> {
    /// # Errors
    /// Refuses foreign address authority, stale revisions and altered retries.
    pub fn handle(
        &self,
        request: &SignedRequest,
        scope: RecipientSigningScope,
        mail_domain: Option<&str>,
        clock: impl FnOnce() -> CanonicalTime,
    ) -> Result<Response, S::Error> {
        self.0.transact_directory(request, |context| {
            if let Command::Publish {
                mailbox: Some(mailbox),
                ..
            } = &request.command
            {
                if mail_domain != Some(mailbox.domain()) {
                    return Err(Error::Invalid.into());
                }
            }
            decide(request, context, scope, clock()).map_err(Into::into)
        })
    }
}
fn decide(
    request: &SignedRequest,
    c: Context,
    scope: RecipientSigningScope,
    now: CanonicalTime,
) -> Result<Decision, Error> {
    if let Some(prior) = &c.prior {
        let prior_digest = match prior {
            Response::Published(r) => r.digest,
            Response::Refused { digest, .. } => *digest,
            Response::Settings(_) => return Err(Error::Invalid),
        };
        if prior_digest != request.digest()? {
            return Err(Error::Conflict);
        }
    }
    match decide_checked(request, c, scope, now) {
        Err(error @ (Error::Conflict | Error::Invalid)) => Ok(Decision {
            response: Response::Refused {
                digest: request.digest()?,
                error,
            },
            effects: None,
            record: true,
        }),
        other => other,
    }
}
#[allow(clippy::too_many_lines)] // Publication authority, alias binding and atomic effects form one decision.
fn decide_checked(
    request: &SignedRequest,
    c: Context,
    scope: RecipientSigningScope,
    now: CanonicalTime,
) -> Result<Decision, Error> {
    if request.scope != scope {
        return Err(Error::Unauthorized);
    }
    if c.alias.id() != request.alias
        || c.alias.owner() != request.address
        || !c.alias.is_bonded_route()
    {
        return Err(Error::Unauthorized);
    }
    let digest = request.digest()?;
    if let Some(prior) = c.prior {
        let prior_digest = match &prior {
            Response::Published(r) => r.digest,
            Response::Refused { digest, .. } => *digest,
            Response::Settings(_) => return Err(Error::Invalid),
        };
        if prior_digest != digest {
            return Err(Error::Conflict);
        }
        return Ok(Decision {
            response: prior,
            effects: None,
            record: false,
        });
    }
    let key =
        c.authority
            .active_verifying_key(request.key, ActorRef::Recipient(request.address), now)?;
    request.verify(&key)?;
    match &request.command {
        Command::Inspect { at } => {
            if at.0 > now.0 || now.0.saturating_sub(at.0) > 300_000 {
                return Err(Error::Unauthorized);
            }
            Ok(Decision {
                response: Response::Settings(Box::new(Settings {
                    entry: c.current,
                    classes: c.classes,
                    policy: c.policy,
                })),
                effects: None,
                record: false,
            })
        }
        Command::Publish {
            expected,
            profile,
            mailbox,
            visibility,
            classes,
        } => {
            if !c.mailbox_available {
                return Err(Error::Conflict);
            }
            if c.current
                .as_ref()
                .map_or(DirectoryRevision(0), DirectoryEntry::revision)
                != *expected
            {
                return Err(Error::Conflict);
            }
            if classes.classes.recipient() != request.address
                || classes.classes.alias() != request.alias
                || mailbox.as_ref() != Some(c.alias.address())
            {
                return Err(Error::Invalid);
            }
            crate::request_pricing::decide_classes(
                classes,
                scope,
                crate::request_pricing::ClassesPublicationContext {
                    authority: c.authority,
                    policy: c.policy,
                    current: c.classes,
                },
                now,
            )
            .map_err(|e| match e {
                crate::request_pricing::ClassesError::Security(_) => Error::Unauthorized,
                crate::request_pricing::ClassesError::Protocol(
                    cs_mail_protocol::ProtocolError::VersionConflict,
                ) => Error::Conflict,
                crate::request_pricing::ClassesError::Protocol(_) => Error::Invalid,
            })?;
            let entry = DirectoryEntry::replace(
                c.current.as_ref(),
                request.address,
                request.alias,
                *expected,
                profile.clone(),
                *visibility,
                mailbox.clone(),
            )?;
            let receipt = Receipt {
                digest,
                revision: entry.revision(),
                at: now,
            };
            Ok(Decision {
                response: Response::Published(receipt.clone()),
                effects: Some(Effects {
                    entry,
                    classes: (**classes).clone(),
                    receipt,
                }),
                record: true,
            })
        }
    }
}

/// Public view has no ownership, key registry, billing record or person evidence.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Approach {
    pub entry: DirectoryEntry,
    pub offer: Option<SenderOffer>,
    pub covered: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Page {
    pub entries: Vec<Approach>,
    pub next_offset: Option<u32>,
}
/// Database-selected facts; interpreted against an explicit time and public purpose.
pub struct Candidate {
    pub entry: DirectoryEntry,
    pub status: AccountStatus,
    pub billing: cs_mail_billing::BillingAccount,
    pub classes: Option<RecipientRequestClasses>,
}
pub fn approach(
    candidate: Candidate,
    policy: Option<&RequestPricingPolicy>,
    at: CanonicalTime,
) -> Option<Approach> {
    if candidate.status != AccountStatus::Active {
        return None;
    }
    let covered = candidate.billing.covers(at);
    let offer = candidate
        .classes
        .as_ref()
        .and_then(|classes| policy.map(|p| p.sender_offer(classes)));
    Some(Approach {
        entry: candidate.entry,
        offer,
        covered,
    })
}
/// Listed search is separate from direct lookup; exact references grant no contact authority.
pub fn searchable(entry: &DirectoryEntry, query: &DirectoryQuery) -> bool {
    if entry.visibility() != DiscoveryVisibility::Listed {
        return false;
    }
    let text = query.text().to_lowercase();
    if entry.mailbox().is_some_and(|m| m.as_str().contains(&text)) {
        return true;
    }
    entry
        .profile()
        .display_name()
        .to_lowercase()
        .contains(&text)
        || entry.profile().description().to_lowercase().contains(&text)
        || entry.address().0.to_string().contains(&text)
}
