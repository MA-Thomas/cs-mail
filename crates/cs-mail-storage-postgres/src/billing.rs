//! Account/service transactions; bank calls execute after durable dispatch authorization.
use super::*;
use cs_mail_billing::{BillingAccount, BillingError, ServiceOffer, SignedBillingCommand};
use cs_mail_finance::{FundingSource, PaymentKind, PaymentOperation, SignedPaymentEvidence};
use cs_mail_primitives::{AllocationId, BillingAccountId, PolicyVersion, ProtocolIdentity};

fn load(tx: &mut Transaction<'_>, id: BillingAccountId) -> Result<BillingAccount, StorageError> {
    let row = tx.query_one(
        "SELECT record FROM cs_billing_accounts WHERE id=$1 FOR UPDATE",
        &[&id.0.to_string()],
    )?;
    let account = row.get::<_, Json<BillingAccount>>(0).0;
    if account.id() != id {
        return Err(BillingError::Conflict.into());
    }
    Ok(account)
}
fn save(tx: &mut Transaction<'_>, account: &BillingAccount) -> Result<(), StorageError> {
    tx.execute(
        "UPDATE cs_billing_accounts SET record=$2 WHERE id=$1",
        &[&account.id().0.to_string(), &Json(account)],
    )?;
    Ok(())
}
pub(super) fn source_key(scope: cs_mail_finance::FinancialScope, token: &[u8; 32]) -> String {
    let mut h = Sha256::new();
    h.update(scope.canonical_bytes());
    h.update(token);
    h.finalize()
        .iter()
        .fold(String::with_capacity(64), |mut key, b| {
            use std::fmt::Write;
            let _ = write!(key, "{b:02x}");
            key
        })
}
fn payment_error(e: cs_mail_finance::PaymentError) -> StorageError {
    StorageError::Billing(BillingError::Payment(e))
}
pub(super) fn arrangement(
    tx: &mut Transaction<'_>,
    scope: cs_mail_finance::FinancialScope,
    unit: SettlementUnit,
) -> Result<([u8; 32], [u8; 32]), StorageError> {
    let row=tx.query_one("SELECT scope,settlement_unit,processor_key,bank_authority FROM cs_payment_arrangement WHERE singleton",&[])?;
    if row.get::<_, Json<cs_mail_finance::FinancialScope>>(0).0 != scope
        || row.get::<_, i64>(1) != i64::from(unit.0)
    {
        return Err(BillingError::Conflict.into());
    }
    Ok((
        row.get::<_, Vec<u8>>(2)
            .try_into()
            .map_err(|_| BillingError::Conflict)?,
        row.get::<_, Vec<u8>>(3)
            .try_into()
            .map_err(|_| BillingError::Conflict)?,
    ))
}
impl PostgresDeployment {
    /// Trusted deployment configuration: exactly one payment-processing arrangement.
    /// # Errors
    /// Rejects missing keys, incompatible existing configuration, or database failures.
    pub fn configure_payment_arrangement(
        &self,
        scope: cs_mail_finance::FinancialScope,
        unit: SettlementUnit,
        processor_key: [u8; 32],
        bank_authority: [u8; 32],
    ) -> Result<(), StorageError> {
        if processor_key == [0; 32] || bank_authority == [0; 32] {
            return Err(BillingError::Conflict.into());
        }
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        tx.execute("INSERT INTO cs_payment_arrangement(singleton,scope,settlement_unit,processor_key,bank_authority) VALUES(TRUE,$1,$2,$3,$4) ON CONFLICT DO NOTHING",&[&Json(scope),&i64::from(unit.0),&processor_key.as_slice(),&bank_authority.as_slice()])?;
        if arrangement(&mut tx, scope, unit)? != (processor_key, bank_authority) {
            return Err(BillingError::Conflict.into());
        }
        tx.commit()?;
        Ok(())
    }
    /// Operator-only startup/configuration API; ordinary account commands cannot publish prices or periods.
    /// # Errors
    /// Rejects changes to an existing policy version or database failures.
    pub fn publish_service_offer(&self, offer: &ServiceOffer) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        tx.execute(
            "INSERT INTO cs_service_offers(version,record) VALUES($1,$2) ON CONFLICT DO NOTHING",
            &[&offer.version().0.to_string(), &Json(offer)],
        )?;
        if tx
            .query_one(
                "SELECT record FROM cs_service_offers WHERE version=$1",
                &[&offer.version().0.to_string()],
            )?
            .get::<_, Json<ServiceOffer>>(0)
            .0
            != *offer
        {
            return Err(BillingError::Conflict.into());
        }
        tx.commit()?;
        Ok(())
    }
    /// # Errors
    /// Rejects missing or invalid stored accounts and database failures.
    pub fn billing_account(&self, id: BillingAccountId) -> Result<BillingAccount, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        let account = load(&mut tx, id)?;
        tx.commit()?;
        Ok(account)
    }
}

