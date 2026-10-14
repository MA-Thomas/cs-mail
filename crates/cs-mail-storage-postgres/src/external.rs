//! Atomic external correspondence. Uses the same receipt-order lock as native
//! contact decisions; no network I/O occurs while any database lock is held.
use super::{
    Json, PostgresAccountRepository, StorageError, Transaction, billing, bootstrap_in, guests,
    ingress, members,
};
use cs_mail_application::accounts::{AccountClock, operations::AccountState};
use cs_mail_application::correspondence::external::{
    Command, Context, Decision, Direction, Dispatch, DispatchContext, IncomingEvidence, Message,
    MessageView, Receipt, Reply, ReplyContext, Response, SignedCommand, Store, admit_reply,
    begin_dispatch,
};
use cs_mail_application::members::RelationshipAddress;
use cs_mail_content::EndpointPublicKey;
use cs_mail_correspondence::external::{ExternalConversation, delivery::DeliveryState};
use cs_mail_correspondence::{ConversationId, CorrespondenceError as Error};
use cs_mail_guests::{GuestMailbox, HistoryEvidence, SenderSubject};
use cs_mail_operations::DeploymentBootstrap;
use cs_mail_primitives::{AccountId, CanonicalTime, MessageId, ProtocolIdentity};
use cs_mail_privacy::PrivacyDeriver;
use cs_mail_smtp_gateway::incoming::AuthenticatedReply;
use std::fmt::Write;

pub struct Repository {
    pub accounts: PostgresAccountRepository,
    pub deployment: DeploymentBootstrap,
    pub deriver: PrivacyDeriver,
    pub relay_key: EndpointPublicKey,
}
fn authority(
    tx: &mut Transaction<'_>,
    account: AccountId,
    persona: ProtocolIdentity,
) -> Result<AccountState, StorageError> {
    let row = tx.query_opt("SELECT a.control,a.registry,a.product FROM cs_product_accounts a JOIN cs_persona_owners p ON p.account=a.id WHERE a.id=$1 AND p.identity=$2 FOR UPDATE OF a",
        &[&account.0.to_string(), &persona.0.to_string()])?.ok_or(Error::Unauthorized)?;
    Ok(AccountState {
        control: row.try_get::<_, Json<_>>(0)?.0,
        registry: row.try_get::<_, Json<_>>(1)?.0,
        product: row.try_get(2)?,
    })
}
fn conversation(
    tx: &mut Transaction<'_>,
    id: ConversationId,
) -> Result<Option<(ExternalConversation, String, AccountId)>, StorageError> {
    tx.query_opt(
        "SELECT record,aggregate_key,account FROM cs_external_conversations WHERE id=$1 FOR UPDATE",
        &[&id.value().to_string()],
    )?
    .map(|r| {
        let c = r.try_get::<_, Json<ExternalConversation>>(0)?.0;
        if c.id() != id {
            return Err(Error::Invalid.into());
        }
        Ok((
            c,
            r.try_get(1)?,
            AccountId(
                r.try_get::<_, String>(2)?
                    .parse()
                    .map_err(|_| Error::Invalid)?,
            ),
        ))
    })
    .transpose()
}
fn relationship(
    tx: &mut Transaction<'_>,
    key: &str,
) -> Result<cs_mail_protocol::Relationship, StorageError> {
    Ok(tx.query_one("SELECT relationship_state FROM cs_relationship_aggregates WHERE aggregate_key=$1 FOR UPDATE", &[&key])?
        .try_get::<_, Json<super::domain::RelationshipRecord>>(0)?.0.into_state().relationship)
}

fn mailbox_relationship(
    tx: &mut Transaction<'_>,
    guest: ProtocolIdentity,
    member: ProtocolIdentity,
) -> Result<Option<(String, cs_mail_protocol::Relationship)>, StorageError> {
    tx.query_opt("SELECT aggregate_key,relationship_state FROM cs_relationship_aggregates WHERE relationship_state#>>'{relationship,key,sender}'=$1 AND relationship_state#>>'{relationship,key,recipient}'=$2 FOR UPDATE",
        &[&guest.0.to_string(), &member.0.to_string()])?
        .map(|r| Ok((r.try_get(0)?, r.try_get::<_, Json<super::domain::RelationshipRecord>>(1)?.0.into_state().relationship))).transpose()
}
fn member_account(
    tx: &mut Transaction<'_>,
    member: ProtocolIdentity,
) -> Result<AccountId, StorageError> {
    Ok(AccountId(
        tx.query_opt(
            "SELECT account FROM cs_persona_owners WHERE identity=$1",
            &[&member.0.to_string()],
        )?
        .ok_or(Error::Unavailable)?
        .try_get::<_, String>(0)?
        .parse()
        .map_err(|_| Error::Invalid)?,
    ))
}

