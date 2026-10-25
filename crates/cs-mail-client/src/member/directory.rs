//! Portable address setup and discovery. Exact signed edits are journaled before I/O.
use super::{Error, MemberClient, State, enrollment::Setup, storage};
use cs_mail_accounts::directory::{
    DirectoryQuery, DirectoryRevision, DiscoveryVisibility, PublicProfile,
};
use cs_mail_application::directory::{Approach, Command, Page, Response, Settings, SignedRequest};
use cs_mail_primitives::{IdempotencyKey, ProtocolIdentity};
use cs_mail_protocol::{
    ActorRef,
    pricing::{RecipientRequestClasses, RequestClass},
};
use cs_mail_security::CommandSigner;

fn operation() -> Result<IdempotencyKey, Error> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| Error::Unavailable)?;
    Ok(IdempotencyKey(u128::from_be_bytes(bytes) | 1))
}
impl MemberClient {
    fn inspect_directory(
        &self,
        state: &State,
        keys: &storage::Secrets,
        alias: cs_mail_primitives::AliasId,
    ) -> Result<Settings, Error> {
        let request = SignedRequest {
            scope: state.profile.deployment.scope,
            address: state.profile.persona,
            alias,
            key: state.profile.recipient_key,
            operation: operation()?,
            command: Command::Inspect { at: (self.clock)() },
            signature: Vec::new(),
        }
        .sign(&keys.recipient)
        .map_err(|_| Error::InvalidData)?;
        let response: Response = self.call(
            &state.profile.server,
            "v1/directory/commands",
            Some(&serde_json::to_vec(&request).map_err(|_| Error::InvalidData)?),
        )?;
        let Response::Settings(settings) = response else {
            return Err(Error::InvalidData);
        };
        if settings
            .entry
            .as_ref()
            .is_some_and(|e| e.address() != state.profile.persona || e.alias() != alias)
            || settings
                .classes
                .as_ref()
                .is_some_and(|c| c.recipient() != state.profile.persona || c.alias() != alias)
        {
            return Err(Error::InvalidData);
        }
        Ok(*settings)
    }
    /// # Errors
    /// Current address authority, custody or network is unavailable.
    pub fn directory_settings(
        &self,
        alias: cs_mail_primitives::AliasId,
    ) -> Result<Settings, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        self.finish_device_setup(&mut state, &keys)?;
        self.inspect_directory(&state, &keys, alias)
    }
    /// Atomic profile/class publication. A reviewed revision cannot silently overwrite
    /// a later edit. Interrupted edits retain their signed bytes through reopening.
    /// # Errors
    /// Conflicting setup, stale revision, invalid classes, unavailable custody or network.
    #[allow(clippy::too_many_arguments)] // These independently authored publication fields are signed together.
    pub fn publish_directory(
        &self,
        alias: cs_mail_primitives::AliasId,
        expected: DirectoryRevision,
        class_version: u64,
        profile: PublicProfile,
        visibility: DiscoveryVisibility,
        classes: Vec<RequestClass>,
        mailbox: Option<cs_mail_accounts::directory::MemberAddress>,
    ) -> Result<(), Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let mut state = storage::open(self.store.as_ref(), &keys)?;
        if state.setup.is_some() {
            return Err(Error::SetupPending);
        }
        let classes =
            RecipientRequestClasses::new(state.profile.persona, alias, class_version, classes)
                .map_err(|_| Error::InvalidData)?;
        let classes = CommandSigner::from_secret_bytes(
            ActorRef::Recipient(state.profile.persona),
            state.profile.recipient_key,
            &keys.recipient,
        )
        .sign_request_classes(state.profile.deployment.scope, classes)
        .map_err(|_| Error::InvalidData)?;
        let signed = SignedRequest {
            scope: state.profile.deployment.scope,
            address: state.profile.persona,
            alias,
            key: state.profile.recipient_key,
            operation: operation()?,
            command: Command::Publish {
                expected,
                profile,
                mailbox,
                visibility,
                classes: Box::new(classes),
            },
            signature: Vec::new(),
        }
        .sign(&keys.recipient)
        .map_err(|_| Error::InvalidData)?;
        state.setup = Some(Setup::Directory(Box::new(signed)));
        storage::save(self.store.as_ref(), &keys, &state)?;
        self.finish_directory(&mut state, &keys)
    }
    pub(super) fn finish_directory(
        &self,
        state: &mut State,
        keys: &storage::Secrets,
    ) -> Result<(), Error> {
        let Some(Setup::Directory(request)) = &state.setup else {
            return Ok(());
        };
        let response: Response = self.call(
            &state.profile.server,
            "v1/directory/commands",
            Some(&serde_json::to_vec(request).map_err(|_| Error::InvalidData)?),
        )?;
        let receipt = match response {
            Response::Published(receipt) => receipt,
            Response::Refused { digest, .. }
                if digest == request.digest().map_err(|_| Error::InvalidData)? =>
            {
                state.setup = None;
                storage::save(self.store.as_ref(), keys, state)?;
                return Err(Error::PublicationRefused);
            }
            _ => return Err(Error::InvalidData),
        };
        let Command::Publish { expected, .. } = request.command else {
            return Err(Error::InvalidData);
        };
        if receipt.digest != request.digest().map_err(|_| Error::InvalidData)?
            || receipt.revision.0 != expected.0.checked_add(1).ok_or(Error::InvalidData)?
        {
            return Err(Error::InvalidData);
        }
        state.setup = None;
        storage::save(self.store.as_ref(), keys, state)
    }
    /// # Errors
    /// Invalid query, network failure or malformed result.
    pub fn directory_search(&self, query: &DirectoryQuery) -> Result<Page, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        self.call(
            &state.profile.server,
            "v1/directory/search",
            Some(&serde_json::to_vec(&query).map_err(|_| Error::InvalidData)?),
        )
    }
    /// # Errors
    /// Unknown/unavailable address, invalid reference or network failure.
    pub fn directory_lookup(&self, reference: &str) -> Result<Approach, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let reference = if reference.starts_with("https://") || reference.starts_with("http://") {
            let url = reqwest::Url::parse(reference).map_err(|_| Error::InvalidData)?;
            let server =
                reqwest::Url::parse(&state.profile.server).map_err(|_| Error::InvalidData)?;
            if url.origin() != server.origin() || url.query().is_some() || url.fragment().is_some()
            {
                return Err(Error::InvalidData);
            }
            url.path().to_owned()
        } else {
            reference.trim().to_owned()
        };
        let result: Approach = self.call(
            &state.profile.server,
            "v1/directory/lookup",
            Some(
                &serde_json::to_vec(&serde_json::json!({"reference":reference}))
                    .map_err(|_| Error::InvalidData)?,
            ),
        )?;
        if result.entry.address() == ProtocolIdentity(0) {
            return Err(Error::InvalidData);
        }
        Ok(result)
    }
    /// # Errors
    /// Local state or key custody unavailable.
    pub fn directory_identity(
        &self,
        alias: cs_mail_primitives::AliasId,
    ) -> Result<(String, String, bool), Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        Ok((
            state.profile.persona.0.to_string(),
            format!("{}/r/{}", state.profile.server, alias.value()),
            matches!(state.setup, Some(Setup::Directory(_))),
        ))
    }
}