fn service_offer(
    tx: &mut Transaction<'_>,
    version: PolicyVersion,
) -> Result<ServiceOffer, StorageError> {
    Ok(tx
        .query_one(
            "SELECT record FROM cs_service_offers WHERE version=$1",
            &[&version.0.to_string()],
        )?
        .get::<_, Json<ServiceOffer>>(0)
        .0)
}
pub(super) fn reserve_source(
    tx: &mut Transaction<'_>,
    account: BillingAccountId,
    operation: &PaymentOperation,
) -> Result<(), StorageError> {
    let key = source_key(operation.scope, &operation.destination);
    let row = tx
        .query_opt(
            "SELECT account,record FROM cs_funding_sources WHERE source_key=$1 FOR UPDATE",
            &[&key],
        )?
        .ok_or(BillingError::Payment(
            cs_mail_finance::PaymentError::FundingRestricted,
        ))?;
    if row.get::<_, String>(0) != account.0.to_string() {
        return Err(BillingError::Conflict.into());
    }
    let mut source = row.get::<_, Json<FundingSource>>(1).0;
    source.reserve(operation).map_err(payment_error)?;
    tx.execute(
        "UPDATE cs_funding_sources SET record=$2 WHERE source_key=$1",
        &[&key, &Json(source)],
    )?;
    Ok(())
}
pub(super) fn authorize_source(
    tx: &mut Transaction<'_>,
    operation: &PaymentOperation,
) -> Result<(), StorageError> {
    if operation.kind != PaymentKind::Capture {
        return Ok(());
    }
    let key = source_key(operation.scope, &operation.destination);
    let mut source = tx
        .query_one(
            "SELECT record FROM cs_funding_sources WHERE source_key=$1 FOR UPDATE",
            &[&key],
        )?
        .get::<_, Json<FundingSource>>(0)
        .0;
    source
        .authorize_dispatch(operation.id)
        .map_err(payment_error)?;
    tx.execute(
        "UPDATE cs_funding_sources SET record=$2 WHERE source_key=$1",
        &[&key, &Json(source)],
    )?;
    Ok(())
}
pub(super) fn record_source(
    tx: &mut Transaction<'_>,
    operation: &PaymentOperation,
    receipt: &SignedPaymentEvidence,
    key: &[u8; 32],
) -> Result<(), StorageError> {
    if operation.kind != PaymentKind::Capture {
        return Ok(());
    }
    let source_id = source_key(operation.scope, &operation.destination);
    let mut source = tx
        .query_one(
            "SELECT record FROM cs_funding_sources WHERE source_key=$1 FOR UPDATE",
            &[&source_id],
        )?
        .get::<_, Json<FundingSource>>(0)
        .0;
    source.record(receipt, key).map_err(payment_error)?;
    tx.execute(
        "UPDATE cs_funding_sources SET record=$2 WHERE source_key=$1",
        &[&source_id, &Json(source)],
    )?;
    Ok(())
}
pub(super) fn identity_account(
    tx: &mut Transaction<'_>,
    identity: ProtocolIdentity,
) -> Result<BillingAccount, StorageError> {
    Ok(tx.query_opt("SELECT a.record FROM cs_billing_accounts a JOIN cs_product_accounts p ON a.id=p.billing JOIN cs_persona_owners i ON p.id=i.account WHERE i.identity=$1 FOR SHARE OF a",&[&identity.0.to_string()])?.ok_or(BillingError::MissingRecord)?.get::<_,Json<BillingAccount>>(0).0)
}
pub(super) fn check_local_sender_coverage(
    tx: &mut Transaction<'_>,
    sender: ProtocolIdentity,
    at: CanonicalTime,
) -> Result<(), StorageError> {
    // Explicitly registered remote identities have their own service authority.
    if tx
        .query_opt(
            "SELECT 1 FROM cs_remote_identities WHERE identity=$1",
            &[&sender.0.to_string()],
        )?
        .is_some()
    {
        return Ok(());
    }
    let active = tx.query_opt("SELECT p.control FROM cs_persona_owners i JOIN cs_product_accounts p ON p.id=i.account WHERE i.identity=$1 FOR SHARE OF p", &[&sender.0.to_string()])?.ok_or(BillingError::MissingRecord)?.get::<_,Json<cs_mail_accounts::control::AccountControl>>(0).0.allows_service();
    if !active || !identity_account(tx, sender)?.covers(at) {
        return Err(BillingError::ServiceNotCovered.into());
    }
    Ok(())
}

