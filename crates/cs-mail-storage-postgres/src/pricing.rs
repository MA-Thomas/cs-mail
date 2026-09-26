//! Deployment-level request pricing: CSQD's policy versions and each address's signed
//! class publication. Quotes read both inside the relationship transaction.
use super::{Json, PostgresDeployment, StorageError, Transaction, ingress, to_i64};
use cs_mail_application::request_pricing::{
    ClassesPublicationContext, ClassesPublicationDecision, PolicyPublicationContext,
    PolicyPublicationDecision, PolicyPublicationReport, RequestPricingStore,
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

pub(super) fn current_policy(
    tx: &mut Transaction<'_>,
    lock: RowLock,
) -> Result<Option<RequestPricingPolicy>, StorageError> {
    Ok(tx
        .query_opt(
            &format!(
                "SELECT p.policy FROM cs_request_pricing_current c \
                 JOIN cs_request_pricing_policies p USING(version) WHERE c.singleton {}",
                lock.clause()
            ),
            &[],
        )?
        .map(|row| row.get::<_, Json<RequestPricingPolicy>>(0).0))
}

pub(super) fn current_classes(
    tx: &mut Transaction<'_>,
    recipient: ProtocolIdentity,
    lock: RowLock,
) -> Result<Option<RecipientRequestClasses>, StorageError> {
    Ok(tx
        .query_opt(
            &format!(
                "SELECT classes FROM cs_recipient_request_classes WHERE recipient=$1 {}",
                lock.clause()
            ),
            &[&recipient.0.to_string()],
        )?
        .map(|row| row.get::<_, Json<RecipientRequestClasses>>(0).0))
}

/// Loads the current policy and the recipient's current publication under share locks and
/// resolves the requested class with the domain rule.
pub(super) fn resolve_price(
    tx: &mut Transaction<'_>,
    recipient: ProtocolIdentity,
    class: RequestClassId,
) -> Result<QuotePricing, StorageError> {
    let policy = current_policy(tx, RowLock::Share)?;
    let classes = current_classes(tx, recipient, RowLock::Share)?;
    Ok(QuotePricing::resolve(
        policy.as_ref(),
        classes.as_ref(),
        class,
    ))
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
        recipient: ProtocolIdentity,
    ) -> Result<Option<RecipientRequestClasses>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        let classes = current_classes(&mut tx, recipient, RowLock::Share)?;
        tx.commit()?;
        Ok(classes)
    }
}

impl RequestPricingStore for PostgresDeployment {
    type Error = StorageError;
    fn transact_pricing_policy(
        &self,
        policy: &RequestPricingPolicy,
        decide: impl FnOnce(PolicyPublicationContext) -> Result<PolicyPublicationDecision, StorageError>,
    ) -> Result<PolicyPublicationReport, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        // Ordered with receipts, but not drained: prices are read when a command is
        // processed, never captured at receipt, so in-flight commands need not finish first.
        ingress::lock_receipt_order(&mut tx)?;
        let current = current_policy(&mut tx, RowLock::Update)?;
        let version = to_i64(policy.version().0)?;
        let same_version = tx
            .query_opt(
                "SELECT policy FROM cs_request_pricing_policies WHERE version=$1",
                &[&version],
            )?
            .map(|row| row.get::<_, Json<RequestPricingPolicy>>(0).0);
        let publications = tx
            .query("SELECT classes FROM cs_recipient_request_classes", &[])?
            .into_iter()
            .map(|row| row.get::<_, Json<RecipientRequestClasses>>(0).0)
            .collect();
        let (next, report) = decide(PolicyPublicationContext {
            current,
            same_version,
            publications,
        })?
        .into_parts();
        if let Some((policy, at)) = next {
            tx.execute(
                "INSERT INTO cs_request_pricing_policies(version,policy,published_at) \
                 VALUES($1,$2,$3)",
                &[&version, &Json(&policy), &to_i64(at.0)?],
            )?;
            tx.execute(
                "INSERT INTO cs_request_pricing_current(singleton,version) VALUES(TRUE,$1) \
                 ON CONFLICT(singleton) DO UPDATE SET version=EXCLUDED.version",
                &[&version],
            )?;
        }
        tx.commit()?;
        Ok(report)
    }

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
        let policy = current_policy(&mut tx, RowLock::Share)?;
        let current = current_classes(&mut tx, recipient, RowLock::Update)?;
        if let Some((classes, at)) = decide(ClassesPublicationContext {
            authority,
            policy,
            current,
        })?
        .into_effects()
        {
            let version = to_i64(classes.version())?;
            tx.execute(
                "INSERT INTO cs_request_class_publications(recipient,version,publication,received_at) \
                 VALUES($1,$2,$3,$4)",
                &[
                    &recipient.0.to_string(),
                    &version,
                    &Json(signed),
                    &to_i64(at.0)?,
                ],
            )?;
            tx.execute(
                "INSERT INTO cs_recipient_request_classes(recipient,version,classes) VALUES($1,$2,$3) \
                 ON CONFLICT(recipient) DO UPDATE SET version=EXCLUDED.version,classes=EXCLUDED.classes",
                &[&recipient.0.to_string(), &version, &Json(&classes)],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}
