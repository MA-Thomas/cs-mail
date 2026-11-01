//! Deployment bootstrap and the operator command log.
use super::{
    Json, PostgresDeployment, StorageError, Transaction, accounts, billing, ingress, members,
    pricing, to_i64,
};
use cs_mail_application::operations::{
    BootstrapDecision, BootstrapStore, OperatorContext, OperatorDecision, OperatorEffect,
    OperatorStore, OperatorTarget,
};
use cs_mail_operations::{
    BootstrapOutcome, CommandFamily, DeploymentBootstrap, OperationsError, OperatorCommand,
    OperatorOutcome, SignedOperatorCommand,
};

fn stored_deployment(
    tx: &mut Transaction<'_>,
    lock: bool,
) -> Result<Option<DeploymentBootstrap>, StorageError> {
    Ok(tx
        .query_opt(
            if lock {
                "SELECT record FROM cs_deployment WHERE singleton FOR UPDATE"
            } else {
                "SELECT record FROM cs_deployment WHERE singleton FOR SHARE"
            },
            &[],
        )?
        .map(|row| row.get::<_, Json<DeploymentBootstrap>>(0).0))
}

/// Whether the database answers and the deployment has been bootstrapped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Readiness {
    pub bootstrapped: bool,
    /// A processing fault degrades its owner scopes; independent scopes remain available.
    pub command_failure: Option<ingress::ProcessingFailure>,
}

impl PostgresDeployment {
    /// Checks the connection and whether the deployment exists; any database error means
    /// the service is not ready.
    /// # Errors
    /// Returns the database error.
    pub fn readiness(&self) -> Result<Readiness, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        Ok(Readiness {
            bootstrapped: client
                .query_opt("SELECT 1 FROM cs_deployment WHERE singleton", &[])?
                .is_some(),
            command_failure: client.query_opt("SELECT processing_failure FROM cs_received_commands WHERE outcome IS NULL AND processing_failure IS NOT NULL ORDER BY position LIMIT 1", &[])?
                .map(|row| row.try_get::<_, Option<Json<ingress::ProcessingFailure>>>(0))
                .transpose()?.flatten().map(|value| value.0),
        })
    }
    /// Whether this handle can no longer complete a database round trip. The sync
    /// client drives its connection only during I/O; `is_closed` alone can miss a
    /// server disconnect while idle. A failed connection is not reopened: the process
    /// restarts under its supervisor with fresh connections.
    pub fn connection_lost(&self) -> bool {
        self.client.lock().map_or(true, |mut client| {
            client.is_closed() || client.simple_query("SELECT 1").is_err()
        })
    }
    /// The bootstrapped deployment, if any.
    /// # Errors
    /// Returns database or decoding errors.
    pub fn deployment_record(&self) -> Result<Option<DeploymentBootstrap>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        let record = stored_deployment(&mut tx, false)?;
        tx.commit()?;
        Ok(record)
    }
}

impl BootstrapStore for PostgresDeployment {
    type Error = StorageError;
    fn transact_bootstrap(
        &self,
        bootstrap: &DeploymentBootstrap,
        decide: impl FnOnce(Option<DeploymentBootstrap>) -> Result<BootstrapDecision, StorageError>,
    ) -> Result<BootstrapOutcome, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        // The receipt-order lock also protects the absent deployment row.
        ingress::require_drained(&mut tx)?;
        let stored = stored_deployment(&mut tx, true)?;
        let BootstrapDecision { outcome, at } = decide(stored)?;
        if outcome == BootstrapOutcome::Created {
            let record = bootstrap.record();
            tx.execute(
                "INSERT INTO cs_deployment(singleton,record,bootstrapped_at) VALUES(TRUE,$1,$2)",
                &[&Json(bootstrap), &to_i64(at.0)?],
            )?;
            billing::install_arrangement(
                &mut tx,
                record.financial_scope,
                record.unit,
                record.payment.processor_key,
                record.payment.bank_authority,
            )?;
            accounts::install_identity_trust(
                &mut tx,
                &record.identity.issuer,
                &record.identity.product,
                record.identity.decision_key,
            )?;
        }
        tx.commit()?;
        Ok(outcome)
    }
}