/// Persist an application-approved enrollment under receipt ordering and ownership locks.
pub(super) fn insert_enrollment(
    tx: &mut Transaction<'_>,
    pending: &cs_mail_application::accounts::PendingEnrollment,
    records: cs_mail_application::accounts::enrollment::EnrollmentRecords,
) -> Result<(), StorageError> {
    let product = &records.product;
    let account = &records.billing;
    let evidence = account.bank().evidence();
    let ledger = records.ledger;
    tx.execute(
        "INSERT INTO cs_billing_accounts(id,person,member,record,ledger) VALUES($1,$2,$3,$4,$5)",
        &[
            &account.id().0.to_string(),
            &evidence.person.as_slice(),
            &account.member().0.to_string(),
            &Json(account),
            &Json(ledger),
        ],
    )?;
    tx.execute("INSERT INTO cs_product_accounts(id,principal,billing,member,issuer,product,subject_ref,binding_version,registry,membership_identity,control) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)", &[&product.id().0.to_string(),&product.principal().0.to_string(),&product.billing().0.to_string(),&product.member().0.to_string(),&product.binding().issuer(),&product.binding().product(),&product.binding().subject_ref(),&to_i64(product.binding().version())?,&Json(records.registry),&records.membership_identity.as_slice(),&Json(records.control)])?;
    tx.execute(
        "INSERT INTO cs_persona_owners(identity,account) VALUES($1,$2)",
        &[
            &pending.input().persona.0.to_string(),
            &product.id().0.to_string(),
        ],
    )?;
    let source_id = source_key(account.scope(), &evidence.bank_token);
    tx.execute(
        "INSERT INTO cs_funding_sources(source_key,account,record) VALUES($1,$2,$3)",
        &[
            &source_id,
            &account.id().0.to_string(),
            &Json(records.source),
        ],
    )?;
    tx.execute(
        "UPDATE cs_key_claims SET claim=$2 WHERE reference=$1",
        &[
            &pending.input().key_ref.0.to_string(),
            &Json(records.key_claim),
        ],
    )?;
    Ok(())
}

