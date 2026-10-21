//! Protected alias lifecycle and receiving-scope commits. The existing receipt
//! barrier serializes these operations with participant blocks and deliveries.
use super::{Json, PostgresAccountRepository, StorageError, Transaction, ingress};
use cs_mail_accounts::aliases::{AdmittedConversation, Alias, AliasError};
use cs_mail_application::{
    accounts::operations::AccountState,
    aliases::{Command, Context, Decision, Receipt, SignedCommand, Store},
};
use cs_mail_correspondence::ConversationId;
use cs_mail_primitives::{AliasId, CanonicalTime, ProtocolIdentity};
use std::collections::{BTreeMap, BTreeSet};
impl From<AliasError> for StorageError {
    fn from(value: AliasError) -> Self {
        Self::Alias(value)
    }
}
pub(super) fn owned_aliases(
    tx: &mut Transaction<'_>,
    owner: ProtocolIdentity,
) -> Result<BTreeMap<AliasId, Alias>, StorageError> {
    let mut result = BTreeMap::new();
    for row in tx.query(
        "SELECT id,address,record FROM cs_alias_routes WHERE owner=$1 ORDER BY id FOR UPDATE",
        &[&owner.0.to_string()],
    )? {
        let alias = row.try_get::<_, Json<Alias>>(2)?.0;
        if alias.owner() != owner
            || row.try_get::<_, String>(0)? != alias.id().value().to_string()
            || row.try_get::<_, String>(1)? != alias.address().as_str()
        {
            return Err(StorageError::NumericRange);
        }
        result.insert(alias.id(), alias);
    }
    Ok(result)
}
impl Store for PostgresAccountRepository {
    type Error = StorageError;
    fn transact_alias(
        &self,
        request: &SignedCommand,
        decide: impl FnOnce(
            Context,
            &dyn cs_mail_application::accounts::AccountClock,
        ) -> Result<Decision, StorageError>,
    ) -> Result<Receipt, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        ingress::require_drained(&mut tx)?;
        let account = request.account.0.to_string();
        let owner = request.owner.0.to_string();
        let row = tx.query_opt("SELECT a.control,a.registry,a.product,e.persona FROM cs_product_accounts a JOIN cs_persona_owners p ON p.account=a.id JOIN cs_enrollment_operations e ON e.account=a.id WHERE a.id=$1 AND p.identity=$2 FOR UPDATE OF a", &[&account, &owner])?.ok_or(AliasError::Permission)?;
        let participant = ProtocolIdentity(
            row.try_get::<_, String>(3)?
                .parse()
                .map_err(|_| StorageError::NumericRange)?,
        );
        let authority = AccountState {
            control: row.try_get::<_, Json<_>>(0)?.0,
            registry: row.try_get::<_, Json<_>>(1)?.0,
            product: row.try_get(2)?,
        };
        let prior = tx
            .query_opt(
                "SELECT request,receipt FROM cs_alias_commands WHERE account=$1 AND operation=$2",
                &[&account, &request.operation.0.to_string()],
            )?
            .map(|row| {
                let original = row.try_get::<_, Json<SignedCommand>>(0)?.0;
                let receipt = row.try_get::<_, Json<Receipt>>(1)?.0;
                if original.digest()? != receipt.digest || !receipt.matches(&original) {
                    return Err(StorageError::NumericRange);
                }
                Ok(receipt)
            })
            .transpose()?;
        let aliases = owned_aliases(&mut tx, request.owner)?;
        let mut reserved_addresses = BTreeSet::new();
        let mut reserved_ids = BTreeSet::new();
        for row in tx.query("SELECT id,address FROM cs_alias_routes", &[])? {
            reserved_ids.insert(
                AliasId::new(
                    row.try_get::<_, String>(0)?
                        .parse()
                        .map_err(|_| StorageError::NumericRange)?,
                )
                .map_err(|_| StorageError::NumericRange)?,
            );
            reserved_addresses.insert(row.try_get(1)?);
        }
        for row in tx.query(
            "SELECT mailbox FROM cs_address_directory WHERE mailbox IS NOT NULL",
            &[],
        )? {
            reserved_addresses.insert(row.try_get(0)?);
        }
        let existing = admitted_conversations(&mut tx, &owner)?;
        let (relationship, lane) = if let Command::ScopeSender { sender, .. }
        | Command::ApproveConversation { sender, .. } =
            request.command
        {
            participant_permission(&mut tx, sender, request.owner)?
        } else {
            (None, None)
        };
        let sender_scope = if let Command::ScopeSender { sender, .. } = request.command {
            tx.query_opt("SELECT record FROM cs_alias_sender_scopes WHERE recipient=$1 AND sender=$2 FOR UPDATE", &[&owner, &sender.0.to_string()])?.map(|row| row.try_get::<_, Json<cs_mail_application::aliases::SenderRouteScope>>(0).map(|j| j.0)).transpose()?
        } else {
            None
        };
        let (receipt, effects) = decide(
            Context {
                authority,
                participant,
                aliases,
                reserved_addresses,
                reserved_ids,
                existing,
                relationship,
                lane,
                prior,
                sender_scope,
            },
            &*self.clock,
        )?
        .into_parts();
        if let Some(effects) = effects {
            let (aliases, scope, recorded) = effects.into_parts();
            for alias in aliases {
                if tx.execute("INSERT INTO cs_alias_routes(id,owner,address,record) VALUES($1,$2,$3,$4) ON CONFLICT(id) DO UPDATE SET record=EXCLUDED.record WHERE cs_alias_routes.owner=EXCLUDED.owner AND cs_alias_routes.address=EXCLUDED.address", &[&alias.id().value().to_string(),&owner,&alias.address().as_str(),&Json(&alias)])? != 1 { return Err(StorageError::VersionConflict); }
            }
            if let Some(scope) = scope {
                tx.execute("INSERT INTO cs_alias_sender_scopes(recipient,sender,record) VALUES($1,$2,$3) ON CONFLICT(recipient,sender) DO UPDATE SET record=EXCLUDED.record", &[&owner,&scope.sender().0.to_string(),&Json(scope)])?;
            }
            tx.execute(
                "INSERT INTO cs_alias_commands(account,operation,request,receipt) VALUES($1,$2,$3,$4)",
                &[&account, &request.operation.0.to_string(), &Json(request), &Json(recorded)],
            )?;
        }
        tx.commit()?;
        Ok(receipt)
    }
}