impl OperatorStore for PostgresDeployment {
    type Error = StorageError;
    fn transact_operator_command(
        &self,
        signed: &SignedOperatorCommand<OperatorCommand>,
        decide: impl FnOnce(OperatorContext) -> Result<OperatorDecision, StorageError>,
    ) -> Result<OperatorOutcome, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        let authority = stored_deployment(&mut tx, false)?
            .ok_or(OperationsError::NotBootstrapped)?
            .operator_authority();
        let identity = signed.idempotency_key.0.to_string();
        let prior = tx
            .query_opt(
                "SELECT digest,outcome FROM cs_operator_commands \
                 WHERE family=$1 AND idempotency_key=$2 FOR UPDATE",
                &[&OperatorCommand::NAME, &identity],
            )?
            .map(|row| -> Result<_, StorageError> {
                Ok((
                    row.get::<_, Vec<u8>>(0)
                        .try_into()
                        .map_err(|_| StorageError::NumericRange)?,
                    row.get::<_, Json<OperatorOutcome>>(1).0,
                ))
            })
            .transpose()?;
        let target = match &signed.command {
            OperatorCommand::PublishServiceOffer(offer) => OperatorTarget::ServiceOffer {
                same_version: billing::offer_version(&mut tx, offer.version())?,
            },
            OperatorCommand::PublishRequestPricing(policy) => {
                OperatorTarget::RequestPricing(pricing::policy_context(&mut tx, policy)?)
            }
            OperatorCommand::PublishProtocolPolicy(policy) => OperatorTarget::ProtocolPolicy {
                current: members::current_protocol_policy(&mut tx, true)?.map(Box::new),
                same_version: members::protocol_policy_version(&mut tx, policy.version())?
                    .map(Box::new),
            },
        };
        let (record, effect, outcome) = decide(OperatorContext {
            authority,
            prior,
            target,
        })?
        .into_parts();
        match effect {
            Some(OperatorEffect::PublishServiceOffer(offer)) => {
                billing::insert_offer(&mut tx, &offer)?;
            }
            Some(OperatorEffect::PublishRequestPricing(policy, at)) => {
                pricing::publish_policy(&mut tx, &policy, at)?;
            }
            Some(OperatorEffect::PublishProtocolPolicy(policy, at)) => {
                members::publish_protocol_policy(&mut tx, &policy, at)?;
            }
            None => {}
        }
        if let Some(record) = record {
            tx.execute(
                "INSERT INTO cs_operator_commands(family,idempotency_key,operator,digest,command,outcome,authorized_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7)",
                &[
                    &OperatorCommand::NAME,
                    &identity,
                    &record.command.operator().0.to_string(),
                    &record.command.digest().as_slice(),
                    &Json(record.command.command()),
                    &Json(record.outcome),
                    &to_i64(record.authorized_at.0)?,
                ],
            )?;
        }
        tx.commit()?;
        Ok(outcome)
    }
}

