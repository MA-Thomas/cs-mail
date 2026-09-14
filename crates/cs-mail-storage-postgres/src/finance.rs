//! Durable financial program. Request exports and pool ingestion share one transaction.
use super::*;
use cs_mail_finance::{
    FinancialProgram, Forfeiture, PaymentOperation, ProgramError, ProgramOutcome,
    SignedPaymentEvidence, SignedProgramCommand,
};
use cs_mail_primitives::{AllocationId, PaymentOperationId, RequestId};

/// Point operations assemble only their owner. Allocation and audit require the complete program.
#[derive(Clone, Copy)]
enum ProgramRead {
    All,
    Metadata,
    Lot(PaymentOperationId),
    Member(cs_mail_primitives::MemberId),
    Payable(AllocationId),
}
fn load_program(
    transaction: &mut Transaction<'_>,
    unit: SettlementUnit,
    scope: Option<cs_mail_finance::FinancialScope>,
    read: ProgramRead,
) -> Result<FinancialProgram, StorageError> {
    if let Some(scope) = scope {
        transaction.execute("INSERT INTO cs_financial_programs(settlement_unit,metadata,program_format_version) VALUES($1,$2,4) ON CONFLICT DO NOTHING",&[&i64::from(unit.0),&Json(FinancialProgram::new(scope,unit).metadata())])?;
    }
    let row=transaction.query_one("SELECT metadata,program_format_version,ledger_revision FROM cs_financial_programs WHERE settlement_unit=$1 FOR UPDATE",&[&i64::from(unit.0)])?;
    let version: i16 = row.get(1);
    if version != 4 {
        return Err(StorageError::UnsupportedStoredFinancialFormat(version));
    }
    let metadata = row.get::<_, Json<cs_mail_finance::ProgramMetadata>>(0).0;
    let mut records = cs_mail_finance::ProgramRecords::default();
    for (table, selected) in [
        (
            "cs_program_lots",
            match read {
                ProgramRead::Lot(id) => Some(id.0.to_string()),
                _ => None,
            },
        ),
        (
            "cs_program_members",
            match read {
                ProgramRead::Member(id) => Some(id.0.to_string()),
                _ => None,
            },
        ),
        ("cs_program_schedules", None),
        ("cs_program_quarters", None),
        (
            "cs_program_payables",
            match read {
                ProgramRead::Payable(id) => Some(id.0.to_string()),
                _ => None,
            },
        ),
    ] {
        if !matches!(read, ProgramRead::All) && selected.is_none() {
            continue;
        }
        let rows=transaction.query(&format!("SELECT id,record FROM {table} WHERE settlement_unit=$1 AND ($2::text IS NULL OR id=$2)"),&[&i64::from(unit.0),&selected])?;
        for row in rows {
            let id = row
                .get::<_, String>(0)
                .parse::<u128>()
                .map_err(|_| StorageError::NumericRange)?;
            let record = row.get::<_, Json<serde_json::Value>>(1).0;
            match table {
                "cs_program_lots" => {
                    records
                        .funding
                        .insert(PaymentOperationId(id), serde_json::from_value(record)?);
                }
                "cs_program_members" => {
                    records.members.insert(
                        cs_mail_primitives::MemberId(id),
                        serde_json::from_value(record)?,
                    );
                }
                "cs_program_schedules" => {
                    records.schedules.insert(
                        cs_mail_primitives::QuarterId(id),
                        serde_json::from_value(record)?,
                    );
                }
                "cs_program_quarters" => {
                    records.quarters.insert(
                        cs_mail_primitives::QuarterId(id),
                        serde_json::from_value(record)?,
                    );
                }
                "cs_program_payables" => {
                    records
                        .payables
                        .insert(AllocationId(id), serde_json::from_value(record)?);
                }
                _ => unreachable!(),
            }
        }
    }
    let balances = decode_balances(transaction.query(
        "SELECT account,balance::text AS balance FROM cs_program_accounts WHERE settlement_unit=$1",
        &[&i64::from(unit.0)],
    )?)?;
    let ledger = LedgerView::from_balances(to_u64(row.get(2))?, unit, balances);
    FinancialProgram::restore(metadata, records, ledger, Vec::new()).map_err(StorageError::Finance)
}
fn save_program(
    transaction: &mut Transaction<'_>,
    program: &FinancialProgram,
    event: &serde_json::Value,
    at: CanonicalTime,
) -> Result<(), StorageError> {
    transaction.execute(
        "UPDATE cs_financial_programs SET metadata=$2,ledger_revision=$3 WHERE settlement_unit=$1",
        &[
            &i64::from(program.unit.0),
            &Json(program.metadata()),
            &to_i64(program.ledger().revision)?,
        ],
    )?;
    let records = program.records();
    macro_rules! save_records {($table:literal,$records:expr)=>{for (id,record) in $records {
        transaction.execute(concat!("INSERT INTO ",$table,"(settlement_unit,id,record) VALUES($1,$2,$3) ON CONFLICT(settlement_unit,id) DO UPDATE SET record=EXCLUDED.record WHERE ",$table,".record IS DISTINCT FROM EXCLUDED.record"),&[&i64::from(program.unit.0),&id.0.to_string(),&Json(record)])?;
    }}}
    save_records!("cs_program_lots", &records.funding);
    save_records!("cs_program_members", &records.members);
    save_records!("cs_program_schedules", &records.schedules);
    save_records!("cs_program_quarters", &records.quarters);
    save_records!("cs_program_payables", &records.payables);
    for (account, balance) in program.ledger().balances() {
        transaction.execute("INSERT INTO cs_program_accounts(settlement_unit,account_key,account,balance) VALUES($1,$2,$3,$4::text::numeric) ON CONFLICT(settlement_unit,account_key) DO UPDATE SET balance=EXCLUDED.balance WHERE cs_program_accounts.balance IS DISTINCT FROM EXCLUDED.balance",&[&i64::from(program.unit.0),&account_key(*account)?,&Json(account),&balance.to_string()])?;
    }
    for (ordinal, entry) in program.journal().iter().enumerate() {
        transaction.execute("INSERT INTO cs_program_journal(settlement_unit,revision,ordinal,entry) VALUES($1,$2,$3,$4)",&[&i64::from(program.unit.0),&to_i64(program.revision)?,&i32::try_from(ordinal).map_err(|_|StorageError::NumericRange)?,&Json(entry)])?;
    }
    transaction.execute("INSERT INTO cs_financial_events(settlement_unit,revision,event,received_at) VALUES($1,$2,$3,$4)",&[&i64::from(program.unit.0),&to_i64(program.revision)?,&Json(event),&to_i64(at.0)?])?;
    Ok(())
}
pub(super) fn persist_forfeitures(
    transaction: &mut Transaction<'_>,
    sources: &[Forfeiture],
    at: CanonicalTime,
) -> Result<(), StorageError> {
    for source in sources {
        let mut program = load_program(
            transaction,
            source.unit,
            Some(source.terms.scope),
            ProgramRead::Lot(source.id),
        )?;
        let previous = program.revision;
        program
            .record_forfeiture(source.clone(), at)
            .map_err(StorageError::Finance)?;
        if program.revision != previous {
            save_program(transaction, &program, &serde_json::to_value(source)?, at)?;
        }
    }
    Ok(())
}
impl PostgresEngine {
    /// Configures separate administration and payment-verification authority once.
    /// This startup API is not an unauthenticated network endpoint.
    /// # Errors
    /// Rejects attempts to replace an established authority.
    pub fn configure_financial_program(
        &self,
        scope: cs_mail_finance::FinancialScope,
        unit: SettlementUnit,
        administration_key: [u8; 32],
        provider_key: [u8; 32],
    ) -> Result<(), StorageError> {
        if administration_key == [0; 32] || provider_key == [0; 32] {
            return Err(StorageError::Finance(ProgramError::InvalidPolicy));
        }
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        let program = load_program(&mut tx, unit, Some(scope), ProgramRead::Metadata)?;
        if program.scope != scope {
            return Err(StorageError::VersionConflict);
        }
        let row=tx.query_one("SELECT administration_key,payment_provider_key FROM cs_financial_programs WHERE settlement_unit=$1",&[&i64::from(unit.0)])?;
        let old: Option<Vec<u8>> = row.get(0);
        let provider: Option<Vec<u8>> = row.get(1);
        if old.as_deref().is_some_and(|key| key != administration_key)
            || provider.as_deref().is_some_and(|key| key != provider_key)
        {
            return Err(StorageError::DuplicateConflict);
        }
        tx.execute("UPDATE cs_financial_programs SET administration_key=$2,payment_provider_key=$3 WHERE settlement_unit=$1",&[&i64::from(unit.0),&administration_key.as_slice(),&provider_key.as_slice()])?;
        tx.commit()?;
        Ok(())
    }
    /// Verifies administration authority and atomically records one replay-safe program command.
    /// # Errors
    /// Rejects signatures, stale revisions, conflicting replay, or invalid business transitions.
    pub fn execute_financial_command(
        &self,
        signed: &SignedProgramCommand,
        at: CanonicalTime,
    ) -> Result<ProgramOutcome, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        let mut program = load_program(
            &mut tx,
            signed.unit,
            None,
            match &signed.command {
                cs_mail_finance::ProgramCommand::PreparePayout { allocation, .. } => {
                    ProgramRead::Payable(*allocation)
                }
                cs_mail_finance::ProgramCommand::Hold { source, .. }
                | cs_mail_finance::ProgramCommand::ClearMaturity { source, .. } => {
                    ProgramRead::Lot(*source)
                }
                cs_mail_finance::ProgramCommand::SetMembership { member, .. }
                | cs_mail_finance::ProgramCommand::RecordActivity { member, .. } => {
                    ProgramRead::Member(*member)
                }
                _ => ProgramRead::All,
            },
        )?;
        let row = tx.query_one(
            "SELECT administration_key FROM cs_financial_programs WHERE settlement_unit=$1",
            &[&i64::from(signed.unit.0)],
        )?;
        let key: Option<Vec<u8>> = row.get(0);
        let key: [u8; 32] = key
            .ok_or(StorageError::Finance(ProgramError::InvalidPayment))?
            .try_into()
            .map_err(|_| StorageError::NumericRange)?;
        signed.verify(&key).map_err(StorageError::Finance)?;
        let command = serde_json::to_value(signed)?;
        let operation_key = signed.idempotency_key.0.to_string();
        if let Some(row)=tx.query_opt("SELECT command,outcome FROM cs_financial_commands WHERE settlement_unit=$1 AND operation_key=$2",&[&i64::from(signed.unit.0),&operation_key])? {
            if row.get::<_,Json<serde_json::Value>>(0).0!=command {return Err(StorageError::DuplicateConflict);}
            let outcome=row.get::<_,Json<ProgramOutcome>>(1).0;tx.commit()?;return Ok(outcome);
        }
        if signed.scope != program.scope || signed.expected_revision != program.revision {
            return Err(StorageError::VersionConflict);
        }
        let previous = program.revision;
        let outcome = program
            .apply(&signed.command, at)
            .map_err(StorageError::Finance)?;
        if let ProgramOutcome::Payout(Some(operation)) = &outcome {
            let cs_mail_finance::PaymentKind::MemberPayout { allocation } = operation.kind else {
                return Err(StorageError::Finance(ProgramError::InvalidPayment));
            };
            work::enqueue(
                &mut tx,
                &self.aggregate_key,
                work::WorkSource::Payout(operation.id),
                &WorkPayload::MemberPayment {
                    unit: signed.unit,
                    allocation,
                    operation: operation.id,
                },
                at,
            )?;
        }
        if program.revision != previous {
            save_program(&mut tx, &program, &command, at)?;
        }
        tx.execute("INSERT INTO cs_financial_commands (settlement_unit,operation_key,command,outcome,received_at) VALUES ($1,$2,$3,$4,$5)",&[&i64::from(signed.unit.0),&operation_key,&Json(command),&Json(&outcome),&to_i64(at.0)?])?;
        tx.commit()?;
        Ok(outcome)
    }
    /// # Errors
    /// Returns storage or decoding errors.
    pub fn financial_program(
        &self,
        unit: SettlementUnit,
    ) -> Result<FinancialProgram, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        let loaded = load_program(&mut tx, unit, None, ProgramRead::All)?;
        let journal=tx.query("SELECT entry FROM cs_program_journal WHERE settlement_unit=$1 ORDER BY revision,ordinal",&[&i64::from(unit.0)])?.into_iter().map(|r|r.get::<_,Json<cs_mail_finance::ProgramJournalEntry>>(0).0).collect();
        let program = FinancialProgram::restore(
            loaded.metadata(),
            loaded.records().clone(),
            loaded.ledger(),
            journal,
        )
        .map_err(StorageError::Finance)?;
        tx.commit()?;
        Ok(program)
    }
    /// Returns a request's still-pending operation and whether capture cancellation is required.
    /// # Errors
    /// Returns snapshot errors or missing requests.
    pub fn pending_request_payment(
        &self,
        request: RequestId,
        operation: PaymentOperationId,
    ) -> Result<Option<(PaymentOperation, bool)>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let row = client.query_opt(
            "SELECT f.financials,a.protocol_format_version FROM cs_request_financials f JOIN cs_relationship_aggregates a USING(aggregate_key) WHERE f.aggregate_key=$1 AND f.request_id=$2",
            &[&self.aggregate_key, &request.0.to_string()],
        )?;
        if let Some(row) = &row {
            let version: i16 = row.get(1);
            if version != CURRENT_PROTOCOL_FORMAT_VERSION {
                return Err(StorageError::UnsupportedStoredProtocolFormat(version));
            }
        }
        Ok(row.and_then(|r| {
            r.get::<_, Json<cs_mail_finance::RequestFinancials>>(0)
                .0
                .pending_payment(operation)
                .map(|(o, c)| (o.clone(), c))
        }))
    }
    /// Applies independently signed provider evidence through the ordinary atomic transition.
    /// # Errors
    /// Rejects evidence that fails the request's immutable provider key and operation binding.
    pub fn confirm_request_payment(
        &self,
        request: RequestId,
        receipt: SignedPaymentEvidence,
        now: CanonicalTime,
        policy: PolicySnapshot,
    ) -> Result<ReceivedOutcome, StorageError> {
        let key = payment_event_key(&receipt);
        let command = KernelCommand::new(
            ProtocolCommand::RecordPayment {
                request_id: request,
                receipt,
            },
            ActorRef::Provider(policy.recipient_provider),
            OperationalKeyRef(0),
            key,
        );
        self.execute_system(command, now, policy)
    }
    /// Loads one pending payable operation; completed/obsolete work returns None.
    /// # Errors
    /// Returns storage or program errors.
    pub fn pending_member_payment(
        &self,
        unit: SettlementUnit,
        id: AllocationId,
        operation: PaymentOperationId,
    ) -> Result<Option<PaymentOperation>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let row = client.query_opt(
            "SELECT r.record,p.program_format_version FROM cs_program_payables r JOIN cs_financial_programs p USING(settlement_unit) WHERE r.settlement_unit=$1 AND r.id=$2",
            &[&i64::from(unit.0), &id.0.to_string()],
        )?;
        if let Some(row) = &row {
            let version: i16 = row.get(1);
            if version != 4 {
                return Err(StorageError::UnsupportedStoredFinancialFormat(version));
            }
        }
        Ok(row.and_then(|r| {
            r.get::<_, Json<cs_mail_finance::MemberPayable>>(0)
                .0
                .lifecycle
                .pending()
                .filter(|o| o.id == operation)
                .cloned()
        }))
    }
    /// # Errors
    /// Rejects unauthenticated or mismatched payout evidence; failed payment leaves the liability intact.
    pub fn confirm_member_payment(
        &self,
        unit: SettlementUnit,
        id: AllocationId,
        receipt: &SignedPaymentEvidence,
        now: CanonicalTime,
    ) -> Result<(), StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::require_drained(&mut tx)?;
        let mut program = load_program(&mut tx, unit, None, ProgramRead::Payable(id))?;
        let row = tx.query_one(
            "SELECT payment_provider_key FROM cs_financial_programs WHERE settlement_unit=$1",
            &[&i64::from(unit.0)],
        )?;
        let key: Option<Vec<u8>> = row.get(0);
        let key: [u8; 32] = key
            .ok_or(StorageError::Finance(ProgramError::InvalidPayment))?
            .try_into()
            .map_err(|_| StorageError::NumericRange)?;
        let previous = program.revision;
        program
            .confirm_payout(id, receipt, &key, now)
            .map_err(StorageError::Finance)?;
        if program.revision != previous {
            save_program(&mut tx, &program, &serde_json::to_value(receipt)?, now)?;
        }
        tx.commit()?;
        Ok(())
    }
}
fn payment_event_key(receipt: &SignedPaymentEvidence) -> IdempotencyKey {
    let mut h = Sha256::new();
    h.update(b"cs-mail/payment-event/v1");
    h.update(receipt.evidence.operation_id.0.to_be_bytes());
    h.update(receipt.evidence.event_id.0.to_be_bytes());
    let digest = h.finalize();
    let mut b = [0; 16];
    b.copy_from_slice(&digest[..16]);
    IdempotencyKey(u128::from_be_bytes(b))
}

pub(super) fn persist_forfeiture_holds(
    transaction: &mut Transaction<'_>,
    unit: SettlementUnit,
    ids: &[PaymentOperationId],
    at: CanonicalTime,
) -> Result<(), StorageError> {
    for id in ids {
        let mut program = load_program(transaction, unit, None, ProgramRead::Lot(*id))?;
        program
            .note_reversal(*id, at)
            .map_err(StorageError::Finance)?;
        save_program(
            transaction,
            &program,
            &serde_json::json!({"reversed_forfeiture":id}),
            at,
        )?;
    }
    Ok(())
}