fn admitted_conversations(
    tx: &mut Transaction<'_>,
    owner: &str,
) -> Result<Vec<AdmittedConversation>, StorageError> {
    let mut existing = vec![];
    // Earliest admitted evidence per exact conversation/sender/receiving alias.
    for row in tx.query("SELECT d.alias,d.conversation,d.sender,min(d.admitted_at) FROM cs_alias_deliveries d JOIN cs_alias_routes a ON a.id=d.alias WHERE a.owner=$1 AND NOT (d.evidence->'provenance' ? 'DevelopmentAssignment') GROUP BY d.alias,d.conversation,d.sender", &[&owner])? {
            existing.push(AdmittedConversation::new(
                AliasId::new(row.try_get::<_, String>(0)?.parse().map_err(|_| StorageError::NumericRange)?).map_err(|_| StorageError::NumericRange)?,
                ConversationId::new(row.try_get::<_, String>(1)?.parse().map_err(|_| StorageError::NumericRange)?).map_err(StorageError::Correspondence)?,
                ProtocolIdentity(row.try_get::<_, String>(2)?.parse().map_err(|_| StorageError::NumericRange)?),
                CanonicalTime(super::to_u64(row.try_get(3)?)?),
            )?);
        }
    Ok(existing)
}

fn participant_permission(
    tx: &mut Transaction<'_>,
    sender: ProtocolIdentity,
    recipient: ProtocolIdentity,
) -> Result<
    (
        Option<cs_mail_protocol::Relationship>,
        Option<cs_mail_capabilities::SenderScopedLane>,
    ),
    StorageError,
> {
    let mut found = None;
    for row in tx.query(
                "SELECT aggregate_key,relationship_state FROM cs_relationship_aggregates WHERE relationship_state#>>'{relationship,key,sender}'=$1 AND relationship_state#>>'{relationship,key,recipient}'=$2 FOR UPDATE",
                &[&sender.0.to_string(), &recipient.0.to_string()],
            )? {
                let state = row
                    .try_get::<_, Json<super::domain::RelationshipRecord>>(1)?
                    .0
                    .into_state();
                if state.relationship.key.sender == sender
                    && state.relationship.key.recipient == recipient
                {
                    if found.is_some() {
                        return Err(StorageError::NumericRange);
                    }
                    found = Some((row.try_get::<_, String>(0)?, state.relationship));
                }
            }
    if let Some((key, relationship)) = found {
        let lane = ingress::load_current_lane(tx, &key)?;
        Ok((Some(relationship), lane))
    } else {
        Ok((None, None))
    }
}