use cs_mail_application::accounts::operations::AccountState;
use cs_mail_application::billing::operations::{
    BillingContext, BillingDecision, BillingScope, BillingStore, DistributionContext,
    DistributionDecision,
};
impl From<cs_mail_finance::ProgramError> for StorageError {
    fn from(error: cs_mail_finance::ProgramError) -> Self {
        Self::Finance(error)
    }
}
impl BillingStore for PostgresDeployment {
    type Error = StorageError;
    fn billing<T>(
        &self,
        scope: BillingScope,
        decide: impl FnOnce(BillingContext) -> Result<BillingDecision<T>, StorageError>,
    ) -> Result<T, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        let id = scope.account.0.to_string();
        let row = tx.query_opt("SELECT registry,control,product FROM cs_product_accounts WHERE billing=$1 FOR SHARE", &[&id])?.ok_or(BillingError::MissingRecord)?;
        let product = AccountState {
            registry: row.get::<_, Json<KeyRegistry>>(0).0,
            control: row
                .get::<_, Json<cs_mail_accounts::control::AccountControl>>(1)
                .0,
            product: row.get(2),
        };
        let account = load(&mut tx, scope.account)?;
        let (processor, bank_authority) = arrangement(&mut tx, account.scope(), account.unit())?;
        let offer = scope
            .offer
            .map(|version| service_offer(&mut tx, version))
            .transpose()?;
        let ledger = tx
            .query_one("SELECT ledger FROM cs_billing_accounts WHERE id=$1", &[&id])?
            .get::<_, Json<LedgerView>>(0)
            .0;
        let sources = tx.query("SELECT record FROM cs_funding_sources WHERE account=$1 ORDER BY source_key FOR UPDATE", &[&id])?.into_iter().map(|row| { let source = row.get::<_,Json<FundingSource>>(0).0; (*source.bank_token(), source) }).collect();
        let prior = if let Some(command) = scope.command {
            tx.query_opt(
                "SELECT command,outcome FROM cs_billing_commands WHERE account=$1 AND id=$2",
                &[&id, &command.0.to_string()],
            )?
            .map(|row| {
                (
                    row.get::<_, Json<SignedBillingCommand>>(0).0,
                    row.get::<_, Json<Vec<PaymentOperation>>>(1).0,
                )
            })
        } else {
            None
        };
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
            save(&mut tx, &writes.account)?;
            for (token, source) in writes.sources {
                let key = source_key(writes.account.scope(), &token);
                if tx.execute(
                    "UPDATE cs_funding_sources SET record=$3 WHERE source_key=$1 AND account=$2",
                    &[&key, &id, &Json(source)],
                )? != 1
                {
                    return Err(StorageError::VersionConflict);
                }
            }
            tx.execute(
                "UPDATE cs_billing_accounts SET ledger=$2 WHERE id=$1",
                &[&id, &Json(writes.ledger)],
            )?;
            if let Some(journal) = writes.journal {
                tx.execute("INSERT INTO cs_billing_journal(account,event,entry) VALUES($1,$2,$3)", &[&id, &journal.receipt.evidence.event_id.0.to_string(), &Json(serde_json::json!({"at":journal.at,"receipt":journal.receipt,"postings":journal.postings}))])?;
            }
            if let Some(work) = writes.work {
                work::enqueue_deployment(
                    &mut tx,
                    work::WorkSource::PaymentAttempt(work.operation),
                    &WorkPayload::UtilityPayment {
                        account: work.account,
                        contract: work.contract,
                        operation: work.operation,
                    },
                    work.at,
                )?;
            }
            if let Some((command, outcome, at)) = writes.command {
                tx.execute("INSERT INTO cs_billing_commands(account,id,command,outcome,received_at) VALUES($1,$2,$3,$4,$5)", &[&id,&command.idempotency_key.0.to_string(),&Json(command),&Json(outcome),&to_i64(at.0)?])?;
            }
        }
        tx.commit()?;
        Ok(outcome)
    }
    fn distribution(
        &self,
        unit: SettlementUnit,
        allocation: AllocationId,
        decide: impl FnOnce(DistributionContext) -> Result<DistributionDecision, StorageError>,
    ) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        let program = finance::load_program(
            &mut tx,
            unit,
            None,
            finance::ProgramRead::Payable(allocation),
        )?;
        let member = program
            .records()
            .payables
            .get(&allocation)
            .ok_or(BillingError::MissingRecord)?
            .member();
        let account = tx
            .query_one(
                "SELECT record FROM cs_billing_accounts WHERE member=$1 FOR SHARE",
                &[&member.0.to_string()],
            )?
            .get::<_, Json<BillingAccount>>(0)
            .0;
        let (program, operation, at, changed) =
            decide(DistributionContext { program, account })?.into_parts();
        if changed {
            finance::save_program(
                &mut tx,
                &program,
                &serde_json::json!({"prepare_distribution":allocation}),
                at,
            )?;
        }
        if let Some(op) = operation {
            work::enqueue_deployment(
                &mut tx,
                work::WorkSource::PaymentAttempt(op.id),
                &WorkPayload::MemberPayment {
                    unit,
                    allocation,
                    operation: op.id,
                },
                at,
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}