pub(super) fn request_link_still_allowed(
    tx: &mut Transaction<'_>,
    record: &cs_mail_application::notices::NoticeRecord,
    clock: impl FnOnce() -> CanonicalTime,
) -> Result<(bool, CanonicalTime), StorageError> {
    let cs_mail_application::notices::NoticePurpose::RequestLink { member, .. } = record.purpose
    else {
        return Err(Error::Invalid.into());
    };
    let deployment = tx
        .query_one("SELECT record FROM cs_deployment WHERE singleton", &[])?
        .try_get::<_, Json<DeploymentBootstrap>>(0)?
        .0;
    let account = member_account(tx, member)?;
    let control = authority(tx, account, member)?;
    let r = mailbox_relationship(
        tx,
        record
            .recipient
            .synthetic_protocol_identity(&deployment.record().deployment_domain),
        member,
    )?;
    let billing = billing::identity_account(tx, member)?;
    let now = clock();
    let route = cs_mail_application::correspondence::external::new_mail_route(
        r.as_ref().map(|(_, r)| r.state),
        control.control.allows_service() && billing.covers(now),
    );
    Ok((
        matches!(
            route,
            Ok(cs_mail_application::correspondence::external::NewMailRoute::RequestLink)
        ),
        now,
    ))
}
fn member_key(
    tx: &mut Transaction<'_>,
    member: ProtocolIdentity,
) -> Result<EndpointPublicKey, StorageError> {
    Ok(members::current_endpoint_key(tx, member, false)?
        .ok_or(Error::Unavailable)?
        .key)
}
impl Repository {
    pub fn scope(&self) -> cs_mail_security::RecipientSigningScope {
        cs_mail_security::RecipientSigningScope {
            deployment_domain: self.deployment.record().deployment_domain,
            provider: self.deployment.record().provider,
        }
    }
    /// Public encryption/routing information only; this performs no canonical writes.
    /// # Errors
    /// Unknown member or unavailable content key.
    pub fn prepare(
        &self,
        member: ProtocolIdentity,
        mailbox: &GuestMailbox,
    ) -> Result<Preparation, StorageError> {
        let address = RelationshipAddress::derive(
            &self.deriver,
            &self.scope().deployment_domain,
            mailbox.synthetic_protocol_identity(&self.scope().deployment_domain),
            member,
        )?;
        let mut db = self
            .accounts
            .client
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        let member_key = member_key(&mut tx, member)?;
        tx.commit()?;
        Ok(Preparation {
            relationship: address.public(),
            relay_key: self.relay_key,
            member_key,
        })
    }
    fn open_relationship(
        &self,
        tx: &mut Transaction<'_>,
        member: ProtocolIdentity,
        mailbox: &GuestMailbox,
        now: CanonicalTime,
    ) -> Result<String, StorageError> {
        let deployment = self.scope().deployment_domain;
        let guest = mailbox.synthetic_protocol_identity(&deployment);
        guests::register_subject(tx, guest, &SenderSubject::Guest(mailbox.clone()), now)?;
        let address = RelationshipAddress::derive(&self.deriver, &deployment, guest, member)?;
        let evidence = HistoryEvidence::Mailbox(mailbox.clone());
        let history = guests::linked_history(tx, member, &evidence)?.map_or_else(
            || {
                self.deriver
                    .guest_request_history_ref(1, &deployment, guest, member)
            },
            |h| h.reference,
        );
        bootstrap_in(
            tx,
            address.aggregate_key(),
            &address.initial_state(history, now),
            self.deployment.record().unit,
        )?;
        tx.execute("INSERT INTO cs_guest_email_bindings(aggregate_key,binding) VALUES($1,$2) ON CONFLICT DO NOTHING",
            &[&address.aggregate_key(), &Json(address.public())])?;
        let history_bytes = super::scoped_key(history.derivation_version(), history.as_bytes());
        let history_text = history_bytes.iter().fold(String::new(), |mut s, b| {
            write!(s, "{b:02x}").expect("writing a string");
            s
        });
        tx.execute("INSERT INTO cs_guest_history_links(recipient,evidence,history,linked_at) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
            &[&member.0.to_string(), &evidence.key(), &history_text, &super::to_i64(now.0)?])?;
        Ok(address.aggregate_key().to_owned())
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Preparation {
    pub relationship: cs_mail_application::members::RelationshipHandle,
    pub relay_key: EndpointPublicKey,
    pub member_key: EndpointPublicKey,
}
impl Store for Repository {
    type Error = StorageError;
    fn transact(
        &self,
        request: &SignedCommand,
        decide: impl FnOnce(Context, &dyn AccountClock) -> Result<Decision, StorageError>,
    ) -> Result<Response, StorageError> {
        let mut db = self
            .accounts
            .client
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        ingress::require_drained(&mut tx)?;
        let authority = authority(&mut tx, request.account, request.persona)?;
        request.authenticate(&authority, self.scope(), self.accounts.clock.now())?;
        let prior = tx
            .query_opt(
                "SELECT receipt FROM cs_external_receipts WHERE account=$1 AND operation=$2",
                &[
                    &request.account.0.to_string(),
                    &request.operation.0.to_string(),
                ],
            )?
            .map(|r| r.try_get::<_, Json<Receipt>>(0).map(|j| j.0))
            .transpose()?;
        // Authenticate and reject altered retries before any start preparation writes.
        if let Some(prior) = &prior {
            return Ok(request.replay(prior)?);
        }
        tx.batch_execute("SAVEPOINT external_changes")?;
        let existing = conversation(&mut tx, request.conversation)?;
        let key = match &existing {
            Some((c, key, account))
                if c.member() == request.persona && *account == request.account =>
            {
                key.clone()
            }
            Some(_) => return Err(Error::Unauthorized.into()),
            None => match &request.command {
                Command::Start { mailbox, .. } => self.open_relationship(
                    &mut tx,
                    request.persona,
                    mailbox,
                    self.accounts.clock.now(),
                )?,
                _ => return Err(Error::Unavailable.into()),
            },
        };
        let relationship = relationship(&mut tx, &key)?;
        let mut messages = vec![];
        if let Command::Read { after } = request.command {
            for row in tx.query("SELECT m.sequence,m.record,d.state FROM cs_external_messages m LEFT JOIN cs_external_deliveries d ON d.message=m.id WHERE m.conversation=$1 AND m.sequence>$2 ORDER BY m.sequence LIMIT 100",
                &[&request.conversation.value().to_string(), &super::to_i64(after)?])? {
                messages.push(MessageView { sequence: super::to_u64(row.try_get(0)?)?, message: row.try_get::<_, Json<Message>>(1)?.0,
                    delivery: row.try_get::<_, Option<Json<DeliveryState>>>(2)?.map(|j| j.0),
                    provider_events: tx.query("SELECT record FROM cs_external_provider_events WHERE message=$1 ORDER BY received_at,id", &[&row.try_get::<_, Json<Message>>(1)?.0.manifest.message().0.to_string()])?.into_iter().map(|r| r.try_get::<_, Json<cs_mail_correspondence::external::delivery::ProviderEvent>>(0).map(|j| j.0)).collect::<Result<Vec<_>, _>>()? });
            }
        }
        let context = Context {
            scope: self.scope(),
            authority,
            billing: billing::identity_account(&mut tx, request.persona)?,
            relationship,
            conversation: existing.map(|(c, _, _)| c),
            relay_key: self.relay_key,
            member_key: member_key(&mut tx, request.persona)?,
            prior,
            messages,
        };
        let Decision { response, effects } = decide(context, &*self.accounts.clock)?;
        if let Some(e) = effects {
            let id = request.conversation.value().to_string();
            if let Some(changes) = e.changes {
                persist_changes(&mut tx, request, &key, changes, e.receipt.at)?;
            } else {
                // A refusal commits only its receipt, including for a never-created conversation.
                tx.batch_execute("ROLLBACK TO SAVEPOINT external_changes")?;
            }
            tx.execute("INSERT INTO cs_external_receipts(account,operation,conversation,receipt) VALUES($1,$2,$3,$4)",
                &[&request.account.0.to_string(),&request.operation.0.to_string(),&id,&Json(e.receipt)])?;
        }
        tx.commit()?;
        Ok(response)
    }
}

fn persist_changes(
    tx: &mut Transaction<'_>,
    request: &SignedCommand,
    key: &str,
    changes: cs_mail_application::correspondence::external::Changes,
    at: CanonicalTime,
) -> Result<(), StorageError> {
    let id = request.conversation.value().to_string();
    tx.execute("INSERT INTO cs_conversation_routes(id,route) VALUES($1,'external') ON CONFLICT(id) DO NOTHING", &[&id])?;
    tx.execute("INSERT INTO cs_external_conversations(id,member,account,aggregate_key,record) VALUES($1,$2,$3,$4,$5) ON CONFLICT(id) DO UPDATE SET record=EXCLUDED.record",
                &[&id,&request.persona.0.to_string(),&request.account.0.to_string(),&key,&Json(changes.conversation)])?;
    if let Some(p) = changes.delivery {
        let message = Message {
            manifest: p.manifest,
            sender_key: p.sender_key,
            member_copy: p.member_copy,
            direction: Direction::MemberToGuest,
            committed_at: at,
        };
        tx.execute(
            "INSERT INTO cs_external_messages(id,conversation,record) VALUES($1,$2,$3)",
            &[
                &message.manifest.message().0.to_string(),
                &id,
                &Json(&message),
            ],
        )?;
        tx.execute("INSERT INTO cs_external_deliveries(message,account,operational_key,relay_copy,state) VALUES($1,$2,$3,$4,$5)",
                    &[&message.manifest.message().0.to_string(),&request.account.0.to_string(),&request.key.0.to_string(),&Json(p.relay_copy),&Json(DeliveryState::Waiting)])?;
    }
    if changes.cancel_waiting {
        for row in tx.query("SELECT d.message,d.state FROM cs_external_deliveries d JOIN cs_external_messages m ON m.id=d.message WHERE m.conversation=$1 FOR UPDATE OF d", &[&id])? {
                    let old = row.try_get::<_, Json<DeliveryState>>(1)?.0;
                    tx.execute("UPDATE cs_external_deliveries SET state=$2 WHERE message=$1", &[&row.try_get::<_, String>(0)?, &Json(old.cancel(at))])?;
                }
    }
    Ok(())
}

/// Incoming processing does not hold signing, relay-decryption, or derivation secrets.
pub struct IncomingRepository {
    pub accounts: PostgresAccountRepository,
    pub scope: cs_mail_security::RecipientSigningScope,
}
impl Repository {
    pub fn incoming(&self) -> IncomingRepository {
        IncomingRepository {
            accounts: self.accounts.clone(),
            scope: self.scope(),
        }
    }
}
impl IncomingRepository {
    /// A sealed DMARC pass may queue a bounded instruction notice, never admit
    /// unknown-sender content. The original text is not retained.
    /// # Errors
    /// Refuses automatic/bounce mail, changed retries, blocks and revoked access.
    pub fn queue_request_link(
        &self,
        mail: &cs_mail_smtp_gateway::incoming::VerifiedMail,
        secret: &[u8; 32],
    ) -> Result<(), StorageError> {
        use cs_mail_application::{
            correspondence::external::{NewMailRoute, new_mail_route},
            notices::{NoticePurpose, NoticeRecord},
        };
        let cs_mail_smtp_gateway::incoming::MailDestination::Member(member) = mail.destination()
        else {
            return Err(Error::Unauthorized.into());
        };
        if !mail.may_notify() || *secret == [0; 32] {
            return Err(Error::Unauthorized.into());
        }
        let (identity, digest) = mail.fingerprints(&self.scope.deployment_domain, secret);
        let mut db = self
            .accounts
            .client
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        ingress::require_drained(&mut tx)?;
        if let Some(row) = tx.query_opt(
            "SELECT content_digest FROM cs_request_link_receipts WHERE identity=$1",
            &[&identity.bytes().as_slice()],
        )? {
            return if row.try_get::<_, Vec<u8>>(0)? == digest {
                Ok(())
            } else {
                Err(Error::Conflict.into())
            };
        }
        if tx
            .query_opt(
                "SELECT 1 FROM cs_external_messages WHERE id=$1",
                &[&identity.message_id().0.to_string()],
            )?
            .is_some()
        {
            // Admission won a race with this preparation. Retry through its receipt.
            return Err(StorageError::VersionConflict);
        }
        let account = member_account(&mut tx, member)?;
        let control = authority(&mut tx, account, member)?;
        let r = mailbox_relationship(
            &mut tx,
            mail.mailbox()
                .synthetic_protocol_identity(&self.scope.deployment_domain),
            member,
        )?;
        let billing = billing::identity_account(&mut tx, member)?;
        // One instruction notice per mailbox/member per day bounds amplification.
        let last = tx.query_one("SELECT max((record->>'created_at')::bigint) FROM cs_mail_notices WHERE record->>'recipient'=$1 AND record#>>'{purpose,RequestLink,member}'=$2",
            &[&mail.mailbox().address(), &member.0.to_string()])?.try_get::<_, Option<i64>>(0)?
            .map(|n| super::to_u64(n).map(CanonicalTime)).transpose()?;
        let now = self.accounts.clock.now();
        if now < mail.authentication().evaluated_at
            || now.0.saturating_sub(mail.authentication().evaluated_at.0) > 300_000
        {
            return Err(Error::Unauthorized.into());
        }
        if new_mail_route(
            r.as_ref().map(|(_, r)| r.state),
            control.control.allows_service() && billing.covers(now),
        )? != NewMailRoute::RequestLink
        {
            return Err(StorageError::VersionConflict);
        }
        let record = NoticeRecord::new(
            &self.scope.deployment_domain,
            mail.mailbox().clone(),
            NoticePurpose::RequestLink {
                member,
                source: identity.bytes(),
            },
            now,
        )?;
        let notice = if cs_mail_application::notices::request_link_due(last, now) {
            tx.execute(
                "INSERT INTO cs_mail_notices(id,record,state) VALUES($1,$2,$3)",
                &[
                    &record.id.value().to_string(),
                    &Json(&record),
                    &Json(DeliveryState::Waiting),
                ],
            )?;
            Some(record.id.value().to_string())
        } else {
            None
        };
        tx.execute(
            "INSERT INTO cs_request_link_receipts(identity,content_digest,notice) VALUES($1,$2,$3)",
            &[&identity.bytes().as_slice(), &digest.as_slice(), &notice],
        )?;
        tx.commit()?;
        Ok(())
    }
    /// Accepts only a verifier-produced, encrypted reply. Raw evidence and independently
    /// prepared ciphertext cannot be substituted at this persistence boundary.
    /// # Errors
    /// Refuses altered redelivery, unauthorized replies and partial writes.
    #[allow(clippy::too_many_lines)] // One protected atomic admission, including new conversation creation.
    pub fn receive(&self, authenticated: &AuthenticatedReply) -> Result<MessageId, StorageError> {
        let reply = Reply {
            mailbox: authenticated.mailbox().clone(),
            conversation: authenticated.conversation(),
            message: Message {
                manifest: authenticated.manifest().clone(),
                sender_key: authenticated.sender_key(),
                member_copy: authenticated.member_copy().clone(),
                direction: Direction::GuestToMember,
                committed_at: authenticated.member_copy().created_at,
            },
            delivery_digest: authenticated.delivery_digest(),
            evidence: IncomingEvidence {
                authentication: authenticated.authentication().clone(),
                transport_digest: authenticated.transport_digest(),
                message_digest: authenticated.raw_digest(),
            },
        };
        let mut db = self
            .accounts
            .client
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        ingress::require_drained(&mut tx)?;
        if let Some(row) = tx.query_opt(
            "SELECT content_digest FROM cs_request_link_receipts WHERE identity=$1",
            &[&authenticated.arrival().bytes().as_slice()],
        )? {
            // A prompt won a race with preparation: no inbox effect may follow it.
            return Err(
                if row.try_get::<_, Vec<u8>>(0)? == authenticated.delivery_digest() {
                    StorageError::VersionConflict
                } else {
                    Error::Conflict.into()
                },
            );
        }
        let (c, key, account, create) =
            if let Some((c, key, account)) = conversation(&mut tx, reply.conversation)? {
                (c, key, account, false)
            } else {
                let cs_mail_smtp_gateway::incoming::MailDestination::Member(member) =
                    authenticated.destination()
                else {
                    return Err(Error::Unavailable.into());
                };
                let account = member_account(&mut tx, member)?;
                let (key, r) = mailbox_relationship(
                    &mut tx,
                    reply
                        .mailbox
                        .synthetic_protocol_identity(&self.scope.deployment_domain),
                    member,
                )?
                .ok_or(Error::Unauthorized)?;
                let c = ExternalConversation::inbound(
                    reply.conversation,
                    member,
                    reply.mailbox.clone(),
                    self.scope.deployment_domain,
                    &r,
                    self.accounts.clock.now(),
                )?;
                (c, key, account, true)
            };
        let state = authority(&mut tx, account, c.member())?;
        if !state.control.allows_service() {
            return Err(Error::Unauthorized.into());
        }
        let r = relationship(&mut tx, &key)?;
        let id = reply.message.manifest.message();
        let digest = reply.delivery_digest;
        if let Some(row) = tx.query_opt(
            "SELECT inbound_digest FROM cs_external_messages WHERE id=$1",
            &[&id.0.to_string()],
        )? {
            if row.try_get::<_, Option<Vec<u8>>>(0)?.as_deref() != Some(digest.as_slice()) {
                return Err(Error::Conflict.into());
            }
            // Historical admission receipt only, never another delivery or lane use.
            return Ok(id);
        }
        let lane = tx
            .query_opt(
                "SELECT lane FROM cs_capability_lanes WHERE aggregate_key=$1 FOR UPDATE",
                &[&key],
            )?
            .map(|row| {
                row.try_get::<_, Json<cs_mail_capabilities::Lane>>(0)
                    .map(|j| j.0)
            })
            .transpose()?;
        let member_key = member_key(&mut tx, c.member())?;
        if reply.message.member_copy.envelope.recipient_key != member_key.reference {
            return Err(StorageError::VersionConflict);
        }
        let billing = billing::identity_account(&mut tx, c.member())?;
        let now = self.accounts.clock.now();
        let next_lane = admit_reply(
            ReplyContext {
                conversation: &c,
                relationship: &r,
                lane,
                member_key,
                billing: &billing,
                provider: self.scope.provider,
            },
            &reply,
            now,
        )?;
        let mut message = reply.message.clone();
        message.committed_at = now;
        if create {
            let id = c.id().value().to_string();
            tx.execute(
                "INSERT INTO cs_conversation_routes(id,route) VALUES($1,'external')",
                &[&id],
            )?;
            tx.execute("INSERT INTO cs_external_conversations(id,member,account,aggregate_key,record) VALUES($1,$2,$3,$4,$5)",
                &[&id, &c.member().0.to_string(), &account.0.to_string(), &key, &Json(&c)])?;
        }
        tx.execute("INSERT INTO cs_external_messages(id,conversation,record,inbound_digest,inbound_evidence,incoming_message_id) VALUES($1,$2,$3,$4,$5,$6)",
            &[&id.0.to_string(), &reply.conversation.value().to_string(), &Json(message), &digest.as_slice(), &Json(&reply.evidence), &authenticated.rfc_message_id()])?;
        if let Some(lane) = next_lane {
            super::upsert_lane(&mut tx, &key, &lane, now)?;
            super::persist_schedules(&mut tx, &key, &[lane.schedule_change()])?;
        }
        tx.commit()?;
        Ok(id)
    }
}
impl Repository {
    /// Atomically rechecks consent and authority and crosses the dispatch boundary.
    /// A returned capability is released only after commit. A crash after this point
    /// leaves `DispatchBegun` (outcome unknown), never automatically retryable work.
    /// # Errors
    /// Refuses duplicate dispatch and returns database failures.
    pub fn begin_dispatch(&self, id: MessageId) -> Result<Option<Dispatch>, StorageError> {
        let mut db = self
            .accounts
            .client
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        ingress::require_drained(&mut tx)?;
        let row = tx.query_opt("SELECT m.conversation,m.record,d.account,d.operational_key,d.relay_copy,d.state FROM cs_external_messages m JOIN cs_external_deliveries d ON d.message=m.id WHERE m.id=$1 FOR UPDATE OF d", &[&id.0.to_string()])?.ok_or(Error::Unavailable)?;
        let conversation_id = ConversationId::new(
            row.try_get::<_, String>(0)?
                .parse()
                .map_err(|_| Error::Invalid)?,
        )?;
        let (c, key, account) =
            conversation(&mut tx, conversation_id)?.ok_or(Error::Unavailable)?;
        if row.try_get::<_, String>(2)? != account.0.to_string() {
            return Err(Error::Invalid.into());
        }
        let authority = authority(&mut tx, account, c.member())?;
        let relationship = relationship(&mut tx, &key)?;
        let billing = billing::identity_account(&mut tx, c.member())?;
        let root = tx.query_one("SELECT id,incoming_message_id FROM cs_external_messages WHERE conversation=$1 ORDER BY sequence LIMIT 1", &[&conversation_id.value().to_string()])?;
        let context = DispatchContext {
            thread_root: match root.try_get::<_, Option<String>>(1)? {
                Some(id) => cs_mail_application::correspondence::external::ThreadRoot::Incoming(id),
                None => {
                    cs_mail_application::correspondence::external::ThreadRoot::Member(MessageId(
                        root.try_get::<_, String>(0)?
                            .parse()
                            .map_err(|_| Error::Invalid)?,
                    ))
                }
            },
            conversation: c,
            relationship,
            authority,
            billing,
            key: cs_mail_primitives::OperationalKeyRef(
                row.try_get::<_, String>(3)?
                    .parse()
                    .map_err(|_| Error::Invalid)?,
            ),
            message: row.try_get::<_, Json<Message>>(1)?.0,
            relay_copy: row.try_get::<_, Json<_>>(4)?.0,
            relay_key: self.relay_key,
            state: row.try_get::<_, Json<_>>(5)?.0,
        };
        if context.message.manifest.message() != id {
            return Err(Error::Invalid.into());
        }
        let (next, dispatch) = begin_dispatch(context, self.accounts.clock.now())?;
        tx.execute(
            "UPDATE cs_external_deliveries SET state=$2 WHERE message=$1",
            &[&id.0.to_string(), &Json(next)],
        )?;
        tx.commit()?;
        Ok(dispatch)
    }
    /// Records or reconciles an external handoff. `None` means definite failure;
    /// an unknown outcome must remain `DispatchBegun` for reconciliation.
    /// # Errors
    /// Conflicting results, malformed references, or database errors.
    pub fn finish_dispatch(
        &self,
        id: MessageId,
        accepted: Option<cs_mail_correspondence::external::delivery::ProviderReference>,
    ) -> Result<(), StorageError> {
        let mut db = self
            .accounts
            .client
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        let row = tx
            .query_opt(
                "SELECT state FROM cs_external_deliveries WHERE message=$1 FOR UPDATE",
                &[&id.0.to_string()],
            )?
            .ok_or(Error::Unavailable)?;
        let state = row.try_get::<_, Json<DeliveryState>>(0)?.0;
        if let Some(reference) = &accepted {
            super::notices::claim_provider_identity(
                &mut tx,
                "reference",
                reference.as_str(),
                super::notices::MailTarget::Message(id),
            )?;
        }
        let next = state.finish(accepted, self.accounts.clock.now())?;
        tx.execute(
            "UPDATE cs_external_deliveries SET state=$2 WHERE message=$1",
            &[&id.0.to_string(), &Json(next)],
        )?;
        tx.commit()?;
        Ok(())
    }
}

impl Repository {
    /// Bounded candidate list; dispatch authorization is rechecked under protection.
    /// # Errors
    /// Invalid batch size or database failure.
    pub fn waiting(&self, limit: i64) -> Result<Vec<MessageId>, StorageError> {
        if !(1..=1000).contains(&limit) {
            return Err(Error::Invalid.into());
        }
        let mut db = self
            .accounts
            .client
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        db.query("SELECT message FROM cs_external_deliveries WHERE state=to_jsonb('Waiting'::text) ORDER BY message LIMIT $1", &[&limit])?
            .into_iter().map(|r| Ok(MessageId(r.try_get::<_, String>(0)?.parse().map_err(|_| Error::Invalid)?))).collect()
    }
}

impl super::PostgresDeployment {
    /// # Errors
    /// Returns malformed identifiers or storage failures. The service authenticates
    /// the participant before exposing this relationship's conversation index.
    pub fn external_conversations_for(
        &self,
        key: &str,
    ) -> Result<Vec<ConversationId>, StorageError> {
        self.client
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?
            .query(
                "SELECT id FROM cs_external_conversations WHERE aggregate_key=$1 ORDER BY id",
                &[&key],
            )?
            .into_iter()
            .map(|row| {
                ConversationId::new(
                    row.try_get::<_, String>(0)?
                        .parse()
                        .map_err(|_| Error::Invalid)?,
                )
                .map_err(Into::into)
            })
            .collect()
    }
    /// Evidence for the application-owned recipient visibility rule.
    /// # Errors
    /// Database failure.
    pub fn has_external_invitation(
        &self,
        sender: ProtocolIdentity,
        member: ProtocolIdentity,
    ) -> Result<bool, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        Ok(db.query_opt("SELECT 1 FROM cs_external_conversations c JOIN cs_relationship_aggregates r ON r.aggregate_key=c.aggregate_key WHERE c.member=$1 AND r.relationship_state#>>'{relationship,key,sender}'=$2 LIMIT 1", &[&member.0.to_string(), &sender.0.to_string()])?.is_some())
    }
}

pub struct IncomingPreparation {
    pub conversation: ExternalConversation,
    pub member_key: EndpointPublicKey,
    pub relationship: cs_mail_application::members::RelationshipHandle,
    pub lane: Option<cs_mail_primitives::LaneId>,
}

pub(super) fn paid_conversation_in(
    tx: &mut Transaction<'_>,
    notice: &cs_mail_application::notices::NoticeRecord,
) -> Result<(), StorageError> {
    use cs_mail_application::notices::{NoticePurpose, paid_conversation};
    let Some(id) = paid_conversation(notice) else {
        return Ok(());
    };
    let NoticePurpose::Outcome {
        relationship: key,
        member,
        notice: outcome,
    } = &notice.purpose
    else {
        return Err(Error::Invalid.into());
    };
    let request = tx
        .query_opt(
            "SELECT request FROM cs_requests WHERE aggregate_key=$1 AND request_id=$2",
            &[key, &outcome.request.0.to_string()],
        )?
        .ok_or(Error::Unavailable)?
        .try_get::<_, Json<cs_mail_protocol::RelationshipRequest>>(0)?
        .0;
    if !matches!(
        request.lifecycle,
        cs_mail_protocol::RequestLifecycle::Accepted { .. }
    ) {
        return Err(Error::Unauthorized.into());
    }
    let deployment = tx
        .query_one("SELECT record FROM cs_deployment WHERE singleton", &[])?
        .try_get::<_, Json<DeploymentBootstrap>>(0)?
        .0;
    let relationship = relationship(tx, key)?;
    let conversation = ExternalConversation::inbound(
        id,
        *member,
        notice.recipient.clone(),
        deployment.record().deployment_domain,
        &relationship,
        notice.created_at,
    )?;
    let account: String = tx
        .query_one(
            "SELECT account FROM cs_persona_owners WHERE identity=$1",
            &[&member.0.to_string()],
        )?
        .try_get(0)?;
    tx.execute(
        "INSERT INTO cs_conversation_routes(id,route) VALUES($1,'external')",
        &[&id.value().to_string()],
    )?;
    tx.execute("INSERT INTO cs_external_conversations(id,member,account,aggregate_key,record) VALUES($1,$2,$3,$4,$5)",
        &[&id.value().to_string(), &member.0.to_string(), &account, key, &Json(conversation)])?;
    tx.execute("INSERT INTO cs_guest_request_conversations(aggregate_key,request_id,conversation) VALUES($1,$2,$3)",
        &[key, &outcome.request.0.to_string(), &id.value().to_string()])?;
    Ok(())
}
impl IncomingRepository {
    /// Preparation is only a hint; both admission and request-link queuing recheck
    /// protected current state before committing.
    /// # Errors
    /// Unknown member, blocked/revoked sender, unavailable coverage, or storage failure.
    pub fn prepare_mail(
        &self,
        mail: &cs_mail_smtp_gateway::incoming::VerifiedMail,
        secret: &[u8; 32],
    ) -> Result<Option<IncomingPreparation>, StorageError> {
        use cs_mail_smtp_gateway::incoming::MailDestination;
        let MailDestination::Member(member) = mail.destination() else {
            return self
                .prepare(
                    mail.conversation_id(&self.scope.deployment_domain, secret),
                    mail.mailbox(),
                )
                .map(Some);
        };
        let id = mail.conversation_id(&self.scope.deployment_domain, secret);
        // A replay keeps its original route. The receive boundary returns the old
        // receipt only for the same authenticated content.
        {
            let mut db = self
                .accounts
                .client
                .lock()
                .map_err(|_| StorageError::LockPoisoned)?;
            let mut tx = db.transaction()?;
            let (arrival, digest) = mail.fingerprints(&self.scope.deployment_domain, secret);
            if let Some(row) = tx.query_opt(
                "SELECT content_digest FROM cs_request_link_receipts WHERE identity=$1",
                &[&arrival.bytes().as_slice()],
            )? {
                return if row.try_get::<_, Vec<u8>>(0)? == digest {
                    Ok(None)
                } else {
                    Err(Error::Conflict.into())
                };
            }
            if conversation(&mut tx, id)?.is_some() {
                drop(tx);
                drop(db);
                return self.prepare(id, mail.mailbox()).map(Some);
            }
        }
        let mut db = self
            .accounts
            .client
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        let account = member_account(&mut tx, member)?;
        let control = authority(&mut tx, account, member)?;
        let r = mailbox_relationship(
            &mut tx,
            mail.mailbox()
                .synthetic_protocol_identity(&self.scope.deployment_domain),
            member,
        )?;
        let billing = billing::identity_account(&mut tx, member)?;
        let now = self.accounts.clock.now();
        let route = cs_mail_application::correspondence::external::new_mail_route(
            r.as_ref().map(|(_, r)| r.state),
            control.control.allows_service() && billing.covers(now),
        )?;
        if route == cs_mail_application::correspondence::external::NewMailRoute::RequestLink {
            return Ok(None);
        }
        let (key, r) = r.ok_or(Error::Unavailable)?;
        let handle = tx
            .query_one(
                "SELECT binding FROM cs_guest_email_bindings WHERE aggregate_key=$1",
                &[&key],
            )?
            .try_get::<_, Json<cs_mail_application::members::RelationshipHandle>>(0)?
            .0;
        let member_key = member_key(&mut tx, member)?;
        let lane = tx
            .query_opt(
                "SELECT lane FROM cs_capability_lanes WHERE aggregate_key=$1",
                &[&key],
            )?
            .map(|r| {
                r.try_get::<_, Json<cs_mail_capabilities::Lane>>(0)
                    .map(|j| j.0.grant().id)
            })
            .transpose()?;
        Ok(Some(IncomingPreparation {
            conversation: ExternalConversation::inbound(
                id,
                member,
                mail.mailbox().clone(),
                self.scope.deployment_domain,
                &r,
                now,
            )?,
            member_key,
            relationship: handle,
            lane,
        }))
    }
    /// Untrusted preparation only; admission rechecks all current authority at commit.
    /// # Errors
    /// Unknown route, mismatched mailbox, or storage failure.
    pub fn prepare(
        &self,
        id: ConversationId,
        mailbox: &GuestMailbox,
    ) -> Result<IncomingPreparation, StorageError> {
        let mut db = self
            .accounts
            .client
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        let (c, key, _) = conversation(&mut tx, id)?.ok_or(Error::Unavailable)?;
        if c.mailbox() != mailbox {
            return Err(Error::Unauthorized.into());
        }
        let member_key = member_key(&mut tx, c.member())?;
        let handle = tx
            .query_one(
                "SELECT binding FROM cs_guest_email_bindings WHERE aggregate_key=$1",
                &[&key],
            )?
            .try_get::<_, Json<cs_mail_application::members::RelationshipHandle>>(0)?
            .0;
        let lane = tx
            .query_opt(
                "SELECT lane FROM cs_capability_lanes WHERE aggregate_key=$1",
                &[&key],
            )?
            .map(|r| {
                r.try_get::<_, Json<cs_mail_capabilities::Lane>>(0)
                    .map(|j| j.0.grant().id)
            })
            .transpose()?;
        tx.commit()?;
        Ok(IncomingPreparation {
            conversation: c,
            member_key,
            relationship: handle,
            lane,
        })
    }
}

impl Repository {
    /// Atomically reconcile handoff and retain normalized provider evidence.
    /// # Errors
    /// Refuses altered event replays, mismatched messages and conflicting handoffs.
    pub fn provider_feedback(
        &self,
        event: &cs_mail_correspondence::external::delivery::ProviderEvent,
    ) -> Result<(), StorageError> {
        let mut db = self
            .accounts
            .client
            .lock()
            .map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        // Same protection order as begin_dispatch and member decisions.
        ingress::require_drained(&mut tx)?;
        if let Some(row) = tx.query_opt(
            "SELECT record FROM cs_external_provider_events WHERE id=$1",
            &[&event.id.as_str()],
        )? {
            let prior: cs_mail_correspondence::external::delivery::ProviderEvent =
                row.try_get::<_, Json<_>>(0)?.0;
            return if prior == *event {
                Ok(())
            } else {
                Err(Error::Conflict.into())
            };
        }
        let row = tx.query_opt("SELECT m.conversation,d.state FROM cs_external_messages m JOIN cs_external_deliveries d ON d.message=m.id WHERE m.id=$1 FOR UPDATE OF d", &[&event.message.0.to_string()])?.ok_or(Error::Unavailable)?;
        let conversation_id = ConversationId::new(
            row.try_get::<_, String>(0)?
                .parse()
                .map_err(|_| Error::Invalid)?,
        )?;
        let (conversation, _, _) =
            conversation(&mut tx, conversation_id)?.ok_or(Error::Unavailable)?;
        let state = row.try_get::<_, Json<DeliveryState>>(1)?.0;
        let now = self.accounts.clock.now();
        super::notices::claim_provider_identity(
            &mut tx,
            "event",
            event.id.as_str(),
            super::notices::MailTarget::Message(event.message),
        )?;
        super::notices::claim_provider_identity(
            &mut tx,
            "reference",
            event.reference.as_str(),
            super::notices::MailTarget::Message(event.message),
        )?;
        let next = cs_mail_application::correspondence::external::provider_feedback(
            &conversation,
            &state,
            event,
            now,
        )?;
        tx.execute(
            "UPDATE cs_external_deliveries SET state=$2 WHERE message=$1",
            &[&event.message.0.to_string(), &Json(next)],
        )?;
        tx.execute("INSERT INTO cs_external_provider_events(id,message,record,received_at) VALUES($1,$2,$3,$4)", &[&event.id.as_str(), &event.message.0.to_string(), &Json(event), &super::to_i64(now.0)?])?;
        tx.commit()?;
        Ok(())
    }
}
