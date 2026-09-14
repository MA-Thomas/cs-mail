use crate::{DAY_MILLIS, MembershipStatus, QuarterSchedule};
use cs_mail_primitives::{CanonicalTime, MemberId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Member {
    pub(crate) id: MemberId,
    pub(crate) identity_digest: [u8; 32],
    pub(crate) joined_at: CanonicalTime,
    pub(crate) changes: Vec<(CanonicalTime, MembershipStatus)>,
    pub(crate) activity_days: BTreeSet<u64>,
}

impl Member {
    pub(crate) fn qualifies(&self, schedule: &QuarterSchedule) -> bool {
        let eligible = self
            .changes
            .iter()
            .rev()
            .find(|(t, _)| *t < schedule.cutoff)
            .is_some_and(|(_, s)| s.opted_in && s.verified && !s.suspended);
        let old_enough = self
            .joined_at
            .checked_add(schedule.eligibility.minimum_tenure)
            .is_some_and(|t| t <= schedule.cutoff);
        let days = self
            .activity_days
            .range(schedule.start.0 / DAY_MILLIS..schedule.cutoff.0 / DAY_MILLIS)
            .count();
        eligible && old_enough && days >= schedule.eligibility.minimum_active_days as usize
    }
}
