//! One durable execution lifecycle for external effects. Claims are opaque and fenced.
use super::{
    CanonicalTime, Client, Deserialize, Duration, EffectIntent, JournalPosition, Json,
    PostgresDeployment, PostgresEngine, Serialize, SettlementUnit, StorageError, Transaction,
    to_i64,
};
use cs_mail_primitives::{
    AllocationId, AnnualDistributionId, BillingAccountId, PaymentOperationId,
};

/// Work owned by one relationship aggregate, claimed through its [`PostgresEngine`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelationshipQueue {
    Delivery,
    RequestPayments,
    Artifacts,
}
impl RelationshipQueue {
    const fn kind(self) -> &'static str {
        match self {
            Self::Delivery => "delivery",
            Self::RequestPayments => "request-payment",
            Self::Artifacts => "artifacts",
        }
    }
    fn owner(aggregate: &str) -> String {
        format!("relationship:{aggregate}")
    }
}
/// Work owned by the deployment (billing and the financial program), claimed through
/// [`PostgresDeployment`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeploymentQueue {
    UtilityPayments,
    AnnualAllocations(SettlementUnit),
    DistributionPreparation,
    MemberPayments(SettlementUnit),
}
impl DeploymentQueue {
    const fn kind(self) -> &'static str {
        match self {
            Self::UtilityPayments => "utility-payment",
            Self::AnnualAllocations(_) => "annual-allocation",
            Self::DistributionPreparation => "prepare-distribution",
            Self::MemberPayments(_) => "member-payment",
        }
    }
    fn owner(self) -> String {
        match self {
            Self::UtilityPayments | Self::DistributionPreparation => "billing".into(),
            Self::MemberPayments(unit) | Self::AnnualAllocations(unit) => {
                format!("program:{}", unit.0)
            }
        }
    }
}
#[derive(Clone, Copy)]
enum Queue {
    Relationship(RelationshipQueue),
    Deployment(DeploymentQueue),
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
    const fn queue(&self) -> Queue {
        match self {
            Self::AnnualAllocation { unit, .. } => {
                Queue::Deployment(DeploymentQueue::AnnualAllocations(*unit))
            }
            Self::PrepareDistribution { .. } => {
                Queue::Deployment(DeploymentQueue::DistributionPreparation)
            }
            Self::UtilityPayment { .. } => Queue::Deployment(DeploymentQueue::UtilityPayments),
            Self::MemberPayment { unit, .. } => {
                Queue::Deployment(DeploymentQueue::MemberPayments(*unit))
            }
            Self::Effect(EffectIntent::ExecutePayment { .. }) => {
                Queue::Relationship(RelationshipQueue::RequestPayments)
            }
            Self::Effect(_) => Queue::Relationship(RelationshipQueue::Delivery),
            Self::Artifacts { .. } => Queue::Relationship(RelationshipQueue::Artifacts),
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

/// Enqueues relationship-owned work produced by a relationship transaction.
pub(super) fn enqueue(
    tx: &mut Transaction<'_>,
    key: &str,
    source: WorkSource,
    payload: &WorkPayload,
    now: CanonicalTime,
) -> Result<(), StorageError> {
    let Queue::Relationship(queue) = payload.queue() else {
        return Err(StorageError::WorkScopeMismatch);
    };
    insert(
        tx,
        &RelationshipQueue::owner(key),
        Some(key),
        queue.kind(),
        0,
        source,
        payload,
        now,
    )
}
/// Enqueues deployment-owned work (billing and the financial program).
pub(super) fn enqueue_deployment(
    tx: &mut Transaction<'_>,
    source: WorkSource,
    payload: &WorkPayload,
    now: CanonicalTime,
) -> Result<(), StorageError> {
    let Queue::Deployment(queue) = payload.queue() else {
        return Err(StorageError::WorkScopeMismatch);
    };
    insert(
        tx,
        &queue.owner(),
        None,
        queue.kind(),
        now.0,
        source,
        payload,
        now,
    )
}
#[allow(clippy::too_many_arguments)]
fn insert(
    tx: &mut Transaction<'_>,
    owner: &str,
    aggregate: Option<&str>,
    kind: &str,
    available_at: u64,
    source: WorkSource,
    payload: &WorkPayload,
    now: CanonicalTime,
) -> Result<(), StorageError> {
    let content = match payload {
        WorkPayload::Effect(EffectIntent::DeliverMessage { content_ref, .. }) => {
            Some(content_ref.0.to_string())
        }
        _ => None,
    };
    let (work_key, position) = match source {
        WorkSource::Receipt { position, ordinal } => (
            format!("{kind}:{}:{ordinal}", position.0),
            Some(to_i64(position.0)?),
        ),
        WorkSource::AnnualAllocation(id) => (format!("annual-allocation:{}", id.0), None),
        WorkSource::PrepareDistribution(allocation) => {
            (format!("prepare-distribution:{}", allocation.0), None)
        }
        WorkSource::PaymentAttempt(id) => (format!("payout:{}", id.0), None),
    };
    tx.execute("INSERT INTO cs_work(owner,aggregate_key,work_key,kind,payload,content_ref,status,available_at,created_at,journal_position) VALUES($1,$2,$3,$4,$5,$6,'ready',$9,$7,$8) ON CONFLICT(owner,work_key) DO NOTHING", &[&owner, &aggregate, &work_key, &kind, &Json(payload), &content, &to_i64(now.0)?, &position, &to_i64(available_at)?])?;
    Ok(())
}

/// Acknowledgement of claimed work, fenced by the claim token. Implemented by both the
/// deployment and relationship handles for the work each owns.
pub trait WorkClaims {
    /// Acknowledges only this claim, never another worker's reclaimed operation.
    /// # Errors
    /// Returns database or numeric errors.
    fn complete_work(&self, item: &WorkItem, now: CanonicalTime) -> Result<bool, StorageError>;
    /// Releases failed work with capped exponential backoff; uncertainty preserves its identity.
    /// # Errors
    /// Returns database or clock overflow errors.
    fn retry_work(
        &self,
        item: &WorkItem,
        now: CanonicalTime,
        failure: WorkFailure,
    ) -> Result<bool, StorageError>;
    /// Retains an unexecutable obligation for explicit operator resolution.
    /// # Errors
    /// Returns database or numeric errors.
    fn block_work(
        &self,
        item: &WorkItem,
        now: CanonicalTime,
        failure: WorkFailure,
    ) -> Result<bool, StorageError>;
}

fn claim(
    client: &std::sync::Mutex<Client>,
    owner: &str,
    kind: &str,
    now: CanonicalTime,
    lease: Duration,
    limit: i64,
) -> Result<Vec<WorkItem>, StorageError> {
    if lease.0 == 0 || limit < 0 {
        return Err(StorageError::NumericRange);
    }
    let until = to_i64(now.checked_add(lease).ok_or(StorageError::NumericRange)?.0)?;
    let mut client = client.lock().map_err(|_| StorageError::LockPoisoned)?;
    let rows = client.query("WITH candidates AS (SELECT id FROM cs_work WHERE owner=$1 AND kind=$2 AND ((status='ready' AND available_at<=$3) OR (status='running' AND claim_until<=$3)) ORDER BY available_at,id LIMIT $4 FOR UPDATE SKIP LOCKED) UPDATE cs_work w SET status='running',claim_until=$5,claim_token=nextval('cs_work_claim_token'),attempts=attempts+1 FROM candidates c WHERE w.id=c.id RETURNING w.id,w.payload,w.attempts,w.owner,w.claim_token,w.aggregate_key,w.journal_position", &[&owner,&kind,&to_i64(now.0)?,&limit,&until])?;
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

fn finish(
    client: &std::sync::Mutex<Client>,
    expected_owner: Option<&str>,
    item: &WorkItem,
    now: CanonicalTime,
    status: &str,
    available: CanonicalTime,
    failure: Option<WorkFailure>,
) -> Result<bool, StorageError> {
    if expected_owner != Some(item.owner.as_str()) {
        return Ok(false);
    }
    let mut client = client.lock().map_err(|_| StorageError::LockPoisoned)?;
    Ok(client.execute("UPDATE cs_work SET status=$4,available_at=$5,failure=$6,completed_at=CASE WHEN $4='complete' THEN $7 ELSE NULL END,claim_until=NULL WHERE id=$1 AND owner=$2 AND claim_token=$3 AND status='running' AND claim_until>$7",&[&item.id,&item.owner,&item.token,&status,&to_i64(available.0)?,&failure.map(WorkFailure::code),&to_i64(now.0)?])? == 1)
}

fn retry_delay(item: &WorkItem, now: CanonicalTime) -> Result<CanonicalTime, StorageError> {
    let delay = Duration(1_000_u64.saturating_mul(1_u64 << item.attempts.min(8)));
    now.checked_add(delay).ok_or(StorageError::NumericRange)
}

fn resume(
    client: &std::sync::Mutex<Client>,
    owner: &str,
    kind: &str,
    id: i64,
    now: CanonicalTime,
) -> Result<bool, StorageError> {
    let mut client = client.lock().map_err(|_| StorageError::LockPoisoned)?;
    Ok(client.execute("UPDATE cs_work SET status='ready',available_at=$3,failure=NULL WHERE id=$1 AND owner=$2 AND kind=$4 AND status='blocked'",&[&id,&owner,&to_i64(now.0)?,&kind])?==1)
}

impl PostgresEngine {
    fn owned(&self, item: &WorkItem) -> Option<String> {
        matches!(item.payload.queue(), Queue::Relationship(_))
            .then(|| RelationshipQueue::owner(&self.aggregate_key))
    }
    /// Claims a bounded batch of this relationship's work. Reclaiming always assigns a new
    /// fencing token.
    /// # Errors
    /// Rejects zero leases, negative limits, clock overflow and database failures.
    pub fn claim_work(
        &self,
        queue: RelationshipQueue,
        now: CanonicalTime,
        lease: Duration,
        limit: i64,
    ) -> Result<Vec<WorkItem>, StorageError> {
        claim(
            &self.client,
            &RelationshipQueue::owner(&self.aggregate_key),
            queue.kind(),
            now,
            lease,
            limit,
        )
    }
    /// Host administration API: resume blocked work after resolving its recorded failure.
    /// # Errors
    /// Returns database or numeric errors.
    pub fn resume_work(
        &self,
        queue: RelationshipQueue,
        id: i64,
        now: CanonicalTime,
    ) -> Result<bool, StorageError> {
        resume(
            &self.client,
            &RelationshipQueue::owner(&self.aggregate_key),
            queue.kind(),
            id,
            now,
        )
    }
}
impl WorkClaims for PostgresEngine {
    fn complete_work(&self, item: &WorkItem, now: CanonicalTime) -> Result<bool, StorageError> {
        finish(
            &self.client,
            self.owned(item).as_deref(),
            item,
            now,
            "complete",
            now,
            None,
        )
    }
    fn retry_work(
        &self,
        item: &WorkItem,
        now: CanonicalTime,
        failure: WorkFailure,
    ) -> Result<bool, StorageError> {
        let available = retry_delay(item, now)?;
        finish(
            &self.client,
            self.owned(item).as_deref(),
            item,
            now,
            "ready",
            available,
            Some(failure),
        )
    }
    fn block_work(
        &self,
        item: &WorkItem,
        now: CanonicalTime,
        failure: WorkFailure,
    ) -> Result<bool, StorageError> {
        finish(
            &self.client,
            self.owned(item).as_deref(),
            item,
            now,
            "blocked",
            now,
            Some(failure),
        )
    }
}

impl PostgresDeployment {
    fn owned(item: &WorkItem) -> Option<String> {
        match item.payload.queue() {
            Queue::Deployment(queue) => Some(queue.owner()),
            Queue::Relationship(_) => None,
        }
    }
    /// Claims a bounded batch of deployment-owned work. Reclaiming always assigns a new
    /// fencing token.
    /// # Errors
    /// Rejects zero leases, negative limits, clock overflow and database failures.
    pub fn claim_work(
        &self,
        queue: DeploymentQueue,
        now: CanonicalTime,
        lease: Duration,
        limit: i64,
    ) -> Result<Vec<WorkItem>, StorageError> {
        claim(
            &self.client,
            &queue.owner(),
            queue.kind(),
            now,
            lease,
            limit,
        )
    }
    /// Host administration API: resume blocked work after resolving its recorded failure.
    /// # Errors
    /// Returns database or numeric errors.
    pub fn resume_work(
        &self,
        queue: DeploymentQueue,
        id: i64,
        now: CanonicalTime,
    ) -> Result<bool, StorageError> {
        resume(&self.client, &queue.owner(), queue.kind(), id, now)
    }
}
impl WorkClaims for PostgresDeployment {
    fn complete_work(&self, item: &WorkItem, now: CanonicalTime) -> Result<bool, StorageError> {
        finish(
            &self.client,
            Self::owned(item).as_deref(),
            item,
            now,
            "complete",
            now,
            None,
        )
    }
    fn retry_work(
        &self,
        item: &WorkItem,
        now: CanonicalTime,
        failure: WorkFailure,
    ) -> Result<bool, StorageError> {
        let available = retry_delay(item, now)?;
        finish(
            &self.client,
            Self::owned(item).as_deref(),
            item,
            now,
            "ready",
            available,
            Some(failure),
        )
    }
    fn block_work(
        &self,
        item: &WorkItem,
        now: CanonicalTime,
        failure: WorkFailure,
    ) -> Result<bool, StorageError> {
        finish(
            &self.client,
            Self::owned(item).as_deref(),
            item,
            now,
            "blocked",
            now,
            Some(failure),
        )
    }
}
