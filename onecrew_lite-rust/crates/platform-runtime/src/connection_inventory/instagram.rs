use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{LocalConnectionAvailability, shared};
use crate::{OperatorAccountPlatform, OperatorAccountProjection};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InstagramConnectionSummary {
    pub operator_account_id: String,
    pub username: String,
    pub label: String,
    pub availability: LocalConnectionAvailability,
}

pub(super) fn summaries(accounts: &[OperatorAccountProjection]) -> Vec<InstagramConnectionSummary> {
    let mut summaries = accounts
        .iter()
        .filter(|account| account.platform == OperatorAccountPlatform::Instagram)
        .filter(|account| !account.operator_account_id.trim().is_empty())
        .map(|account| InstagramConnectionSummary {
            operator_account_id: account.operator_account_id.clone(),
            username: account.username.clone(),
            label: account.label.clone(),
            availability: shared::local_availability(shared::operator_is_locally_available(
                account,
            )),
        })
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then_with(|| left.operator_account_id.cmp(&right.operator_account_id))
    });
    summaries
}
