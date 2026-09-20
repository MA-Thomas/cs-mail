//! Staged in-memory commits execute the same application decisions as `PostgreSQL`.
use super::operations::{
    BillingContext, BillingDecision, BillingScope, BillingStore, DistributionContext,
    DistributionDecision,
};
use crate::{EngineError, InMemoryStore};
use cs_mail_billing::{BillingError, ServiceOffer};
use cs_mail_primitives::{AllocationId, SettlementUnit};
impl InMemoryStore {
    /// # Errors
    /// Rejects conflicting trusted payment configuration.
    pub fn configure_payment_arrangement(
        &self,
        unit: SettlementUnit,
        processor: [u8; 32],
        authority: [u8; 32],
    ) -> Result<(), EngineError> {
        let mut world = self.inner.lock().map_err(|_| EngineError::LockPoisoned)?;
        if processor == [0; 32]
            || authority == [0; 32]
            || world
                .billing_configuration
                .is_some_and(|old| old != (unit, processor, authority))
            || world
                .enrollment
                .configuration
                .as_ref()
                .is_some_and(|(_, key)| *key != authority)
        {
            return Err(BillingError::Conflict.into());
        }
        world.billing_configuration = Some((unit, processor, authority));
        Ok(())
    }
    /// # Errors
    /// Rejects changed content for an already published offer version.
    pub fn publish_service_offer(&self, offer: &ServiceOffer) -> Result<(), EngineError> {
        let mut world = self.inner.lock().map_err(|_| EngineError::LockPoisoned)?;
        if world
            .service_offers
            .get(&offer.version())
            .is_some_and(|old| old != offer)
        {
            return Err(BillingError::Conflict.into());
        }
        world.service_offers.insert(offer.version(), offer.clone());
        Ok(())
    }
}
impl BillingStore for InMemoryStore {
    type Error = EngineError;
    fn billing<T>(
        &self,
        scope: BillingScope,
        decide: impl FnOnce(BillingContext) -> Result<BillingDecision<T>, EngineError>,
    ) -> Result<T, EngineError> {
        let mut world = self.inner.lock().map_err(|_| EngineError::LockPoisoned)?;
        let mut staged = world.clone();
        let account = staged
            .billing_accounts
            .get(&scope.account)
            .ok_or(BillingError::MissingRecord)?
            .clone();
        let owner = staged
            .product_accounts
            .iter()
            .find(|(_, (product, _))| product.billing() == scope.account)
            .map(|(id, _)| *id)
            .ok_or(BillingError::MissingRecord)?;
        let product = staged
            .enrollment
            .managed
            .get(&owner)
            .ok_or(BillingError::MissingRecord)?
            .state
            .clone();
        let (unit, processor, bank_authority) = staged
            .billing_configuration
            .ok_or(BillingError::MissingRecord)?;
        if account.unit() != unit {
            return Err(BillingError::Conflict.into());
        }
        let offer = scope
            .offer
            .map(|version| {
                staged
                    .service_offers
                    .get(&version)
                    .cloned()
                    .ok_or(BillingError::MissingRecord)
            })
            .transpose()?;
        let sources = staged
            .funding_sources
            .iter()
            .filter(|(_, (owner, _))| *owner == scope.account)
            .map(|(token, (_, source))| (*token, source.clone()))
            .collect();
        let ledger = staged
            .billing_ledgers
            .get(&scope.account)
            .ok_or(BillingError::MissingRecord)?
            .clone();
        let prior = scope
            .command
            .and_then(|id| staged.billing_commands.get(&(scope.account, id)).cloned());
        let (outcome, writes) = decide(BillingContext {
            account,
            product,
            processor,
            bank_authority,
            offer,
            sources,
            ledger,
            prior,
        })?
        .into_parts();
        if let Some(writes) = writes {
            for (token, source) in writes.sources {
                let (owner, stored) = staged
                    .funding_sources
                    .get_mut(&token)
                    .ok_or(BillingError::MissingRecord)?;
                if *owner != scope.account {
                    return Err(EngineError::VersionConflict);
                }
                *stored = source;
            }
            staged
                .billing_accounts
                .insert(scope.account, writes.account);
            staged.billing_ledgers.insert(scope.account, writes.ledger);
            if let Some(journal) = writes.journal {
                let key = (
                    scope.account,
                    journal.receipt.evidence.event_id.0.to_string(),
                );
                if staged.billing_journal.contains_key(&key) {
                    return Err(EngineError::DuplicateConflict);
                }
                staged.billing_journal.insert(key, journal);
            }
            if let Some(work) = writes.work {
                staged.utility_work.insert(work.operation, work);
            }
            if let Some((command, outcome, _)) = writes.command {
                let key = (scope.account, command.idempotency_key);
                if staged.billing_commands.contains_key(&key) {
                    return Err(EngineError::DuplicateConflict);
                }
                staged.billing_commands.insert(key, (command, outcome));
            }
        }
        *world = staged;
        Ok(outcome)
    }
    fn distribution(
        &self,
        unit: SettlementUnit,
        allocation: AllocationId,
        decide: impl FnOnce(DistributionContext) -> Result<DistributionDecision, EngineError>,
    ) -> Result<(), EngineError> {
        let mut world = self.inner.lock().map_err(|_| EngineError::LockPoisoned)?;
        let mut staged = world.clone();
        let program = staged
            .programs
            .get(&unit)
            .ok_or(BillingError::MissingRecord)?
            .program
            .clone();
        let member = program
            .records()
            .payables
            .get(&allocation)
            .ok_or(BillingError::MissingRecord)?
            .member();
        let account = staged
            .billing_accounts
            .values()
            .find(|account| account.member() == member)
            .ok_or(BillingError::MissingRecord)?
            .clone();
        let (program, operation, _, changed) =
            decide(DistributionContext { program, account })?.into_parts();
        if changed {
            staged
                .programs
                .get_mut(&unit)
                .ok_or(BillingError::MissingRecord)?
                .program = program;
        }
        if let Some(operation) = operation {
            if staged
                .billing_work
                .get(&operation.id)
                .is_some_and(|old| old != &operation)
            {
                return Err(EngineError::DuplicateConflict);
            }
            staged.billing_work.insert(operation.id, operation);
        }
        *world = staged;
        Ok(())
    }
}
