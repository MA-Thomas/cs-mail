//! Owner-only alias management with the existing encrypted exact-operation journal.
use super::{Error, MemberClient, State, enrollment::Setup, storage};
use cs_mail_accounts::aliases::Alias;
use cs_mail_application::aliases::{Acknowledgement, Command, Outcome, SignedCommand};
use cs_mail_primitives::IdempotencyKey;
impl MemberClient {
    pub(super) fn signed_alias_command(
        state: &State,
        keys: &storage::Secrets,
        command: Command,
    ) -> Result<SignedCommand, Error> {
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(|_| Error::Unavailable)?;
        SignedCommand {
            scope: state.profile.deployment.scope,
            product: state.profile.deployment.product.clone(),
            account: state.profile.account,
            owner: state.profile.persona,
            key: state.profile.recipient_key,
            operation: IdempotencyKey(u128::from_be_bytes(random) | 1),
            command,
            signature: vec![],
        }
        .sign(&keys.recipient)
        .map_err(|_| Error::InvalidData)
    }
    pub(super) fn alias_call(
        &self,
        state: &State,
        request: &SignedCommand,
    ) -> Result<Outcome, Error> {
        let acknowledgement: Acknowledgement = self.call(
            &state.profile.server,
            "v1/aliases/commands",
            Some(&serde_json::to_vec(request).map_err(|_| Error::InvalidData)?),
        )?;
        acknowledgement
            .verify(request, &state.profile.deployment.provider_key)
            .map_err(|_| Error::InvalidData)?;
        Ok(acknowledgement.receipt.outcome)
    }
    /// A signed, non-reserving suggestion. It creates no alias or durable mutation.
    /// # Errors
    /// Refuses invalid name input, unavailable candidates, transport or substituted evidence.
    pub fn suggest_alias(
        &self,
        name: String,
        fragment: Option<String>,
    ) -> Result<cs_mail_application::aliases::generation::AddressSuggestion, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        if state.setup.is_some() {
            return Err(Error::SetupPending);
        }
        let command = Command::Suggest {
            name: name.try_into().map_err(|_| Error::InvalidAliasName)?,
            fragment: fragment
                .map(TryInto::try_into)
                .transpose()
                .map_err(|_| Error::InvalidAliasName)?,
        };
        let request = Self::signed_alias_command(&state, &keys, command)?;
        match self.alias_call(&state, &request)? {
            Outcome::Suggested(value) => Ok(value),
            Outcome::Refused(_) => Err(Error::AddressUnavailable),
            _ => Err(Error::InvalidData),
        }
    }
    /// # Errors
    /// Rejects unavailable authority or substituted acknowledgement evidence.
    pub fn aliases(&self) -> Result<Vec<Alias>, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let request = Self::signed_alias_command(&state, &keys, Command::Inspect)?;
        let Outcome::Aliases(aliases) = self.alias_call(&state, &request)? else {
            return Err(Error::InvalidData);
        };
        if aliases
            .iter()
            .any(|alias| alias.owner() != state.profile.persona)
        {
            return Err(Error::InvalidData);
        }
        Ok(aliases)
    }
    /// # Errors
    /// Unavailable custody/transport, another pending setup, or a durable refusal.
    /// All mutations are journaled before I/O; uncertain results retain the same signature.
    pub fn change_alias(&self, command: Command) -> Result<Outcome, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        if matches!(command, Command::Inspect | Command::Suggest { .. }) {
            return Err(Error::InvalidData);
        }
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        if state.setup.is_some()
            || state.pending.is_some()
            || state.sending.is_some()
            || state.requesting.is_some()
            || state.editing_conversation.is_some()
            || state
                .external
                .as_ref()
                .is_some_and(|workspace| workspace.pending.is_some())
        {
            return Err(Error::SetupPending);
        }
        let request = Self::signed_alias_command(&state, &keys, command)?;
        state.setup = Some(Setup::Alias(Box::new(request)));
        storage::save(self.store.as_ref(), &keys, &state)?;
        self.finish_alias(&mut state, &keys)?
            .ok_or(Error::InvalidData)
    }
    /// # Errors
    /// Keeps uncertain work on failures; only verified terminal outcomes clear it.
    pub fn retry_alias(&self) -> Result<Option<Outcome>, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        self.finish_alias(&mut state, &keys)
    }
    pub(super) fn finish_alias_setup(
        &self,
        state: &mut State,
        keys: &storage::Secrets,
    ) -> Result<(), Error> {
        if matches!(self.finish_alias(state, keys)?, Some(Outcome::Refused(_))) {
            return Err(Error::SetupRefused);
        }
        Ok(())
    }
    pub(super) fn finish_alias(
        &self,
        state: &mut State,
        keys: &storage::Secrets,
    ) -> Result<Option<Outcome>, Error> {
        let Some(Setup::Alias(request)) = &state.setup else {
            return Ok(None);
        };
        let outcome = self.alias_call(state, request)?;
        if matches!(outcome, Outcome::Aliases(_) | Outcome::Suggested(_)) {
            return Err(Error::InvalidData);
        }
        state.setup = None;
        storage::save(self.store.as_ref(), keys, state)?;
        Ok(Some(outcome))
    }
}
