//! Account/service transactions; bank calls execute after durable dispatch authorization.
use super::*;
use cs_mail_billing::{
    BillingAccount, BillingCommand, BillingError, ServiceOffer, SignedBillingCommand,
};
use cs_mail_finance::{
    BankVerification, FundingSource, PaymentKind, PaymentOperation, SignedPaymentEvidence,
};
use cs_mail_primitives::{
    AllocationId, BillingAccountId, PaymentOperationId, PolicyVersion, ProtocolIdentity,
};

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
fn source_key(scope: cs_mail_finance::FinancialScope, token: &[u8; 32]) -> String {
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
impl PostgresEngine {
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
    /// Trusted enrollment boundary. Signature, person uniqueness, aliases and explicit account grants commit together.
    /// # Errors
    /// Rejects invalid bank evidence, duplicate person/account bindings, conflicting aliases, or database failures.
    pub fn register_billing_account(
        &self,
        evidence: &BankVerification,
        identities: &[ProtocolIdentity],
        account_actors: &[ActorRef],
        maximum_unresolved: u32,
    ) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        let (_, authority) = arrangement(&mut tx, evidence.scope, evidence.unit)?;
        let verified = evidence.verify(&authority).map_err(payment_error)?;
        let source =
            FundingSource::verified(&verified, maximum_unresolved).map_err(payment_error)?;
        let account = BillingAccount::new(verified);
        let ledger = cs_mail_ledger::LedgerState::new(account.unit()).view();
        tx.execute("INSERT INTO cs_billing_accounts(id,person,member,record,ledger) VALUES($1,$2,$3,$4,$5) ON CONFLICT(id) DO NOTHING",&[&account.id().0.to_string(),&evidence.person.as_slice(),&account.member().0.to_string(),&Json(&account),&Json(ledger)])?;
        let old = load(&mut tx, account.id())?;
        if old.bank() != account.bank() {
            return Err(BillingError::Conflict.into());
        }
        for identity in identities {
            tx.execute("INSERT INTO cs_billing_identities(identity,account) VALUES($1,$2) ON CONFLICT DO NOTHING",&[&identity.0.to_string(),&account.id().0.to_string()])?;
            if tx
                .query_one(
                    "SELECT account FROM cs_billing_identities WHERE identity=$1",
                    &[&identity.0.to_string()],
                )?
                .get::<_, String>(0)
                != account.id().0.to_string()
            {
                return Err(BillingError::Conflict.into());
            }
        }
        for actor in account_actors {
            let (ActorRef::Sender(identity) | ActorRef::Recipient(identity)) = actor else {
                return Err(BillingError::Conflict.into());
            };
            if !identities.contains(identity) {
                return Err(BillingError::Conflict.into());
            }
            tx.execute("INSERT INTO cs_account_authorities(account,actor) VALUES($1,$2) ON CONFLICT DO NOTHING",&[&account.id().0.to_string(),&Json(actor)])?;
        }
        tx.execute("INSERT INTO cs_funding_sources(source_key,account,record) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",&[&source_key(account.scope(),&evidence.bank_token),&account.id().0.to_string(),&Json(source)])?;
        let source_owner: String = tx
            .query_one(
                "SELECT account FROM cs_funding_sources WHERE source_key=$1",
                &[&source_key(account.scope(), &evidence.bank_token)],
            )?
            .get(0);
        if source_owner != account.id().0.to_string() {
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
    /// Signed account actions reuse revocable operational-key authority and explicit account grants.
    /// # Errors
    /// Rejects unauthorized account actions, stale revisions, conflicting replay, or invalid service transitions.
    pub fn execute_billing_command(
        &self,
        signed: &SignedBillingCommand,
        at: CanonicalTime,
    ) -> Result<Vec<PaymentOperation>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        let registry = ingress::registry_locked(&mut tx, &self.aggregate_key)?;
        let (actor, key) = registry.active_actor(signed.operational_key, at)?;
        signed.verify(&key)?;
        if tx
            .query_opt(
                "SELECT 1 FROM cs_account_authorities WHERE account=$1 AND actor=$2",
                &[&signed.account.0.to_string(), &Json(actor)],
            )?
            .is_none()
        {
            return Err(BillingError::Conflict.into());
        }
        let mut account = load(&mut tx, signed.account)?;
        if account.scope() != signed.scope {
            return Err(BillingError::Conflict.into());
        }
        if let Some(row) = tx.query_opt(
            "SELECT command,outcome FROM cs_billing_commands WHERE account=$1 AND id=$2",
            &[
                &signed.account.0.to_string(),
                &signed.idempotency_key.0.to_string(),
            ],
        )? {
            if row.get::<_, Json<SignedBillingCommand>>(0).0 != *signed {
                return Err(StorageError::DuplicateConflict);
            }
            return Ok(row.get::<_, Json<Vec<PaymentOperation>>>(1).0);
        }
        if signed.command == BillingCommand::Inspect {
            return Ok(Vec::new());
        }
        if account.revision() != signed.expected_revision {
            return Err(StorageError::VersionConflict);
        }
        let operations = match signed.command {
            BillingCommand::Inspect => unreachable!(),
            BillingCommand::CloseAccount => {
                account.close()?;
                Vec::new()
            }
            BillingCommand::PurchaseService { offer } => {
                let offer = service_offer(&mut tx, offer)?;
                if at >= offer.period().end() {
                    return Err(BillingError::InvalidSchedule.into());
                }
                let (processor, _) = arrangement(&mut tx, account.scope(), account.unit())?;
                let id = account.purchase(&offer, processor)?;
                let operation = account.contracts()[&id].collection().current().clone();
                reserve_source(&mut tx, account.id(), &operation)?;
                enqueue_collection(
                    &mut tx,
                    &self.aggregate_key,
                    account.id(),
                    id,
                    &operation,
                    at.max(offer.collect_at()),
                )?;
                vec![operation]
            }
            BillingCommand::RetryCollection { contract } => {
                let operation = account.retry_collection(contract)?;
                reserve_source(&mut tx, account.id(), &operation)?;
                enqueue_collection(
                    &mut tx,
                    &self.aggregate_key,
                    account.id(),
                    contract,
                    &operation,
                    at.max(account.contracts()[&contract].offer().collect_at()),
                )?;
                vec![operation]
            }
        };
        save(&mut tx, &account)?;
        tx.execute("INSERT INTO cs_billing_commands(account,id,command,outcome,received_at) VALUES($1,$2,$3,$4,$5)",&[&account.id().0.to_string(),&signed.idempotency_key.0.to_string(),&Json(signed),&Json(&operations),&to_i64(at.0)?])?;
        tx.commit()?;
        Ok(operations)
    }
    /// Establishes a durable grant against current restrictions before any external submission.
    /// # Errors
    /// Rejects early execution, unknown contracts, funding restrictions, or database failures.
    pub fn authorize_utility_dispatch(
        &self,
        id: BillingAccountId,
        contract: cs_mail_primitives::ServiceContractId,
        operation: PaymentOperationId,
        at: CanonicalTime,
    ) -> Result<Option<PaymentOperation>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        let account = load(&mut tx, id)?;
        let contract = account
            .contracts()
            .get(&contract)
            .ok_or(BillingError::MissingRecord)?;
        if at < contract.offer().collect_at() {
            return Err(BillingError::TooEarly.into());
        }
        let operation = contract
            .collection()
            .pending()
            .filter(|op| op.id == operation)
            .cloned();
        if let Some(op) = &operation {
            authorize_source(&mut tx, op)?;
        }
        tx.commit()?;
        Ok(operation)
    }
    /// # Errors
    /// Rejects unbound evidence, invalid funding transitions, or database failures.
    pub fn confirm_utility_payment(
        &self,
        id: BillingAccountId,
        contract: cs_mail_primitives::ServiceContractId,
        receipt: &SignedPaymentEvidence,
        at: CanonicalTime,
    ) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        let mut account = load(&mut tx, id)?;
        let before = account.clone();
        let original = account
            .contracts()
            .get(&contract)
            .ok_or(BillingError::MissingRecord)?;
        let operation = original
            .collection()
            .operation(receipt.evidence.operation_id)
            .cloned()
            .ok_or(BillingError::MissingRecord)?;
        let key = *original.processor_key();
        let batch = account.record_collection(contract, receipt, at)?;
        record_source(&mut tx, &operation, receipt, &key)?;
        if before != account {
            let ledger = tx
                .query_one(
                    "SELECT ledger FROM cs_billing_accounts WHERE id=$1",
                    &[&id.0.to_string()],
                )?
                .get::<_, Json<LedgerView>>(0)
                .0
                .apply(&batch)?;
            save(&mut tx, &account)?;
            tx.execute(
                "UPDATE cs_billing_accounts SET ledger=$2 WHERE id=$1",
                &[&id.0.to_string(), &Json(ledger)],
            )?;
            tx.execute(
                "INSERT INTO cs_billing_journal(account,event,entry) VALUES($1,$2,$3)",
                &[
                    &id.0.to_string(),
                    &receipt.evidence.event_id.0.to_string(),
                    &Json(serde_json::json!({"at":at,"receipt":receipt,"postings":batch})),
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    /// Reverification is accepted only from the configured bank authority, never from account commands.
    /// # Errors
    /// Rejects invalid authority, changed person/bank associations, stale verification, or database failures.
    pub fn reverify_funding_source(&self, evidence: &BankVerification) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        let (_, authority) = arrangement(&mut tx, evidence.scope, evidence.unit)?;
        let bank = evidence.verify(&authority).map_err(payment_error)?;
        let account = load(&mut tx, evidence.account)?;
        let old = account.bank().evidence();
        if old.person != evidence.person
            || old.bank_token != evidence.bank_token
            || old.member != evidence.member
        {
            return Err(BillingError::Conflict.into());
        }
        let key = source_key(evidence.scope, &evidence.bank_token);
        let mut source = tx
            .query_one(
                "SELECT record FROM cs_funding_sources WHERE source_key=$1 FOR UPDATE",
                &[&key],
            )?
            .get::<_, Json<FundingSource>>(0)
            .0;
        source.reverify(&bank).map_err(payment_error)?;
        tx.execute(
            "UPDATE cs_funding_sources SET record=$2 WHERE source_key=$1",
            &[&key, &Json(source)],
        )?;
        tx.commit()?;
        Ok(())
    }
    /// One ordinary distribution path. Closure and future renewal do not change the payable.
    /// # Errors
    /// Rejects missing owners, early payment, incompatible bank associations, or database failures.
    pub fn prepare_due_distribution(
        &self,
        unit: SettlementUnit,
        allocation: AllocationId,
        at: CanonicalTime,
    ) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        let mut program = finance::load_program(
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
        let before = program.revision();
        let operation = cs_mail_application::billing::prepare_distribution(
            &account,
            &mut program,
            allocation,
            at,
        )
        .map_err(|e| match e {
            cs_mail_application::billing::DistributionError::Billing(e) => StorageError::Billing(e),
            cs_mail_application::billing::DistributionError::Program(e) => StorageError::Finance(e),
        })?;
        if before != program.revision() {
            finance::save_program(
                &mut tx,
                &program,
                &serde_json::json!({"prepare_distribution":allocation}),
                at,
            )?;
        }
        if let Some(op) = operation {
            work::enqueue(
                &mut tx,
                &self.aggregate_key,
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
fn enqueue_collection(
    tx: &mut Transaction<'_>,
    aggregate: &str,
    account: BillingAccountId,
    contract: cs_mail_primitives::ServiceContractId,
    operation: &PaymentOperation,
    at: CanonicalTime,
) -> Result<(), StorageError> {
    work::enqueue(
        tx,
        aggregate,
        work::WorkSource::PaymentAttempt(operation.id),
        &WorkPayload::UtilityPayment {
            account,
            contract,
            operation: operation.id,
        },
        at,
    )
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
    Ok(tx.query_opt("SELECT a.record FROM cs_billing_accounts a JOIN cs_billing_identities i ON a.id=i.account WHERE i.identity=$1 FOR SHARE OF a",&[&identity.0.to_string()])?.ok_or(BillingError::MissingRecord)?.get::<_,Json<BillingAccount>>(0).0)
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
    if !identity_account(tx, sender)?.covers(at) {
        return Err(BillingError::ServiceNotCovered.into());
    }
    Ok(())
}