pub(super) fn receiving_context(
    tx: &mut Transaction<'_>,
    id: AliasId,
) -> Result<Option<Alias>, StorageError> {
    let Some(row) = tx.query_opt(
        "SELECT owner,address,record FROM cs_alias_routes WHERE id=$1 FOR UPDATE",
        &[&id.value().to_string()],
    )?
    else {
        return Ok(None);
    };
    let alias = row.try_get::<_, Json<Alias>>(2)?.0;
    if alias.id() != id
        || row.try_get::<_, String>(0)? != alias.owner().0.to_string()
        || row.try_get::<_, String>(1)? != alias.address().as_str()
    {
        return Err(StorageError::NumericRange);
    }
    Ok(Some(alias))
}
pub(super) fn sender_scope(
    tx: &mut Transaction<'_>,
    sender: ProtocolIdentity,
    recipient: ProtocolIdentity,
) -> Result<Option<cs_mail_application::aliases::SenderRouteScope>, StorageError> {
    tx.query_opt(
        "SELECT record FROM cs_alias_sender_scopes WHERE recipient=$1 AND sender=$2 FOR UPDATE",
        &[&recipient.0.to_string(), &sender.0.to_string()],
    )?
    .map(|r| r.try_get::<_, Json<_>>(0).map(|j| j.0))
    .transpose()
    .map_err(Into::into)
}
pub(super) fn delivery_routes(
    tx: &mut Transaction<'_>,
    owner: ProtocolIdentity,
    messages: &[cs_mail_primitives::MessageId],
) -> Result<Vec<cs_mail_application::aliases::DeliveredRoute>, StorageError> {
    let ids: Vec<_> = messages.iter().map(|id| id.0.to_string()).collect();
    let mut result = vec![];
    for row in tx.query("SELECT d.message,d.transport,d.alias,d.conversation,d.sender,d.admitted_at,d.evidence FROM cs_alias_deliveries d JOIN cs_alias_routes a ON a.id=d.alias WHERE a.owner=$1 AND d.message=ANY($2) ORDER BY d.transport,d.message", &[&owner.0.to_string(), &ids])? {
        let r = row.try_get::<_, Json<cs_mail_application::aliases::DeliveredRoute>>(6)?.0;
        let transport = match r.transport() { cs_mail_application::aliases::DeliveryTransport::Native => "native", cs_mail_application::aliases::DeliveryTransport::External => "external" };
        if r.recipient() != owner || row.try_get::<_, String>(0)? != r.message().0.to_string() || row.try_get::<_, String>(1)? != transport || row.try_get::<_, String>(2)? != r.alias().value().to_string() || row.try_get::<_, String>(3)? != r.conversation().value().to_string() || row.try_get::<_, String>(4)? != r.sender().0.to_string() || super::to_u64(row.try_get(5)?)? != r.at().0 { return Err(StorageError::NumericRange); }
        result.push(r);
    }
    Ok(result)
}

pub(super) fn current_sender_scopes(
    tx: &mut Transaction<'_>,
    owner: ProtocolIdentity,
) -> Result<Vec<cs_mail_application::aliases::SenderScopeRecord>, StorageError> {
    let mut result = vec![];
    for row in tx.query("SELECT sender,record FROM cs_alias_sender_scopes WHERE recipient=$1 ORDER BY sender FOR UPDATE", &[&owner.0.to_string()])? {
        let scope = row.try_get::<_, Json<cs_mail_application::aliases::SenderRouteScope>>(1)?.0;
        if scope.recipient() != owner || row.try_get::<_, String>(0)? != scope.sender().0.to_string() { return Err(StorageError::NumericRange); }
        let r: cs_mail_application::aliases::SenderScopeRecord = scope.into();
        let (relationship, lane) = participant_permission(tx, r.sender, owner)?;
        if relationship.is_some_and(|rel| rel.version == r.relationship_version && match (rel.state, r.lane) {
            (cs_mail_protocol::RelationshipState::Accepted, None) => true,
            (cs_mail_protocol::RelationshipState::ExpressLane(id), Some((wanted,version))) => id == wanted && lane.as_ref().is_some_and(|l| l.grant().id == id && l.grant().version == version),
            _ => false,
        }) { result.push(r); }
    }
    Ok(result)
}

/// Commit the application-declared allowance state only against the exact protected permission.
pub(super) fn persist_paired_lane(
    tx: &mut Transaction<'_>,
    route: &cs_mail_application::aliases::DeliveredRoute,
    lane: &cs_mail_capabilities::SenderScopedLane,
) -> Result<bool, StorageError> {
    let cs_mail_application::aliases::ReceivingProvenance::AliasPermission(
        cs_mail_application::aliases::RouteBasis::PairedSender {
            id,
            permission_revision,
            version,
        },
    ) = route.provenance()
    else {
        return Ok(false);
    };
    if *id != lane.grant().id || version.checked_next() != Some(lane.version()) {
        return Err(StorageError::VersionConflict);
    }
    let alias = receiving_context(tx, route.alias())?.ok_or(StorageError::VersionConflict)?;
    if alias.owner() != route.recipient() || alias.permission().revision() != *permission_revision {
        return Err(StorageError::VersionConflict);
    }
    let next = alias.with_permission(alias.permission().consume(lane)?)?;
    tx.execute(
        "UPDATE cs_alias_routes SET record=$2 WHERE id=$1",
        &[&alias.id().value().to_string(), &Json(next)],
    )?;
    Ok(true)
}

impl super::PostgresDeployment {
    /// Only the current public alias is published. Private aliases remain owner-only.
    /// # Errors
    /// Refuses inconsistent stored ownership and unavailable storage.
    pub fn public_receiving_alias(
        &self,
        owner: ProtocolIdentity,
    ) -> Result<Option<cs_mail_application::aliases::PublicReceivingAlias>, StorageError> {
        let mut db = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = db.transaction()?;
        let aliases = owned_aliases(&mut tx, owner)?;
        let portfolio = cs_mail_accounts::aliases::AliasPortfolio::new(owner, &aliases)?;
        let result = portfolio.current_public().map(|alias| {
            cs_mail_application::aliases::PublicReceivingAlias {
                owner,
                id: alias.id(),
                address: alias.address().clone(),
            }
        });
        tx.commit()?;
        Ok(result)
    }
}
