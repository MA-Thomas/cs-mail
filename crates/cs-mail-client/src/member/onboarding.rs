//! Signed account setup through the existing billing and pricing owners.
//! One encrypted setup journal retains the exact request across an uncertain response.
use super::{Error, MemberClient, State, enrollment::Setup, storage};
use cs_mail_billing::{BillingCommand, SignedBillingCommand};
use cs_mail_primitives::{IdempotencyKey, PolicyVersion};

impl MemberClient {
    /// Current signed account coverage; this is independent of purchase acknowledgement.
    /// # Errors
    /// Account custody, authority or the service is unavailable.
    pub fn service_covered(&self) -> Result<bool, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        Ok(self.account_view(&state.profile, &keys)?.covered)
    }

    /// Purchases a published offer; the returned flag is canonical current coverage,
    /// not an assertion that the collection has completed.
    /// # Errors
    /// Unresolved different setup, unavailable custody, refused command or transport failure.
    pub fn purchase_service(&self, offer: PolicyVersion) -> Result<bool, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        let command = BillingCommand::PurchaseService { offer };
        if let Some(setup) = &state.setup {
            if !matches!(setup, Setup::Purchase(signed) if signed.command == command) {
                return Err(Error::SetupPending);
            }
        } else {
            let account = self.account_view(&state.profile, &keys)?;
            let mut random = [0; 16];
            getrandom::fill(&mut random).map_err(|_| Error::Unavailable)?;
            let signed = SignedBillingCommand::sign(
                account.billing,
                state.profile.sender_key,
                account.financial_scope,
                account.billing_revision,
                IdempotencyKey(u128::from_be_bytes(random) | 1),
                command,
                &keys.device,
            )
            .map_err(|_| Error::InvalidData)?;
            state.setup = Some(Setup::Purchase(Box::new(signed)));
            storage::save(self.store.as_ref(), &keys, &state)?;
        }
        self.finish_onboarding(&mut state, &keys)?;
        let account = self.account_view(&state.profile, &keys)?;
        Ok(account.covered)
    }

    pub(super) fn finish_onboarding(
        &self,
        state: &mut State,
        keys: &storage::Secrets,
    ) -> Result<(), Error> {
        if matches!(state.setup, Some(Setup::Directory(_))) {
            return self.finish_directory(state, keys);
        }
        match &state.setup {
            Some(Setup::Purchase(command)) => {
                let answer: cs_mail_application::billing::operations::BillingResponse = self.call(
                    &state.profile.server,
                    "v1/billing/commands",
                    Some(&serde_json::to_vec(command).map_err(|_| Error::InvalidData)?),
                )?;
                answer
                    .verify(command, &state.profile.deployment.provider_key)
                    .map_err(|_| Error::InvalidData)?;
                if let cs_mail_application::billing::operations::BillingCommandOutcome::Refused(_) =
                    answer.outcome
                {
                    state.setup = None;
                    storage::save(self.store.as_ref(), keys, state)?;
                    return Err(Error::SetupRefused);
                }
            }
            _ => return Ok(()),
        }
        self.account_view(&state.profile, keys)?;
        state.setup = None;
        storage::save(self.store.as_ref(), keys, state)
    }
}
