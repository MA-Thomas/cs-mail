//! Fixed annual service contracts. Pool distributions are owned by finance.
mod commands;
pub use commands::{BillingCommand, SignedBillingCommand};
use cs_mail_finance::{
    BankVerification, FinancialScope, PaymentChange, PaymentError, PaymentExecution, PaymentKind,
    PaymentOperation, PaymentState, SignedPaymentEvidence, VerifiedBankAccount,
};
use cs_mail_ledger::{Account, LedgerBatch};
use cs_mail_primitives::{
    BillingAccountId, CanonicalTime, MemberId, Money, PaymentOperationId, PolicyVersion,
    ServiceContractId, SettlementUnit, calendar_date,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LeapDayRule {
    February28,
    March1,
}
/// A calendar-year or anniversary-year interval with an explicit leap-day convention.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "PeriodAnchor", into = "PeriodAnchor")]
pub struct ServicePeriod {
    anchor: PeriodAnchor,
    start: CanonicalTime,
    end: CanonicalTime,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct PeriodAnchor {
    year: u16,
    month: u8,
    day: u8,
    leap_day: LeapDayRule,
}
impl ServicePeriod {
    /// # Errors
    /// Rejects invalid dates or a service year outside the supported calendar.
    pub fn annual(
        year: u16,
        month: u8,
        day: u8,
        leap_day: LeapDayRule,
    ) -> Result<Self, BillingError> {
        let anchor = PeriodAnchor {
            year,
            month,
            day,
            leap_day,
        };
        let date = |year| {
            calendar_date(year, month, day)
                .or_else(|| {
                    if month != 2 || day != 29 {
                        return None;
                    }
                    let (m, d) = match leap_day {
                        LeapDayRule::February28 => (2, 28),
                        LeapDayRule::March1 => (3, 1),
                    };
                    calendar_date(year, m, d)
                })
                .ok_or(BillingError::InvalidSchedule)
        };
        Ok(Self {
            anchor,
            start: date(year)?,
            end: date(year.checked_add(1).ok_or(BillingError::Overflow)?)?,
        })
    }
    pub const fn start(self) -> CanonicalTime {
        self.start
    }
    pub const fn end(self) -> CanonicalTime {
        self.end
    }
    pub fn contains(self, at: CanonicalTime) -> bool {
        self.start <= at && at < self.end
    }
}
impl TryFrom<PeriodAnchor> for ServicePeriod {
    type Error = BillingError;
    fn try_from(v: PeriodAnchor) -> Result<Self, Self::Error> {
        Self::annual(v.year, v.month, v.day, v.leap_day)
    }
}
impl From<ServicePeriod> for PeriodAnchor {
    fn from(v: ServicePeriod) -> Self {
        v.anchor
    }
}

/// Published terms. A purchase references this immutable policy rather than supplying prices.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "StoredOffer", into = "StoredOffer")]
pub struct ServiceOffer {
    version: PolicyVersion,
    period: ServicePeriod,
    collect_at: CanonicalTime,
    price: Money,
    unit: SettlementUnit,
}
#[derive(Deserialize, Serialize)]
struct StoredOffer {
    version: PolicyVersion,
    period: ServicePeriod,
    collect_at: CanonicalTime,
    price: Money,
    unit: SettlementUnit,
}
impl ServiceOffer {
    /// # Errors
    /// Rejects invalid or inconsistent domain inputs.
    pub fn new(
        version: PolicyVersion,
        period: ServicePeriod,
        collect_at: CanonicalTime,
        price: Money,
        unit: SettlementUnit,
    ) -> Result<Self, BillingError> {
        if version.0 == 0 || price.is_zero() || collect_at >= period.start() {
            return Err(BillingError::InvalidSchedule);
        }
        Ok(Self {
            version,
            period,
            collect_at,
            price,
            unit,
        })
    }
    pub const fn version(&self) -> PolicyVersion {
        self.version
    }
    pub const fn period(&self) -> ServicePeriod {
        self.period
    }
    pub const fn collect_at(&self) -> CanonicalTime {
        self.collect_at
    }
    pub const fn price(&self) -> Money {
        self.price
    }
    pub const fn unit(&self) -> SettlementUnit {
        self.unit
    }
}
impl TryFrom<StoredOffer> for ServiceOffer {
    type Error = BillingError;
    fn try_from(v: StoredOffer) -> Result<Self, Self::Error> {
        Self::new(v.version, v.period, v.collect_at, v.price, v.unit)
    }
}
impl From<ServiceOffer> for StoredOffer {
    fn from(v: ServiceOffer) -> Self {
        Self {
            version: v.version,
            period: v.period,
            collect_at: v.collect_at,
            price: v.price,
            unit: v.unit,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
enum ServiceSettlement {
    Unfunded,
    Funded { at: CanonicalTime },
    Reversed { funded_at: CanonicalTime },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "StoredContract", into = "StoredContract")]
pub struct ServiceContract {
    id: ServiceContractId,
    offer: ServiceOffer,
    processor_key: [u8; 32],
    collection: PaymentExecution,
    settlement: ServiceSettlement,
}
#[derive(Deserialize, Serialize)]
struct StoredContract {
    id: ServiceContractId,
    offer: ServiceOffer,
    processor_key: [u8; 32],
    collection: PaymentExecution,
    settlement: ServiceSettlement,
}
impl ServiceContract {
    pub const fn id(&self) -> ServiceContractId {
        self.id
    }
    pub fn offer(&self) -> &ServiceOffer {
        &self.offer
    }
    pub fn collection(&self) -> &PaymentExecution {
        &self.collection
    }
    pub fn processor_key(&self) -> &[u8; 32] {
        &self.processor_key
    }
    pub fn covers(&self, at: CanonicalTime) -> bool {
        matches!(self.settlement, ServiceSettlement::Funded {at:paid} if paid<=at)
            && self.offer.period.contains(at)
    }
    fn retry(&mut self) -> Result<PaymentOperation, BillingError> {
        let id = operation_id(
            self.collection.current().scope,
            b"service-attempt",
            self.id.0,
            self.collection.attempt_count() as u128,
        );
        Ok(self.collection.retry(id)?)
    }
    fn record(
        &mut self,
        receipt: &SignedPaymentEvidence,
        at: CanonicalTime,
    ) -> Result<LedgerBatch, BillingError> {
        if at < self.offer.collect_at {
            return Err(BillingError::TooEarly);
        }
        let change = self.collection.record(receipt, &self.processor_key)?;
        let mut batch = LedgerBatch::new();
        match change {
            PaymentChange::Settled => {
                self.settlement = ServiceSettlement::Funded { at };
                batch.transfer(
                    Account::ProcessorClearing,
                    Account::UtilityServiceReceipts(PaymentOperationId(self.id.0)),
                    self.offer.price,
                );
            }
            PaymentChange::Reversed => {
                let ServiceSettlement::Funded { at } = self.settlement else {
                    return Err(BillingError::Conflict);
                };
                self.settlement = ServiceSettlement::Reversed { funded_at: at };
                batch.transfer(
                    Account::CorporateLossClearing,
                    Account::ProcessorClearing,
                    self.offer.price,
                );
            }
            PaymentChange::None => {}
        }
        Ok(batch)
    }
    fn validate(&self) -> Result<(), BillingError> {
        self.collection.validate()?;
        let op = self.collection.current();
        if self.id.0 == 0
            || self
                .collection
                .operation(PaymentOperationId(self.id.0))
                .is_none()
            || self.processor_key == [0; 32]
            || op.kind != PaymentKind::Capture
            || op.amount != self.offer.price
            || op.unit != self.offer.unit
        {
            return Err(BillingError::Conflict);
        }
        match (self.collection.state(), self.settlement) {
            (
                PaymentState::Pending | PaymentState::Failed { .. } | PaymentState::Voided { .. },
                ServiceSettlement::Unfunded,
            ) => Ok(()),
            (PaymentState::Settled { .. }, ServiceSettlement::Funded { at })
            | (PaymentState::Reversed { .. }, ServiceSettlement::Reversed { funded_at: at })
                if at >= self.offer.collect_at =>
            {
                Ok(())
            }
            _ => Err(BillingError::Conflict),
        }
    }
}
impl TryFrom<StoredContract> for ServiceContract {
    type Error = BillingError;
    fn try_from(v: StoredContract) -> Result<Self, Self::Error> {
        let result = Self {
            id: v.id,
            offer: v.offer,
            processor_key: v.processor_key,
            collection: v.collection,
            settlement: v.settlement,
        };
        result.validate()?;
        Ok(result)
    }
}
impl From<ServiceContract> for StoredContract {
    fn from(v: ServiceContract) -> Self {
        Self {
            id: v.id,
            offer: v.offer,
            processor_key: v.processor_key,
            collection: v.collection,
            settlement: v.settlement,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AccountStatus {
    Open,
    Closed,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "StoredAccount", into = "StoredAccount")]
pub struct BillingAccount {
    bank: VerifiedBankAccount,
    revision: u64,
    status: AccountStatus,
    contracts: BTreeMap<ServiceContractId, ServiceContract>,
}
#[derive(Deserialize, Serialize)]
struct StoredAccount {
    bank: BankVerification,
    verification_authority: [u8; 32],
    revision: u64,
    status: AccountStatus,
    contracts: BTreeMap<ServiceContractId, ServiceContract>,
}
impl BillingAccount {
    pub fn new(bank: VerifiedBankAccount) -> Self {
        Self {
            bank,
            revision: 0,
            status: AccountStatus::Open,
            contracts: BTreeMap::new(),
        }
    }
    pub fn id(&self) -> BillingAccountId {
        self.bank.evidence().account
    }
    pub fn scope(&self) -> FinancialScope {
        self.bank.evidence().scope
    }
    pub fn unit(&self) -> SettlementUnit {
        self.bank.evidence().unit
    }
    pub fn member(&self) -> MemberId {
        self.bank.evidence().member
    }
    pub fn bank(&self) -> &VerifiedBankAccount {
        &self.bank
    }
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    pub const fn status(&self) -> AccountStatus {
        self.status
    }
    pub fn contracts(&self) -> &BTreeMap<ServiceContractId, ServiceContract> {
        &self.contracts
    }
    pub fn covers(&self, at: CanonicalTime) -> bool {
        self.status == AccountStatus::Open && self.contracts.values().any(|c| c.covers(at))
    }
    /// # Errors
    /// Rejects revision overflow; existing obligations remain intact.
    pub fn close(&mut self) -> Result<(), BillingError> {
        if self.status == AccountStatus::Open {
            self.bump()?;
            self.status = AccountStatus::Closed;
        }
        Ok(())
    }
    fn bump(&mut self) -> Result<(), BillingError> {
        self.revision = self.revision.checked_add(1).ok_or(BillingError::Overflow)?;
        Ok(())
    }
    /// # Errors
    /// Rejects closure, overlapping service periods, changed contract terms, or invalid processor authority.
    pub fn purchase(
        &mut self,
        offer: &ServiceOffer,
        processor_key: [u8; 32],
    ) -> Result<ServiceContractId, BillingError> {
        if self.status != AccountStatus::Open
            || offer.unit != self.unit()
            || processor_key == [0; 32]
        {
            return Err(BillingError::Conflict);
        }
        let id = ServiceContractId(
            operation_id(
                self.scope(),
                b"service-contract",
                self.id().0,
                u128::from(offer.period.start().0),
            )
            .0,
        );
        if let Some(old) = self.contracts.get(&id) {
            return if old.offer == *offer && old.processor_key == processor_key {
                Ok(id)
            } else {
                Err(BillingError::Conflict)
            };
        }
        if self.contracts.values().any(|c| {
            c.offer.period.start() < offer.period.end()
                && offer.period.start() < c.offer.period.end()
        }) {
            return Err(BillingError::Conflict);
        }
        let operation = PaymentOperation {
            scope: self.scope(),
            id: PaymentOperationId(id.0),
            kind: PaymentKind::Capture,
            amount: offer.price,
            unit: self.unit(),
            destination: self.bank.evidence().bank_token,
        };
        let contract = ServiceContract {
            id,
            offer: offer.clone(),
            processor_key,
            collection: PaymentExecution::new(operation)?,
            settlement: ServiceSettlement::Unfunded,
        };
        self.bump()?;
        self.contracts.insert(id, contract);
        Ok(id)
    }
    /// # Errors
    /// Rejects a closed account, an unknown contract, or collection without definitive failure.
    pub fn retry_collection(
        &mut self,
        id: ServiceContractId,
    ) -> Result<PaymentOperation, BillingError> {
        if self.status != AccountStatus::Open {
            return Err(BillingError::Conflict);
        }
        let mut contract = self
            .contracts
            .get(&id)
            .cloned()
            .ok_or(BillingError::MissingRecord)?;
        let operation = contract.retry()?;
        self.bump()?;
        self.contracts.insert(id, contract);
        Ok(operation)
    }
    /// # Errors
    /// Rejects unknown contracts, early confirmation, contradictory evidence, or revision overflow.
    pub fn record_collection(
        &mut self,
        id: ServiceContractId,
        receipt: &SignedPaymentEvidence,
        at: CanonicalTime,
    ) -> Result<LedgerBatch, BillingError> {
        let previous = self.contracts.get(&id).ok_or(BillingError::MissingRecord)?;
        let mut next = previous.clone();
        let batch = next.record(receipt, at)?;
        if &next != previous {
            self.bump()?;
            self.contracts.insert(id, next);
        }
        Ok(batch)
    }
}
impl From<BillingAccount> for StoredAccount {
    fn from(v: BillingAccount) -> Self {
        Self {
            bank: v.bank.evidence().clone(),
            verification_authority: *v.bank.authority(),
            revision: v.revision,
            status: v.status,
            contracts: v.contracts,
        }
    }
}
impl TryFrom<StoredAccount> for BillingAccount {
    type Error = BillingError;
    fn try_from(v: StoredAccount) -> Result<Self, Self::Error> {
        let bank = v.bank.verify(&v.verification_authority)?;
        let mut periods = Vec::new();
        for (id, c) in &v.contracts {
            let op = c.collection.current();
            if *id != c.id
                || op.scope != v.bank.scope
                || op.unit != v.bank.unit
                || op.destination != v.bank.bank_token
            {
                return Err(BillingError::Conflict);
            }
            periods.push(c.offer.period);
        }
        periods.sort_by_key(|p| p.start());
        if periods.windows(2).any(|p| p[0].end() > p[1].start()) {
            return Err(BillingError::Conflict);
        }
        Ok(Self {
            bank,
            revision: v.revision,
            status: v.status,
            contracts: v.contracts,
        })
    }
}
pub fn operation_id(
    scope: FinancialScope,
    purpose: &[u8],
    owner: u128,
    cycle: u128,
) -> PaymentOperationId {
    let mut h = Sha256::new();
    h.update(b"cs-mail/billing-operation/v2");
    h.update(scope.canonical_bytes());
    h.update((purpose.len() as u64).to_be_bytes());
    h.update(purpose);
    h.update(owner.to_be_bytes());
    h.update(cycle.to_be_bytes());
    let digest = h.finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    PaymentOperationId(u128::from_be_bytes(bytes))
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BillingError {
    ServiceNotCovered,
    InvalidSchedule,
    Conflict,
    Overflow,
    TooEarly,
    MissingRecord,
    Payment(PaymentError),
}
impl From<PaymentError> for BillingError {
    fn from(e: PaymentError) -> Self {
        Self::Payment(e)
    }
}
impl core::fmt::Display for BillingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BillingError {}