/// A resolved destination is routing evidence, never send permission.
#[derive(Clone, Debug)]
pub enum ComposerRecipient {
    Native {
        other: ProtocolIdentity,
        alias: cs_mail_primitives::AliasId,
        mailbox: Option<String>,
    },
    External {
        mailbox: String,
    },
}
impl MemberClient {
    /// Resolve native publications first. A failed native-domain lookup must never
    /// silently become plaintext external mail; transport failures never fall back.
    /// # Errors
    /// Invalid address, unavailable native publication, custody or transport.
    pub fn composer_recipient(&self, reference: &str) -> Result<ComposerRecipient, Error> {
        match self.directory_lookup(reference) {
            Ok(approach) => Ok(ComposerRecipient::Native {
                other: approach.entry.address(),
                alias: approach.entry.alias(),
                mailbox: approach.entry.mailbox().map(|m| m.as_str().to_owned()),
            }),
            Err(Error::AddressUnavailable) => {
                let mailbox = cs_mail_guests::GuestMailbox::parse(reference)
                    .map_err(|_| Error::InvalidData)?;
                let owned = self.address_workspace()?;
                if owned.addresses.is_empty()
                    || owned.addresses.iter().any(|address| {
                        cs_mail_guests::GuestMailbox::parse(&address.address)
                            .is_ok_and(|native| native.domain() == mailbox.domain())
                    })
                {
                    return Err(Error::AddressUnavailable);
                }
                Ok(ComposerRecipient::External {
                    mailbox: mailbox.address(),
                })
            }
            Err(error) => Err(error),
        }
    }
}
