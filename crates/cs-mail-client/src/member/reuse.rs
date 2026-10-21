//! Explicit native source reuse and conversation-policy edits. Only reviewed source
//! selections enter composition; current ancestry and contact authority are checked at commit.
use super::{Error, MemberClient, State, storage::Secrets};
use crate::correspondence::{EndpointKeys, open};
use cs_mail_application::correspondence::{Command, Response};
use cs_mail_correspondence::{
    BlockDescriptor, ContentBlock, DocumentPart, MessageSelection, MessageVersion,
    SelectionElement, SharedCopy, SourceReference,
};
use cs_mail_primitives::MessageId;

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub enum ReuseMode {
    Quotation,
    Sharing,
    Reference,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SourceDraft {
    pub selection: MessageSelection,
    pub mode: ReuseMode,
}
/// Decrypted in memory, with coordinates identical to the immutable manifest.
#[derive(Clone, Debug)]
pub struct SourceMessage {
    pub message: MessageId,
    pub version: MessageVersion,
    pub blocks: Vec<SourceBlock>,
}
#[derive(Clone, Debug)]
pub enum SourceBlock {
    Text { block: u32, text: String },
    Image { block: u32, bytes: Vec<u8> },
    Reference { block: u32 },
}
#[derive(Clone, Debug)]
pub struct ConversationPolicy {
    pub restricted: bool,
    pub revision: u64,
    pub relaxation: Relaxation,
}
#[derive(Clone, Copy, Debug)]
pub enum Relaxation {
    None,
    ProposedByMe,
    AwaitingMyApproval,
}

#[derive(Clone, Debug)]
pub enum ReferenceContent {
    Readable(String),
    Unavailable,
    AccessDenied,
}
impl MemberClient {
    /// Resolves a reference deliberately carried in a readable message. The pointer
    /// grants no source access; the service rechecks the owner's retained selection.
    /// # Errors
    /// Invalid reference, unavailable custody, expired authority or unknown transport outcome.
    pub fn resolve_reference(
        &self,
        message: MessageId,
        part: u32,
    ) -> Result<ReferenceContent, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = super::storage::secrets(self.store.as_ref())?;
        let state = super::storage::open(self.store.as_ref(), &keys)?;
        let (secret, public) = super::correspondence::own_keys(&state.profile, &keys)?;
        let endpoint = EndpointKeys {
            owner: state.profile.persona,
            secret: &secret,
            public,
        };
        let signed = Self::signed(&state.profile, &keys, Command::Fetch { message })?;
        let Response::Message(container) = self.correspondence_call(&state.profile, &signed)?
        else {
            return Ok(ReferenceContent::Unavailable);
        };
        if container.record.manifest.message() != message {
            return Err(Error::InvalidData);
        }
        let document = open(&endpoint, &container, container.record.sender_key)
            .map_err(|_| Error::InvalidData)?;
        let Some(DocumentPart::Reference(reference)) = document.parts().get(part as usize) else {
            return Err(Error::InvalidData);
        };
        let selection = &reference.0;
        let signed = Self::signed(
            &state.profile,
            &keys,
            Command::Resolve {
                source: selection.clone(),
            },
        )?;
        let result = match self.correspondence_call(&state.profile, &signed)? {
            Response::Message(source) => {
                let document = open(&endpoint, &source, source.record.sender_key)
                    .map_err(|_| Error::InvalidData)?;
                let blocks = document
                    .select(&source.record.manifest, selection)
                    .map_err(|_| Error::InvalidData)?;
                ReferenceContent::Readable(
                    blocks
                        .iter()
                        .map(|block| match block {
                            cs_mail_correspondence::ContentBlock::Text(text) => text.clone(),
                            cs_mail_correspondence::ContentBlock::Image(_) => "[Image]".into(),
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
            }
            Response::Unavailable => ReferenceContent::Unavailable,
            Response::AccessDenied => ReferenceContent::AccessDenied,
            _ => return Err(Error::InvalidData),
        };
        let current = super::storage::secrets(self.store.as_ref())?;
        if current.cache != keys.cache {
            return Err(Error::KeyUnavailable);
        }
        Ok(result)
    }
    /// Opens an authorized source without saving plaintext or creating outgoing work.
    /// # Errors
    /// Source unavailable, invalid authenticated document or unavailable custody.
    pub fn source_message(&self, message: MessageId) -> Result<SourceMessage, Error> {
        if message.0 == 0 {
            return Err(Error::InvalidData);
        }
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = super::storage::secrets(self.store.as_ref())?;
        let state = super::storage::open(self.store.as_ref(), &keys)?;
        let signed = Self::signed(&state.profile, &keys, Command::Fetch { message })?;
        let Response::Message(view) = self.correspondence_call(&state.profile, &signed)? else {
            return Err(Error::SourceUnavailable);
        };
        if view.record.manifest.message() != message {
            return Err(Error::InvalidData);
        }
        let (secret, public) = super::correspondence::own_keys(&state.profile, &keys)?;
        let endpoint = EndpointKeys {
            owner: state.profile.persona,
            secret: &secret,
            public,
        };
        let document =
            open(&endpoint, &view, view.record.sender_key).map_err(|_| Error::InvalidData)?;
        let manifest = &view.record.manifest;
        let blocks = manifest
            .blocks()
            .iter()
            .enumerate()
            .map(|(index, descriptor)| {
                let block = u32::try_from(index).map_err(|_| Error::InvalidData)?;
                let element = match descriptor {
                    BlockDescriptor::Text { bytes: 0 } => {
                        return Ok(SourceBlock::Text {
                            block,
                            text: String::new(),
                        });
                    }
                    BlockDescriptor::Text { bytes } => SelectionElement::TextRange {
                        block,
                        start: 0,
                        end: *bytes,
                    },
                    BlockDescriptor::Image { .. } => SelectionElement::ImageBlock { block },
                    BlockDescriptor::Reference => return Ok(SourceBlock::Reference { block }),
                };
                let selection = MessageSelection::new(message, manifest.version(), vec![element])
                    .map_err(|_| Error::InvalidData)?;
                match document
                    .select(manifest, &selection)
                    .map_err(|_| Error::InvalidData)?
                    .pop()
                {
                    Some(ContentBlock::Text(text)) => Ok(SourceBlock::Text { block, text }),
                    Some(ContentBlock::Image(image)) => Ok(SourceBlock::Image {
                        block,
                        bytes: image.bytes().to_vec(),
                    }),
                    None => Err(Error::InvalidData),
                }
            })
            .collect::<Result<Vec<_>, Error>>()?;
        if super::storage::secrets(self.store.as_ref())?.cache != keys.cache {
            return Err(Error::KeyUnavailable);
        }
        Ok(SourceMessage {
            message,
            version: manifest.version(),
            blocks,
        })
    }
    pub(super) fn source_parts(
        &self,
        state: &State,
        keys: &Secrets,
        sources: &[SourceDraft],
    ) -> Result<Vec<DocumentPart>, Error> {
        if sources.len() > cs_mail_correspondence::MAX_PARTS {
            return Err(Error::InvalidData);
        }
        let (secret, public) = super::correspondence::own_keys(&state.profile, keys)?;
        let endpoint = EndpointKeys {
            owner: state.profile.persona,
            secret: &secret,
            public,
        };
        sources
            .iter()
            .map(|source| {
                let signed = Self::signed(
                    &state.profile,
                    keys,
                    Command::Fetch {
                        message: source.selection.message(),
                    },
                )?;
                let Response::Message(view) = self.correspondence_call(&state.profile, &signed)?
                else {
                    return Err(Error::SourceUnavailable);
                };
                if view.record.manifest.message() != source.selection.message() {
                    return Err(Error::InvalidData);
                }
                let manifest = &view.record.manifest;
                let selection = source.selection.clone();
                selection
                    .validate_target(manifest)
                    .map_err(|_| Error::InvalidData)?;
                let document = open(&endpoint, &view, view.record.sender_key)
                    .map_err(|_| Error::InvalidData)?;
                // Even pointers must select valid UTF-8 boundaries in authenticated plaintext.
                document
                    .select(manifest, &selection)
                    .map_err(|_| Error::InvalidData)?;
                match source.mode {
                    ReuseMode::Reference => Ok(DocumentPart::Reference(SourceReference(selection))),
                    ReuseMode::Quotation => Ok(DocumentPart::Quotation(
                        document
                            .quote(manifest, selection)
                            .map_err(|_| Error::InvalidData)?,
                    )),
                    ReuseMode::Sharing => Ok(DocumentPart::Share(SharedCopy(
                        document
                            .quote(manifest, selection)
                            .map_err(|_| Error::InvalidData)?,
                    ))),
                }
            })
            .collect()
    }

    /// # Errors
    /// Invalid conversation, unresolved work, unavailable custody, changed policy or transport failure.
    pub fn change_conversation_policy(
        &self,
        conversation: u128,
        revision: u64,
        action: cs_mail_correspondence::PolicyAction,
    ) -> Result<(), Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = super::storage::secrets(self.store.as_ref())?;
        let mut state = super::storage::open(self.store.as_ref(), &keys)?;
        super::require_idle(&state)?;
        self.account_view(&state.profile, &keys)?;
        let id = cs_mail_correspondence::ConversationId::new(conversation)
            .map_err(|_| Error::InvalidData)?;
        state.editing_conversation = Some(Self::signed(
            &state.profile,
            &keys,
            Command::ChangePolicy {
                id,
                revision,
                action,
            },
        )?);
        super::storage::save(self.store.as_ref(), &keys, &state)?;
        self.transmit_conversation_edit(&mut state, &keys)
    }

    /// Permanently removes only this member's native endpoint and recovery copies.
    /// The exact signed deletion stays journaled across an uncertain response.
    /// # Errors
    /// Invalid thread, denied authority, unresolved work, refusal or transport failure.
    pub fn delete_native_copies(
        &self,
        conversation: cs_mail_correspondence::ConversationId,
    ) -> Result<(), Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = super::storage::secrets(self.store.as_ref())?;
        let mut state = super::storage::open(self.store.as_ref(), &keys)?;
        super::require_idle(&state)?;
        if !state
            .mailbox
            .participants
            .contains_key(&conversation.value())
        {
            return Err(Error::InvalidData);
        }
        self.account_view(&state.profile, &keys)?;
        state.editing_conversation = Some(Self::signed(
            &state.profile,
            &keys,
            Command::DeleteCopies { id: conversation },
        )?);
        super::storage::save(self.store.as_ref(), &keys, &state)?;
        self.transmit_conversation_edit(&mut state, &keys)
    }

    pub(super) fn transmit_conversation_edit(
        &self,
        state: &mut State,
        keys: &Secrets,
    ) -> Result<(), Error> {
        let Some(signed) = &state.editing_conversation else {
            return Ok(());
        };
        let (id, deleting) = match signed.command {
            Command::ChangePolicy { id, .. } => (id, false),
            Command::DeleteCopies { id } => (id, true),
            _ => return Err(Error::InvalidData),
        };
        let refused = match self.correspondence_call(&state.profile, signed)? {
            Response::CopiesDeleted if deleting => false,
            Response::Conversation(found)
                if !deleting
                    && found.id() == id
                    && found.scope().contains(state.profile.persona) =>
            {
                false
            }
            Response::Refused {
                operation, digest, ..
            } if operation == signed.operation
                && digest == signed.digest().map_err(|_| Error::InvalidData)? =>
            {
                true
            }
            _ => return Err(Error::InvalidData),
        };
        if deleting && !refused {
            let mut retained = super::retained::Retained::load(self, keys, state)?;
            for (_, record) in &state.mailbox.records {
                if record.manifest.conversation() == id {
                    retained.exclude(super::retained::CopyKey {
                        thread: super::MailKey::Native(id),
                        message: record.manifest.message(),
                    });
                }
            }
            retained.save(self, keys)?;
        }
        state.editing_conversation = None;
        super::storage::save(self.store.as_ref(), keys, state)?;
        if deleting {
            self.clear_search()?;
        }
        if refused {
            Err(if deleting {
                Error::CopyDeletionRefused
            } else {
                Error::PolicyRefused
            })
        } else {
            Ok(())
        }
    }
}
