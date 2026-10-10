//! Mailbox control evidence and its session consequence. Stored facts are checked on
//! restoration; their provenance still depends on the trusted persistence boundary.
use crate::{GuestError, GuestMailbox};
use cs_mail_primitives::{CanonicalTime, Duration};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const VERIFICATION_LIFETIME: Duration = Duration(15 * 60 * 1000);
pub const VERIFICATION_ATTEMPTS: u32 = 5;
pub const SESSION_LIFETIME: Duration = Duration(2 * 60 * 60 * 1000);

/// A token's digest identifies its session; it is not proof of mailbox control.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SessionTokenDigest([u8; 32]);
impl SessionTokenDigest {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
enum VerificationState {
    AwaitingCode {
        code_digest: [u8; 32],
    },
    Consumed {
        verified_at: CanonicalTime,
        session_expires_at: CanonicalTime,
        token_digest: SessionTokenDigest,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct VerificationRecord {
    mailbox: GuestMailbox,
    issued_at: CanonicalTime,
    expires_at: CanonicalTime,
    attempts: u32,
    state: VerificationState,
}

/// Checked stored evidence, not independently authenticated authority. Neither its
/// mailbox, attempt count nor consumed state can be changed outside this owner.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "VerificationRecord", into = "VerificationRecord")]
pub struct MailboxVerification(VerificationRecord);
impl TryFrom<VerificationRecord> for MailboxVerification {
    type Error = GuestError;
    fn try_from(record: VerificationRecord) -> Result<Self, Self::Error> {
        if record.expires_at != deadline(record.issued_at, VERIFICATION_LIFETIME)?
            || record.attempts > VERIFICATION_ATTEMPTS
        {
            return Err(GuestError::InvalidVerificationState);
        }
        if let VerificationState::Consumed {
            verified_at,
            session_expires_at,
            ..
        } = record.state
        {
            if record.attempts >= VERIFICATION_ATTEMPTS
                || verified_at < record.issued_at
                || verified_at > record.expires_at
                || session_expires_at != deadline(verified_at, SESSION_LIFETIME)?
            {
                return Err(GuestError::InvalidVerificationState);
            }
        }
        Ok(Self(record))
    }
}
impl From<MailboxVerification> for VerificationRecord {
    fn from(value: MailboxVerification) -> Self {
        value.0
    }
}
fn deadline(at: CanonicalTime, lifetime: Duration) -> Result<CanonicalTime, GuestError> {
    at.0.checked_add(lifetime.0)
        .map(CanonicalTime)
        .ok_or(GuestError::InvalidVerificationState)
}
fn code_digest(code: &str, salt: &[u8; 32]) -> [u8; 32] {
    Sha256::new()
        .chain_update(b"cs-mail/guest-verification-code/v1")
        .chain_update(salt)
        .chain_update(code.as_bytes())
        .finalize()
        .into()
}

/// The complete result of checking a code. Wrong attempts must also be committed.
#[derive(Debug)]
pub enum CodeCheck {
    Verified(VerificationCompletion),
    Wrong(MailboxVerification),
}

/// Produced only by a successful code check. Persistence must commit both effects
/// before exposing the session; the token is bound into the consumed verification.
#[derive(Debug)]
pub struct VerificationCompletion {
    verification: MailboxVerification,
    session: GuestSession,
}
impl VerificationCompletion {
    pub fn into_parts(self) -> (MailboxVerification, GuestSession) {
        (self.verification, self.session)
    }
}
impl MailboxVerification {
    /// # Errors
    /// Refuses a time whose expiry cannot be represented.
    pub fn issue(
        mailbox: GuestMailbox,
        code: &str,
        salt: &[u8; 32],
        now: CanonicalTime,
    ) -> Result<Self, GuestError> {
        Ok(Self(VerificationRecord {
            mailbox,
            issued_at: now,
            expires_at: deadline(now, VERIFICATION_LIFETIME)?,
            attempts: 0,
            state: VerificationState::AwaitingCode {
                code_digest: code_digest(code, salt),
            },
        }))
    }
    pub const fn mailbox(&self) -> &GuestMailbox {
        &self.0.mailbox
    }
    /// # Errors
    /// Refuses closed verifications and exhausted attempts. Time must be sampled after
    /// acquiring the storage protection; the existing inclusive expiry is preserved.
    pub fn check(
        &self,
        code: &str,
        salt: &[u8; 32],
        token_digest: SessionTokenDigest,
        now: CanonicalTime,
    ) -> Result<CodeCheck, GuestError> {
        let VerificationState::AwaitingCode {
            code_digest: expected,
        } = self.0.state
        else {
            return Err(GuestError::VerificationClosed);
        };
        if now < self.0.issued_at || now > self.0.expires_at {
            return Err(GuestError::VerificationClosed);
        }
        if self.0.attempts >= VERIFICATION_ATTEMPTS {
            return Err(GuestError::TooManyAttempts);
        }
        let mut next = self.clone();
        if code_digest(code.trim(), salt) != expected {
            next.0.attempts += 1;
            return Ok(CodeCheck::Wrong(next));
        }
        let session = GuestSession {
            mailbox: self.0.mailbox.clone(),
            verified_at: now,
            expires_at: deadline(now, SESSION_LIFETIME)?,
        };
        next.0.state = VerificationState::Consumed {
            verified_at: now,
            session_expires_at: session.expires_at,
            token_digest,
        };
        Ok(CodeCheck::Verified(VerificationCompletion {
            verification: next,
            session,
        }))
    }
    /// Restores a session from a trusted, durably consumed verification, requiring its
    /// exact token and current lifetime. Decoding these facts alone is not authority;
    /// callers must load them through the protected session-to-verification relation.
    /// # Errors
    /// Refuses an unconsumed verification, another token, or a non-current session.
    pub fn restore_session(
        &self,
        token: SessionTokenDigest,
        now: CanonicalTime,
    ) -> Result<GuestSession, GuestError> {
        let VerificationState::Consumed {
            verified_at,
            session_expires_at,
            token_digest,
        } = self.0.state
        else {
            return Err(GuestError::SessionExpired);
        };
        if token != token_digest {
            return Err(GuestError::SessionExpired);
        }
        let session = GuestSession {
            mailbox: self.0.mailbox.clone(),
            verified_at,
            expires_at: session_expires_at,
        };
        session.authorize(now)?;
        Ok(session)
    }
}

/// Session authority is created by verification or restored from its consumed record.
/// It has no public constructor, writable fields, or deserialization implementation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuestSession {
    mailbox: GuestMailbox,
    verified_at: CanonicalTime,
    expires_at: CanonicalTime,
}
impl GuestSession {
    pub const fn mailbox(&self) -> &GuestMailbox {
        &self.mailbox
    }
    /// # Errors
    /// Refuses times outside the session's verified lifetime.
    pub fn authorize(&self, now: CanonicalTime) -> Result<&GuestMailbox, GuestError> {
        if now < self.verified_at || now > self.expires_at {
            return Err(GuestError::SessionExpired);
        }
        Ok(&self.mailbox)
    }
}