impl cs_mail_application::processing::ProcessingStore for PostgresDeployment {
    type Error = StorageError;
    fn transact_processing(
        &self,
        signed: &SignedOperatorCommand<cs_mail_operations::ProcessingCommand>,
        decide: impl FnOnce(
            cs_mail_application::processing::ProcessingContext,
        )
            -> Result<cs_mail_application::processing::ProcessingDecision, StorageError>,
    ) -> Result<cs_mail_operations::ProcessingResponse, StorageError> {
        use cs_mail_application::processing::{
            ProcessingContext, ProcessingDecision, ProcessingInput, ProcessingTarget,
        };
        use cs_mail_operations::{
            ProcessingCommand, ProcessingInspection, ProcessingResponse, ProcessingStatus,
        };
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        let authority = stored_deployment(&mut tx, false)?
            .ok_or(OperationsError::NotBootstrapped)?
            .operator_authority();
        authority.verify(signed)?;
        let identity = signed.idempotency_key.0.to_string();
        let prior = tx.query_opt("SELECT digest,outcome FROM cs_operator_commands WHERE family=$1 AND idempotency_key=$2", &[&ProcessingCommand::NAME, &identity])?.map(|row| -> Result<_,StorageError> { Ok((row.get::<_,Vec<u8>>(0).try_into().map_err(|_| StorageError::NumericRange)?, row.get::<_,Json<ProcessingResponse>>(1).0)) }).transpose()?;
        let inspect = |row: postgres::Row| -> Result<ProcessingInspection, StorageError> {
            let attempts = super::to_u64(row.get("failed_attempts"))?;
            let status = if row
                .get::<_, Option<Json<serde_json::Value>>>("outcome")
                .is_some()
            {
                ProcessingStatus::Completed
            } else {
                match row
                    .get::<_, Option<Json<ingress::ProcessingFailure>>>("processing_failure")
                    .map(|v| v.0)
                {
                    None => ProcessingStatus::Ready,
                    Some(ingress::ProcessingFailure::AwaitingQuoteSignature) => {
                        ProcessingStatus::WaitingForQuote { attempts }
                    }
                    Some(ingress::ProcessingFailure::Persistence) => {
                        ProcessingStatus::Retryable { attempts }
                    }
                    Some(ingress::ProcessingFailure::RequiresIntervention) => {
                        ProcessingStatus::Intervention { attempts }
                    }
                }
            };
            Ok(ProcessingInspection {
                position: cs_mail_primitives::JournalPosition(super::to_u64(row.get("position"))?),
                digest: cs_mail_security::CommandDigest(
                    row.get::<_, Vec<u8>>("digest")
                        .try_into()
                        .map_err(|_| StorageError::NumericRange)?,
                ),
                status,
            })
        };
        let input = match &signed.command {
            ProcessingCommand::Inspect { limit } if (1..=100).contains(limit) => ProcessingInput::Pending(tx.query("SELECT position,digest,outcome,processing_failure,failed_attempts FROM cs_received_commands WHERE outcome IS NULL ORDER BY position LIMIT $1", &[&i64::from(*limit)])?.into_iter().map(inspect).collect::<Result<_,_>>()?),
            ProcessingCommand::Resume { position, .. } => {
                let row = tx.query_opt("SELECT position,digest,outcome,processing_failure,failed_attempts FROM cs_received_commands WHERE position=$1 FOR UPDATE", &[&super::to_i64(position.0)?])?.ok_or(OperationsError::Invalid)?;
                let record = inspect(row)?;
                ProcessingInput::Command(ProcessingTarget { digest: record.digest, status: record.status })
            }
            ProcessingCommand::Inspect { .. } => return Err(OperationsError::Invalid.into()),
        };
        let decision = decide(ProcessingContext {
            authority,
            prior,
            input,
        })?;
        let response = match decision {
            ProcessingDecision::Observed(response) => response,
            ProcessingDecision::Resumed {
                command,
                at,
                response,
            } => {
                let ProcessingCommand::Resume { position, .. } = command.command() else {
                    return Err(OperationsError::Invalid.into());
                };
                tx.execute("UPDATE cs_received_commands SET processing_failure=NULL WHERE position=$1 AND outcome IS NULL", &[&super::to_i64(position.0)?])?;
                tx.execute("INSERT INTO cs_operator_commands(family,idempotency_key,operator,digest,command,outcome,authorized_at) VALUES($1,$2,$3,$4,$5,$6,$7)", &[&ProcessingCommand::NAME,&identity,&command.operator().0.to_string(),&command.digest().as_slice(),&Json(command.command()),&Json(&response),&super::to_i64(at.0)?])?;
                response
            }
        };
        tx.commit()?;
        Ok(response)
    }
}
