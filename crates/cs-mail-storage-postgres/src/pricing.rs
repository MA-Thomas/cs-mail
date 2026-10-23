//! Deployment-level request pricing: CSQD's policy versions and each address's signed
//! class publication. Quotes read both inside the relationship transaction.
use super::{Json, PostgresDeployment, StorageError, Transaction, ingress, to_i64};
use cs_mail_application::request_pricing::{
    ClassesPublicationContext, ClassesPublicationDecision, PolicyPublicationContext,
    RequestPricingStore,
};
use cs_mail_primitives::{ProtocolIdentity, RequestClassId};
use cs_mail_protocol::pricing::{QuotePricing, RecipientRequestClasses, RequestPricingPolicy};
use cs_mail_security::{AuthoritySnapshot, KeyRegistry, SignedRequestClasses};

#[derive(Clone, Copy)]
pub(super) enum RowLock {
    Share,
    Update,
}
impl RowLock {
    const fn clause(self) -> &'static str {
        match self {
            Self::Share => "FOR SHARE",
            Self::Update => "FOR UPDATE",
        }
    }
}

/// Locks the current-version pointer, then reads that version in a separate statement.
/// A single locking join would be re-checked against the pointer's newest version while
/// keeping the policy row of the older snapshot, and so return no row while a concurrent
/// publication commits.
pub(super) fn current_policy(
    tx: &mut Transaction<'_>,
    lock: RowLock,
) -> Result<Option<RequestPricingPolicy>, StorageError> {
    let Some(version) = tx
        .query_opt(
            &format!(
                "SELECT version FROM cs_request_pricing_current WHERE singleton {}",
                lock.clause()
            ),
            &[],
        )?
        .map(|row| row.get::<_, i64>(0))
    else {
        return Ok(None);
    };
    Ok(Some(
        tx.query_one(
            "SELECT policy FROM cs_request_pricing_policies WHERE version=$1",
            &[&version],
        )?
        .get::<_, Json<RequestPricingPolicy>>(0)
        .0,
    ))
}

pub(super) fn current_classes(
    tx: &mut Transaction<'_>,
    alias: cs_mail_primitives::AliasId,
    lock: RowLock,
) -> Result<Option<RecipientRequestClasses>, StorageError> {
    Ok(tx
        .query_opt(
            &format!(
                "SELECT classes FROM cs_recipient_request_classes WHERE alias=$1 {}",
                lock.clause()
            ),
            &[&alias.value().to_string()],
        )?
        .map(|row| row.get::<_, Json<RecipientRequestClasses>>(0).0))
}

/// Loads the current policy and the recipient's current publication under share locks and
/// resolves the requested class with the domain rule.
pub(super) fn resolve_price(
    tx: &mut Transaction<'_>,
    recipient: ProtocolIdentity,
    alias: cs_mail_primitives::AliasId,
    class: RequestClassId,
) -> Result<QuotePricing, StorageError> {
    let policy = current_policy(tx, RowLock::Share)?;
    let route = super::aliases::load_alias(tx, alias)?;
    if route.owner() != recipient || !route.is_bonded_route() {
        return Err(StorageError::DuplicateConflict);
    }
    let classes = current_classes(tx, alias, RowLock::Share)?;
    Ok(QuotePricing::resolve(
        policy.as_ref(),
        classes.as_ref(),
        class,
    ))
}

/// Protected context for deciding an operator policy publication.
pub(super) fn policy_context(
    tx: &mut Transaction<'_>,
    policy: &RequestPricingPolicy,
) -> Result<PolicyPublicationContext, StorageError> {
    let current = current_policy(tx, RowLock::Update)?;
    let same_version = tx
        .query_opt(
            "SELECT policy FROM cs_request_pricing_policies WHERE version=$1",
            &[&to_i64(policy.version().0)?],
        )?
        .map(|row| row.get::<_, Json<RequestPricingPolicy>>(0).0);
    let publications = tx
        .query("SELECT classes FROM cs_recipient_request_classes", &[])?
        .into_iter()
        .map(|row| row.get::<_, Json<RecipientRequestClasses>>(0).0)
        .collect();
    Ok(PolicyPublicationContext {
        current,
        same_version,
        publications,
    })
}
/// Records a new policy version and makes it current.
pub(super) fn publish_policy(
    tx: &mut Transaction<'_>,
    policy: &RequestPricingPolicy,
    at: cs_mail_primitives::CanonicalTime,
) -> Result<(), StorageError> {
    let version = to_i64(policy.version().0)?;
    tx.execute(
        "INSERT INTO cs_request_pricing_policies(version,policy,published_at) VALUES($1,$2,$3)",
        &[&version, &Json(policy), &to_i64(at.0)?],
    )?;
    tx.execute(
        "INSERT INTO cs_request_pricing_current(singleton,version) VALUES(TRUE,$1) \
         ON CONFLICT(singleton) DO UPDATE SET version=EXCLUDED.version",
        &[&version],
    )?;
    Ok(())
}

