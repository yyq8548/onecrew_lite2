//! Explicit correction appends an audit row; the original evidence is immutable.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::submission::{decode, encoded, text};
use super::{
    DiscoveryIdentityReviewKind, DiscoveryIdentityReviewReceipt, StoredDiscoveryIdentityReview,
};
use crate::{CreatorRepositoryCallError, CreatorRuntime, DiscoveryReviewer};

pub(super) const POLICY: &str = "audited-native-identity-revocation-v1";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevokeDiscoveryIdentityReviewInput {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub decision_id: String,
    pub revokes_decision_id: String,
    pub reviewer: DiscoveryReviewer,
    pub rationale: String,
}

pub enum DiscoveryIdentityRevocationBasis {
    Committed(StoredDiscoveryIdentityReview),
    Original {
        stored: StoredDiscoveryIdentityReview,
        material_json: String,
    },
}

pub struct PreparedDiscoveryIdentityRevocation {
    pub(crate) original_request_hash: String,
    pub(crate) original_receipt_json: String,
    pub(crate) request_hash: String,
    pub(crate) input_json: String,
    pub(crate) material_json: String,
    pub(crate) receipt_json: String,
    pub(crate) receipt: DiscoveryIdentityReviewReceipt,
}

pub(super) fn request_hash(
    account: &str,
    input: &RevokeDiscoveryIdentityReviewInput,
) -> Result<String, String> {
    if !text(account, 256)
        || account.chars().any(char::is_control)
        || [
            &input.campaign_id,
            &input.decision_id,
            &input.revokes_decision_id,
        ]
        .iter()
        .any(|value| !text(value, 128) || value.chars().any(char::is_control))
        || input.expected_revision == 0
        || input.decision_id == input.revokes_decision_id
        || !text(&input.reviewer.label, 256)
        || !text(&input.rationale, 4000)
    {
        return Err("identity_review_input_invalid".into());
    }
    encoded(input, 65536)?;
    crate::discovery_library::digest(&(account, POLICY, input))
}

fn prepare(
    account: &str,
    input: RevokeDiscoveryIdentityReviewInput,
    hash: String,
    stored: StoredDiscoveryIdentityReview,
    material_json: String,
) -> Result<PreparedDiscoveryIdentityRevocation, String> {
    let original_request_hash = stored.request_hash.clone();
    let original_receipt_json = stored.receipt_json.clone();
    let mut receipt = decode(stored, account, None)?;
    if receipt.campaign_id != input.campaign_id || receipt.decision_id != input.revokes_decision_id
    {
        return Err("identity_review_receipt_invalid".into());
    }
    if receipt.kind == DiscoveryIdentityReviewKind::Revoke {
        return Err("identity_review_cannot_revoke_revocation".into());
    }
    receipt.criteria_revision = input.expected_revision;
    receipt.decision_id = input.decision_id.clone();
    receipt.policy_version = POLICY.into();
    receipt.kind = DiscoveryIdentityReviewKind::Revoke;
    receipt.revokes_decision_id = Some(input.revokes_decision_id.clone());
    receipt.created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "identity_review_clock_invalid")?
        .as_secs()
        .try_into()
        .map_err(|_| "identity_review_clock_invalid")?;
    Ok(PreparedDiscoveryIdentityRevocation {
        original_request_hash,
        original_receipt_json,
        request_hash: hash,
        input_json: encoded(&input, 65536)?,
        material_json,
        receipt_json: encoded(&receipt, 65536)?,
        receipt,
    })
}

impl CreatorRuntime {
    /// The authenticated account must own the original review. No caller-supplied
    /// native facts or group state are accepted, and later source changes do not
    /// prevent correcting the historical decision.
    pub fn revoke_discovery_identity_review(
        &self,
        account: &str,
        input: RevokeDiscoveryIdentityReviewInput,
    ) -> Result<DiscoveryIdentityReviewReceipt, String> {
        let hash = request_hash(account, &input)?;
        let basis = self.repository.read_discovery_identity_revocation(&input)?;
        let (stored, material_json) = match basis {
            DiscoveryIdentityRevocationBasis::Committed(stored) => {
                return decode(stored, account, Some(&hash));
            }
            DiscoveryIdentityRevocationBasis::Original {
                stored,
                material_json,
            } => (stored, material_json),
        };
        let prepared = prepare(account, input, hash.clone(), stored, material_json)?;
        decode(
            self.repository
                .commit_discovery_identity_revocation(&prepared)?,
            account,
            Some(&hash),
        )
    }

    pub async fn revoke_discovery_identity_review_async(
        &self,
        account: String,
        input: RevokeDiscoveryIdentityReviewInput,
    ) -> Result<DiscoveryIdentityReviewReceipt, CreatorRepositoryCallError<String>> {
        let (account, input, hash) = self
            .repository_cpu_segment(move || {
                let hash = request_hash(&account, &input)?;
                Ok((account, input, hash))
            })
            .await?;
        let repository = self.repository.clone();
        let (account, input, hash, basis) = self
            .repository_read_segment(move || {
                let basis = repository.read_discovery_identity_revocation(&input)?;
                Ok((account, input, hash, basis))
            })
            .await?;
        let (stored, material_json) = match basis {
            DiscoveryIdentityRevocationBasis::Committed(stored) => {
                return self
                    .repository_cpu_segment(move || decode(stored, &account, Some(&hash)))
                    .await;
            }
            DiscoveryIdentityRevocationBasis::Original {
                stored,
                material_json,
            } => (stored, material_json),
        };
        let trusted_account = account.clone();
        let prepared = self
            .repository_cpu_segment(move || prepare(&account, input, hash, stored, material_json))
            .await?;
        let repository = self.repository.clone();
        let stored = self
            .repository_write_segment(move || {
                repository.commit_discovery_identity_revocation(&prepared)
            })
            .await?;
        self.repository_cpu_segment(move || decode(stored, &trusted_account, None))
            .await
    }
}

#[cfg(test)]
mod tests;
