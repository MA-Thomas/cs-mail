use cs_mail_accounts::EnrollmentInput;
use cs_mail_primitives::{AccountId, PrincipalRef};
use identity_contract::{EnrollmentIntent, Error, MAX_LIFETIME, VERSION, digest, random_id};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingEnrollment {
    account: AccountId,
    principal: PrincipalRef,
    input: EnrollmentInput,
    intent: EnrollmentIntent,
}
impl PendingEnrollment {
    /// Backend-generated IDs and challenge; persist before exposing the intent.
    /// # Errors
    /// Rejects invalid input, timestamps or unavailable system randomness.
    pub fn new(
        product: &str,
        operation: &str,
        input: EnrollmentInput,
        now: i64,
    ) -> Result<Self, Error> {
        input.validate().map_err(|_| Error::Invalid)?;
        let account = AccountId(random_number()?);
        let principal = PrincipalRef(random_number()?);
        let intent = EnrollmentIntent {
            version: VERSION,
            product: product.into(),
            operation: operation.into(),
            account: account.0.to_string(),
            challenge: random_id()?,
            initial_key: input.initial_key,
            enrollment_digest: digest("cs-mail/enrollment/v1", &(account, principal, &input))?,
            bank_digest: digest("cs-mail/bank-evidence/v1", &input.bank)?,
            created_at: now,
            expires_at: now.checked_add(MAX_LIFETIME).ok_or(Error::Invalid)?,
        };
        intent.validate(now)?;
        Ok(Self {
            account,
            principal,
            input,
            intent,
        })
    }
    /// Refreshes only authentication context. Stable ownership never changes.
    /// # Errors
    /// A still-live attempt cannot be replaced.
    pub fn renew(&self, now: i64) -> Result<Self, Error> {
        self.validate()?;
        if now < self.intent.expires_at {
            return Err(Error::Conflict);
        }
        let mut renewed = self.clone();
        renewed.intent.challenge = random_id()?;
        renewed.intent.created_at = now;
        renewed.intent.expires_at = now.checked_add(MAX_LIFETIME).ok_or(Error::Invalid)?;
        renewed.intent.validate(now)?;
        Ok(renewed)
    }
    pub fn intent(&self) -> &EnrollmentIntent {
        &self.intent
    }
    pub fn input(&self) -> &EnrollmentInput {
        &self.input
    }
    pub const fn account(&self) -> AccountId {
        self.account
    }
    pub const fn principal(&self) -> PrincipalRef {
        self.principal
    }
    /// Revalidates persistence/wire data before a transition.
    /// # Errors
    /// Rejects substituted input, IDs, or intent.
    pub fn validate(&self) -> Result<(), Error> {
        self.input.validate().map_err(|_| Error::Invalid)?;
        if self.account.0 == 0
            || self.principal.0 == 0
            || self.intent.account != self.account.0.to_string()
            || self.intent.initial_key != self.input.initial_key
            || self.intent.enrollment_digest
                != digest(
                    "cs-mail/enrollment/v1",
                    &(self.account, self.principal, &self.input),
                )?
            || self.intent.bank_digest != digest("cs-mail/bank-evidence/v1", &self.input.bank)?
        {
            return Err(Error::Context);
        }
        Ok(())
    }
}
fn random_number() -> Result<u128, Error> {
    u128::from_str_radix(&random_id()?[..32], 16).map_err(|_| Error::Unavailable)
}