impl PostgresDeployment {
    /// The current operator pricing policy, if one has been published.
    /// # Errors
    /// Returns database or decoding errors.
    pub fn request_pricing_policy(&self) -> Result<Option<RequestPricingPolicy>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        let policy = current_policy(&mut tx, RowLock::Share)?;
        tx.commit()?;
        Ok(policy)
    }
    /// An address's current request classes, if it has published any.
    /// # Errors
    /// Returns database or decoding errors.
    pub fn request_classes(
        &self,
        alias: cs_mail_primitives::AliasId,
    ) -> Result<Option<RecipientRequestClasses>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        let classes = current_classes(&mut tx, alias, RowLock::Share)?;
        tx.commit()?;
        Ok(classes)
    }
}

impl RequestPricingStore for PostgresDeployment {
    type Error = StorageError;
    fn transact_request_classes(
        &self,
        signed: &SignedRequestClasses,
        decide: impl FnOnce(
            ClassesPublicationContext,
        ) -> Result<ClassesPublicationDecision, StorageError>,
    ) -> Result<(), StorageError> {
        let recipient = signed.classes.recipient();
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        // Ordered with receipts, but not drained: prices are read when a command is
        // processed, never captured at receipt, so in-flight commands need not finish first.
        ingress::lock_receipt_order(&mut tx)?;
        // Only the account owning this address contributes authority; provider keys never
        // authorize a recipient's publication.
        let mut authority = AuthoritySnapshot::new(KeyRegistry::default())?;
        ingress::add_persona_authority(&mut tx, recipient, &mut authority)?;
        let alias = super::aliases::load_alias(&mut tx, signed.classes.alias())?;
        if alias.owner() != recipient || !alias.is_bonded_route() {
            return Err(StorageError::DuplicateConflict);
        }
        let policy = current_policy(&mut tx, RowLock::Share)?;
        let current = current_classes(&mut tx, signed.classes.alias(), RowLock::Update)?;
        if let Some((classes, at)) = decide(ClassesPublicationContext {
            authority,
            policy,
            current,
        })?
        .into_effects()
        {
            persist_classes(&mut tx, signed, &classes, at)?;
        }
        tx.commit()?;
        Ok(())
    }
}

/// Persists one application-authorized canonical class publication.
pub(super) fn persist_classes(
    tx: &mut Transaction<'_>,
    signed: &SignedRequestClasses,
    classes: &RecipientRequestClasses,
    at: cs_mail_primitives::CanonicalTime,
) -> Result<(), StorageError> {
    let recipient = classes.recipient();
    let version = to_i64(classes.version())?;
    tx.execute(
        "INSERT INTO cs_request_class_publications(recipient,alias,version,publication,received_at) \
                 VALUES($1,$5,$2,$3,$4)",
        &[
            &recipient.0.to_string(),
            &version,
            &Json(signed),
            &to_i64(at.0)?,
            &classes.alias().value().to_string(),
        ],
    )?;
    tx.execute(
                "INSERT INTO cs_recipient_request_classes(recipient,alias,version,classes) VALUES($1,$4,$2,$3) \
                 ON CONFLICT(alias) DO UPDATE SET version=EXCLUDED.version,classes=EXCLUDED.classes",
                &[&recipient.0.to_string(), &version, &Json(&classes), &classes.alias().value().to_string()],
            )?;
    Ok(())
}
