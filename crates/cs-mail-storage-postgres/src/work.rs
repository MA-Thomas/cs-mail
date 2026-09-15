//! One durable execution lifecycle for external effects. Claims are opaque and fenced.
use super::{
    CanonicalTime, Deserialize, Duration, EffectIntent, JournalPosition, Json, PostgresEngine,
    Serialize, SettlementUnit, StorageError, Transaction, to_i64,
};
use cs_mail_primitives::{
    AllocationId, AnnualDistributionId, BillingAccountId, PaymentOperationId,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkQueue {
    Delivery,
    UtilityPayments,
    AnnualAllocations(SettlementUnit),
    DistributionPreparation,
    RequestPayments,
    MemberPayments(SettlementUnit),
    Artifacts,
}
impl WorkQueue {
    fn kind(self) -> &'static str {
        match self {
            Self::UtilityPayments => "utility-payment",
            Self::AnnualAllocations(_) => "annual-allocation",
            Self::DistributionPreparation => "prepare-distribution",
            Self::Delivery => "delivery",
            Self::RequestPayments => "request-payment",
            Self::MemberPayments(_) => "member-payment",
            Self::Artifacts => "artifacts",
        }
    }
    fn owner(self, aggregate: &str) -> String {
        match self {
            Self::UtilityPayments | Self::DistributionPreparation => "billing".into(),
            Self::MemberPayments(unit) | Self::AnnualAllocations(unit) => {
                format!("program:{}", unit.0)
            }
            _ => format!("relationship:{aggregate}"),
        }
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorkPayload {
    Effect(EffectIntent),
    AnnualAllocation {
        unit: SettlementUnit,
        distribution: AnnualDistributionId,
    },
    PrepareDistribution {
        unit: SettlementUnit,
        allocation: AllocationId,
    },
    UtilityPayment {
        account: BillingAccountId,
        contract: cs_mail_primitives::ServiceContractId,
        operation: PaymentOperationId,
    },
    MemberPayment {
        unit: SettlementUnit,
        allocation: AllocationId,
        operation: PaymentOperationId,
    },
    Artifacts {
        position: JournalPosition,
    },
}
impl WorkPayload {
    fn queue(&self) -> WorkQueue {
        match self {
            Self::AnnualAllocation { unit, .. } => WorkQueue::AnnualAllocations(*unit),
            Self::PrepareDistribution { .. } => WorkQueue::DistributionPreparation,
            Self::UtilityPayment { .. } => WorkQueue::UtilityPayments,
            Self::Effect(EffectIntent::ExecutePayment { .. }) => WorkQueue::RequestPayments,
            Self::Effect(_) => WorkQueue::Delivery,
            Self::MemberPayment { unit, .. } => WorkQueue::MemberPayments(*unit),
            Self::Artifacts { .. } => WorkQueue::Artifacts,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkItem {
    pub id: i64,
    pub payload: WorkPayload,
    pub attempts: u32,
    owner: String,
    token: i64,
    aggregate: Option<String>,
    position: Option<JournalPosition>,
}
impl WorkItem {
    pub fn aggregate_key(&self) -> Option<&str> {
        self.aggregate.as_deref()
    }
    pub const fn journal_position(&self) -> Option<JournalPosition> {
        self.position
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkFailure {
    DependencyUnavailable,
    FundingRestricted,
    InvalidEvidence,
    MissingContent,
    Storage,
    SigningAuthority,
}
impl WorkFailure {
    const fn code(self) -> &'static str {
        match self {
            Self::FundingRestricted => "funding-restricted",
            Self::DependencyUnavailable => "dependency-unavailable",
            Self::InvalidEvidence => "invalid-evidence",
            Self::MissingContent => "missing-content",
            Self::Storage => "storage",
            Self::SigningAuthority => "signing-authority",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkReport {
    pub claimed: usize,
    pub completed: usize,
    pub retried: usize,
    pub blocked: usize,
    pub lost_claims: usize,
}

#[derive(Clone, Copy)]
pub(super) enum WorkSource {
    Receipt {
        position: JournalPosition,
        ordinal: usize,
    },
    PaymentAttempt(PaymentOperationId),
    AnnualAllocation(AnnualDistributionId),
    PrepareDistribution(AllocationId),
}

pub(super) fn enqueue(
    tx: &mut Transaction<'_>,
    key: &str,
    source: WorkSource,
    payload: &WorkPayload,
    now: CanonicalTime,
) -> Result<(), StorageError> {
    let queue = payload.queue();
    let aggregate = if matches!(
        queue,
        WorkQueue::MemberPayments(_)
            | WorkQueue::UtilityPayments
            | WorkQueue::AnnualAllocations(_)
            | WorkQueue::DistributionPreparation
    ) {
        None
    } else {
        Some(key)
    };
    let content = match payload {
        WorkPayload::Effect(EffectIntent::DeliverMessage { content_ref, .. }) => {
            Some(content_ref.0.to_string())
        }
        _ => None,
    };
    let (work_key, position) = match source {
        WorkSource::Receipt { position, ordinal } => (
            format!("{}:{}:{ordinal}", queue.kind(), position.0),
            Some(to_i64(position.0)?),
        ),
        WorkSource::AnnualAllocation(id) => (format!("annual-allocation:{}", id.0), None),
        WorkSource::PrepareDistribution(allocation) => {
            (format!("prepare-distribution:{}", allocation.0), None)
        }
        WorkSource::PaymentAttempt(id) => (format!("payout:{}", id.0), None),
    };
    let available_at = if matches!(
        queue,
        WorkQueue::UtilityPayments
            | WorkQueue::AnnualAllocations(_)
            | WorkQueue::DistributionPreparation
    ) {
        now.0
    } else {
        0
    };
    tx.execute("INSERT INTO cs_work(owner,aggregate_key,work_key,kind,payload,content_ref,status,available_at,created_at,journal_position) VALUES($1,$2,$3,$4,$5,$6,'ready',$9,$7,$8) ON CONFLICT(owner,work_key) DO NOTHING", &[&queue.owner(key), &aggregate, &work_key, &queue.kind(), &Json(payload), &content, &to_i64(now.0)?, &position, &to_i64(available_at)?])?;
    Ok(())
}
impl PostgresEngine {
    /// Claims a bounded batch. Reclaiming always assigns a new fencing token.
    /// # Errors
    /// Rejects zero leases, negative limits, clock overflow and database failures.
    pub fn claim_work(
        &self,
        queue: WorkQueue,
        now: CanonicalTime,
        lease: Duration,
        limit: i64,
    ) -> Result<Vec<WorkItem>, StorageError> {
        if lease.0 == 0 || limit < 0 {
            return Err(StorageError::NumericRange);
        }
        let until = to_i64(now.checked_add(lease).ok_or(StorageError::NumericRange)?.0)?;
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let rows = client.query("WITH candidates AS (SELECT id FROM cs_work WHERE owner=$1 AND kind=$2 AND ((status='ready' AND available_at<=$3) OR (status='running' AND claim_until<=$3)) ORDER BY available_at,id LIMIT $4 FOR UPDATE SKIP LOCKED) UPDATE cs_work w SET status='running',claim_until=$5,claim_token=nextval('cs_work_claim_token'),attempts=attempts+1 FROM candidates c WHERE w.id=c.id RETURNING w.id,w.payload,w.attempts,w.owner,w.claim_token,w.aggregate_key,w.journal_position", &[&queue.owner(&self.aggregate_key),&queue.kind(),&to_i64(now.0)?,&limit,&until])?;
        let mut items = rows
            .into_iter()
            .map(|r| {
                Ok(WorkItem {
                    id: r.get(0),
                    payload: r.get::<_, Json<WorkPayload>>(1).0,
                    attempts: u32::try_from(r.get::<_, i32>(2))
                        .map_err(|_| StorageError::NumericRange)?,
                    owner: r.get(3),
                    token: r.get(4),
                    aggregate: r.get(5),
                    position: r
                        .get::<_, Option<i64>>(6)
                        .map(super::to_u64)
                        .transpose()?
                        .map(JournalPosition),
                })
            })
            .collect::<Result<Vec<_>, StorageError>>()?;
        items.sort_by_key(|item| item.id);
        Ok(items)
    }
    /// Acknowledges only this claim, never another worker's reclaimed operation.
    /// # Errors
    /// Returns database or numeric errors.
    pub fn complete_work(&self, item: &WorkItem, now: CanonicalTime) -> Result<bool, StorageError> {
        self.finish_work(item, now, "complete", now, None)
    }
    /// Releases failed work with capped exponential backoff; uncertainty preserves its identity.
    /// # Errors
    /// Returns database or clock overflow errors.
    pub fn retry_work(
        &self,
        item: &WorkItem,
        now: CanonicalTime,
        failure: WorkFailure,
    ) -> Result<bool, StorageError> {
        let delay = Duration(1_000_u64.saturating_mul(1_u64 << item.attempts.min(8)));
        self.finish_work(
            item,
            now,
            "ready",
            now.checked_add(delay).ok_or(StorageError::NumericRange)?,
            Some(failure),
        )
    }
    /// Retains an unexecutable obligation for explicit operator resolution.
    /// # Errors
    /// Returns database or numeric errors.
    pub fn block_work(
        &self,
        item: &WorkItem,
        now: CanonicalTime,
        failure: WorkFailure,
    ) -> Result<bool, StorageError> {
        self.finish_work(item, now, "blocked", now, Some(failure))
    }
    fn finish_work(
        &self,
        item: &WorkItem,
        now: CanonicalTime,
        status: &str,
        available: CanonicalTime,
        failure: Option<WorkFailure>,
    ) -> Result<bool, StorageError> {
        if item.owner != item.payload.queue().owner(&self.aggregate_key) {
            return Ok(false);
        }
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        Ok(client.execute("UPDATE cs_work SET status=$4,available_at=$5,failure=$6,completed_at=CASE WHEN $4='complete' THEN $7 ELSE NULL END,claim_until=NULL WHERE id=$1 AND owner=$2 AND claim_token=$3 AND status='running' AND claim_until>$7",&[&item.id,&item.owner,&item.token,&status,&to_i64(available.0)?,&failure.map(WorkFailure::code),&to_i64(now.0)?])? == 1)
    }
    /// Host administration API: resume blocked work after resolving its recorded failure.
    /// # Errors
    /// Returns database or numeric errors.
    pub fn resume_work(
        &self,
        queue: WorkQueue,
        id: i64,
        now: CanonicalTime,
    ) -> Result<bool, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        Ok(client.execute("UPDATE cs_work SET status='ready',available_at=$3,failure=NULL WHERE id=$1 AND owner=$2 AND kind=$4 AND status='blocked'",&[&id,&queue.owner(&self.aggregate_key),&to_i64(now.0)?,&queue.kind()])?==1)
    }
}
