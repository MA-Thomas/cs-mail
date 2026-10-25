//! Inbox membership is a projection of recorded receiving evidence, never sender inference.
use super::{
    Error, MailItem, MailKey, ManagedAddress, MemberClient, State,
    personal::{self, Personal},
    storage,
};
use cs_mail_application::{
    aliases::{DeliveredRoute, DeliveryTransport},
    correspondence::{Command, Response},
};
use cs_mail_primitives::{AccountId, AliasId, ProtocolIdentity};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InboxScope {
    All,
    Alias(AliasId),
}
#[derive(Clone, Debug)]
pub struct MessageRoute {
    pub message: String,
    pub alias: String,
}
#[derive(Clone, Debug)]
pub struct AliasInbox {
    pub address: ManagedAddress,
    pub unread: u64,
    pub items: Vec<MailItem>,
}
#[derive(Clone, Debug)]
pub struct SenderLaneScope {
    pub sender: String,
    pub lane: String,
    pub aliases: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct InboxWorkspace {
    inboxes: Vec<AliasInbox>,
    sent: Vec<MailItem>,
    sender_lanes: Vec<SenderLaneScope>,
}
impl InboxWorkspace {
    /// The aggregate is derived from receiving inboxes and separately identified
    /// sent copies. It has no independently writable received-correspondence list.
    pub fn into_parts(self) -> (Vec<MailItem>, Vec<AliasInbox>, Vec<SenderLaneScope>) {
        let mut union = BTreeMap::<MailKey, MailItem>::new();
        for item in self.inboxes.iter().flat_map(|i| &i.items).chain(&self.sent) {
            if let Some(current) = union.get_mut(&item.key) {
                current.messages.extend(&item.messages);
                current.messages.sort_unstable();
                current.messages.dedup();
                current.receiving.extend(item.receiving.iter().cloned());
                current.unread += item.unread;
                current.last_at = current.last_at.max(item.last_at);
                current.has_sent |= item.has_sent;
            } else {
                union.insert(item.key, item.clone());
            }
        }
        let mut all: Vec<_> = union.into_values().collect();
        all.sort_by_key(|i| (std::cmp::Reverse(i.last_at), i.key));
        (all, self.inboxes, self.sender_lanes)
    }
}
fn sent_items(state: &State, all: Vec<MailItem>) -> Vec<MailItem> {
    let mut sent = BTreeSet::new();
    for (_, record) in &state.mailbox.records {
        if record.author == state.profile.persona {
            sent.insert((
                MailKey::Native(record.manifest.conversation()),
                record.manifest.message(),
            ));
        }
    }
    if let Some(external) = &state.external {
        for record in external.records.values() {
            for view in &record.messages {
                if view.message.direction
                    == cs_mail_application::correspondence::external::Direction::MemberToGuest
                {
                    sent.insert((
                        MailKey::External(record.conversation.id()),
                        view.message.manifest.message(),
                    ));
                }
            }
        }
    }
    all.into_iter()
        .filter_map(|mut item| {
            item.messages.retain(|id| sent.contains(&(item.key, *id)));
            item.receiving.clear();
            item.unread = 0;
            (!item.messages.is_empty()).then_some(item)
        })
        .collect()
}
#[derive(Clone)]
pub(super) struct InboxCatalog {
    account: AccountId,
    owner: ProtocolIdentity,
    aliases: BTreeSet<AliasId>,
}
impl InboxCatalog {
    pub(super) fn new(
        account: AccountId,
        owner: ProtocolIdentity,
        aliases: &[cs_mail_accounts::aliases::Alias],
    ) -> Result<Self, Error> {
        let by_id: BTreeMap<_, _> = aliases.iter().cloned().map(|a| (a.id(), a)).collect();
        if by_id.len() != aliases.len() {
            return Err(Error::InvalidData);
        }
        cs_mail_accounts::aliases::AliasPortfolio::new(owner, &by_id)
            .map_err(|_| Error::InvalidData)?;
        let ids = by_id.into_keys().collect();
        Ok(Self {
            account,
            owner,
            aliases: ids,
        })
    }
    pub(super) fn contains(&self, id: AliasId) -> bool {
        self.aliases.contains(&id)
    }
}
/// Membership is carried by received records. There is no independent routing snapshot.
fn received_routes(state: &State) -> Result<Vec<DeliveredRoute>, Error> {
    let mut routes = Vec::new();
    for (_, record) in &state.mailbox.records {
        if record.receiving.recipient() == state.profile.persona {
            routes.push(record.receiving.clone());
        } else if record.author != state.profile.persona {
            return Err(Error::InvalidData);
        }
    }
    if let Some(external) = &state.external {
        for record in external.records.values() {
            for entry in &record.messages {
                if let cs_mail_application::correspondence::external::Direction::GuestToMember(
                    route,
                ) = &entry.message.direction
                {
                    if route.recipient() != state.profile.persona {
                        return Err(Error::InvalidData);
                    }
                    routes.push((**route).clone());
                }
            }
        }
    }
    Ok(routes)
}
impl MemberClient {
    /// Observes owner-only alias and routing metadata; no content is fetched or decrypted.
    /// A failed observation never converts a known route into an invented assignment.
    /// # Errors
    /// Unavailable current metadata, invalid owner binding or substituted receiving evidence.
    pub fn inbox_workspace(&self) -> Result<InboxWorkspace, Error> {
        match self.observe_inbox_workspace() {
            Ok(value) => Ok(value),
            Err(Error::Offline) => self.cached_inbox_workspace(),
            Err(error) => Err(error),
        }
    }
    /// # Errors
    /// Unavailable local custody or invalid retained metadata.
    pub fn cached_inbox_workspace(&self) -> Result<InboxWorkspace, Error> {
        let _guard = self.workspace.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let retained = super::retained::Retained::load(self, &keys, &state)?;
        *self.receiving.lock().map_err(|_| Error::Unavailable)? = Some(retained.inboxes()?);
        let aliases = retained.aliases();
        drop(retained);
        let personal = Personal::load(self, &keys, &state)?;
        let all = self.scope_items(
            &state,
            personal::items(&state, &personal)?,
            InboxScope::All,
            &personal,
        )?;
        let mut inboxes = Vec::new();
        for alias in aliases {
            let items = self.scope_items(
                &state,
                all.clone(),
                InboxScope::Alias(alias.id()),
                &personal,
            )?;
            let unread = items
                .iter()
                .filter(|i| i.folder == super::MailFolder::Inbox)
                .map(|i| i.unread)
                .sum();
            inboxes.push(AliasInbox {
                address: super::alias_views::project(&alias, (self.clock)()),
                unread,
                items,
            });
        }
        Ok(InboxWorkspace {
            sent: sent_items(&state, all),
            inboxes,
            sender_lanes: Vec::new(),
        })
    }
    #[allow(clippy::too_many_lines)] // Validate one complete observation before publishing any inbox.
    fn observe_inbox_workspace(&self) -> Result<InboxWorkspace, Error> {
        let _guard = self.operation.lock().map_err(|_| Error::Unavailable)?;
        let keys = storage::secrets(self.store.as_ref())?;
        let state = storage::open(self.store.as_ref(), &keys)?;
        let request = Self::signed_alias_command(
            &state,
            &keys,
            cs_mail_application::aliases::Command::Inspect,
        )?;
        let cs_mail_application::aliases::Outcome::Aliases(aliases) =
            self.alias_call(&state, &request)?
        else {
            return Err(Error::InvalidData);
        };
        let alias_ids: BTreeSet<_> = aliases
            .iter()
            .map(cs_mail_accounts::aliases::Alias::id)
            .collect();
        if aliases.iter().any(|a| a.owner() != state.profile.persona)
            || alias_ids.len() != aliases.len()
        {
            return Err(Error::InvalidData);
        }
        let personal = Personal::load(self, &keys, &state)?;
        let all = personal::items(&state, &personal)?;
        let ids: Vec<_> = all
            .iter()
            .flat_map(|i| i.messages.iter().copied())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let routes = received_routes(&state)?;
        if routes.iter().any(|r| !alias_ids.contains(&r.alias())) {
            return Err(Error::InvalidData);
        }
        let mut sender_lanes = Vec::new();
        let chunks: Vec<&[cs_mail_primitives::MessageId]> = if ids.is_empty() {
            vec![&[]]
        } else {
            ids.chunks(100).collect()
        };
        for chunk in chunks {
            let command = Self::signed(
                &state.profile,
                &keys,
                Command::ReceivingRoutes {
                    messages: chunk.to_vec(),
                },
            )?;
            let Response::ReceivingRoutes {
                routes: found,
                sender_scopes,
            } = self.correspondence_call(&state.profile, &command)?
            else {
                return Err(Error::InvalidData);
            };
            if found.iter().any(|r| {
                r.recipient() != state.profile.persona
                    || !alias_ids.contains(&r.alias())
                    || !chunk.contains(&r.message())
            }) {
                return Err(Error::InvalidData);
            }
            for scope in sender_scopes {
                if scope.recipient != state.profile.persona
                    || scope
                        .receiving
                        .aliases()
                        .iter()
                        .any(|a| !alias_ids.contains(a))
                {
                    return Err(Error::InvalidData);
                }
                if let Some((lane, _)) = scope.lane {
                    if !sender_lanes.iter().any(|s: &SenderLaneScope| {
                        s.sender == scope.sender.0.to_string() && s.lane == lane.0.to_string()
                    }) {
                        sender_lanes.push(SenderLaneScope {
                            sender: scope.sender.0.to_string(),
                            lane: lane.0.to_string(),
                            aliases: scope
                                .receiving
                                .aliases()
                                .iter()
                                .map(|a| a.value().to_string())
                                .collect(),
                        });
                    }
                }
            }
            if found.iter().any(|r| !routes.contains(r)) {
                return Err(Error::InvalidData);
            }
        }
        let mut incoming = BTreeMap::new();
        for (_, record) in &state.mailbox.records {
            if record.author != state.profile.persona {
                incoming.insert(
                    (
                        MailKey::Native(record.manifest.conversation()),
                        record.manifest.message(),
                    ),
                    (record.author, record.committed_at),
                );
            }
        }
        if let Some(external) = &state.external {
            for record in external.records.values() {
                for entry in &record.messages {
                    if matches!(
                        &entry.message.direction,
                        cs_mail_application::correspondence::external::Direction::GuestToMember(_)
                    ) {
                        incoming.insert(
                            (
                                MailKey::External(record.conversation.id()),
                                entry.message.manifest.message(),
                            ),
                            (record.conversation.guest(), entry.message.committed_at),
                        );
                    }
                }
            }
        }
        let mut unique = BTreeSet::new();
        for r in &routes {
            let key = route_key(r);
            if !unique.insert((key, r.message()))
                || incoming.get(&(key, r.message())) != Some(&(r.sender(), r.at()))
            {
                return Err(Error::InvalidData);
            }
        }
        storage::check_keys(self.store.as_ref(), &keys)?;
        let snapshot = InboxCatalog::new(state.profile.account, state.profile.persona, &aliases)?;
        let mut retained = super::retained::Retained::load(self, &keys, &state)?;
        retained.set_aliases(self, &keys, aliases.clone())?;
        drop(retained);
        *self.receiving.lock().map_err(|_| Error::Unavailable)? = Some(snapshot);
        let all = self.scope_items(&state, all, InboxScope::All, &personal)?;
        let mut inboxes = Vec::new();
        for alias in aliases {
            let items = self.scope_items(
                &state,
                all.clone(),
                InboxScope::Alias(alias.id()),
                &personal,
            )?;
            let unread = items
                .iter()
                .filter(|i| i.folder == super::MailFolder::Inbox)
                .map(|i| i.unread)
                .sum();
            inboxes.push(AliasInbox {
                address: super::alias_views::project(&alias, (self.clock)()),
                unread,
                items,
            });
        }
        inboxes.sort_by(|a, b| {
            a.address
                .description
                .as_ref()
                .unwrap_or(&a.address.address)
                .cmp(b.address.description.as_ref().unwrap_or(&b.address.address))
                .then(a.address.id.cmp(&b.address.id))
        });
        Ok(InboxWorkspace {
            sent: sent_items(&state, all),
            inboxes,
            sender_lanes,
        })
    }
    /// Drop all session routing/search state when the session ends.
    /// # Errors
    /// Poisoned session lock.
    pub fn clear_inboxes(&self) -> Result<(), Error> {
        *self.receiving.lock().map_err(|_| Error::Unavailable)? = None;
        {
            let mut index = self.retained.lock().map_err(|_| Error::Unavailable)?;
            *index = None;
            self.retained_epoch
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        self.clear_search()
    }
    pub(super) fn scope_items(
        &self,
        state: &State,
        mut items: Vec<MailItem>,
        scope: InboxScope,
        personal: &Personal,
    ) -> Result<Vec<MailItem>, Error> {
        let keys = storage::secrets(self.store.as_ref())?;
        let retained = super::retained::Retained::load(self, &keys, state)?;
        for item in &mut items {
            if let Some(subject) = retained.subject(item.key, (self.clock)()) {
                subject.text().clone_into(&mut item.title);
            }
        }
        let snapshot = self.receiving.lock().map_err(|_| Error::Unavailable)?;
        let snapshot = snapshot
            .as_ref()
            .filter(|r| r.account == state.profile.account && r.owner == state.profile.persona);
        let Some(snapshot) = snapshot else {
            return if scope == InboxScope::All {
                Ok(items)
            } else {
                Err(Error::Unavailable)
            };
        };
        if let InboxScope::Alias(id) = scope {
            if !snapshot.aliases.contains(&id) {
                return Err(Error::InvalidData);
            }
        }
        let assignments = received_routes(state)?;
        if assignments
            .iter()
            .any(|r| !snapshot.aliases.contains(&r.alias()))
        {
            return Err(Error::Unavailable);
        }
        let routes: BTreeMap<_, _> = assignments
            .iter()
            .map(|r| ((route_key(r), r.message()), r))
            .collect();
        for item in &mut items {
            item.receiving = item
                .messages
                .iter()
                .filter_map(|id| {
                    routes.get(&(item.key, *id)).map(|r| MessageRoute {
                        message: id.0.to_string(),
                        alias: r.alias().value().to_string(),
                    })
                })
                .collect();
            if let InboxScope::Alias(alias) = scope {
                item.messages.retain(|id| {
                    routes
                        .get(&(item.key, *id))
                        .is_some_and(|r| r.alias() == alias)
                });
                item.receiving
                    .retain(|r| r.alias == alias.value().to_string());
                // Receiving membership does not invent a sending address for replies.
                item.has_sent = false;
                item.unread = personal.unread(item.key, &item.messages);
                item.last_at = item
                    .messages
                    .iter()
                    .filter_map(|id| routes.get(&(item.key, *id)).map(|r| r.at().0))
                    .max()
                    .unwrap_or(0);
            }
        }
        storage::check_keys(self.store.as_ref(), &keys)?;
        if scope != InboxScope::All {
            items.retain(|i| !i.messages.is_empty());
        }
        items.sort_by_key(|i| (std::cmp::Reverse(i.last_at), i.key));
        Ok(items)
    }
}
fn route_key(r: &DeliveredRoute) -> MailKey {
    match r.transport() {
        DeliveryTransport::Native => MailKey::Native(r.conversation()),
        DeliveryTransport::External => MailKey::External(r.conversation()),
    }
}
