//! All key registration paths use this one unique ownership table.
use super::{ActorRef, Json, OperationalKeyRef, SecurityError, StorageError, Transaction};
use cs_mail_accounts::keys::{KeyAuthorityOwner, KeyClaim};
use cs_mail_application::accounts::PendingEnrollment;

pub(super) fn insert(
    tx: &mut Transaction<'_>,
    reference: OperationalKeyRef,
    claim: &KeyClaim,
) -> Result<(), StorageError> {
    if tx.execute(
        "INSERT INTO cs_key_claims(reference,claim) VALUES($1,$2) ON CONFLICT DO NOTHING",
        &[&reference.0.to_string(), &Json(claim)],
    )? != 1
    {
        return Err(SecurityError::DuplicateKey.into());
    }
    Ok(())
}
pub(super) fn reserve(
    tx: &mut Transaction<'_>,
    pending: &PendingEnrollment,
) -> Result<(), StorageError> {
    insert(
        tx,
        pending.input().key_ref,
        &KeyClaim::Reserved {
            enrollment: pending.intent().operation.clone(),
            intended_owner: pending.account(),
            actor: pending.input().actor,
            key: pending.input().initial_key,
        },
    )
}
pub(super) fn provider(
    tx: &mut Transaction<'_>,
    reference: OperationalKeyRef,
    actor: ActorRef,
    key: [u8; 32],
) -> Result<(), StorageError> {
    let owner = match actor {
        ActorRef::Provider(id) => KeyAuthorityOwner::Provider(id),
        ActorRef::Scheduler(id) => KeyAuthorityOwner::Scheduler(id),
        _ => return Err(SecurityError::ActorMismatch.into()),
    };
    let claim = KeyClaim::Assigned { owner, actor, key };
    tx.execute(
        "INSERT INTO cs_key_claims(reference,claim) VALUES($1,$2) ON CONFLICT DO NOTHING",
        &[&reference.0.to_string(), &Json(&claim)],
    )?;
    let stored = tx
        .query_one(
            "SELECT claim FROM cs_key_claims WHERE reference=$1 FOR UPDATE",
            &[&reference.0.to_string()],
        )?
        .get::<_, Json<KeyClaim>>(0)
        .0;
    if stored != claim {
        return Err(SecurityError::DuplicateKey.into());
    }
    Ok(())
}
