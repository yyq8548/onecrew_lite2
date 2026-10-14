//! Read-only identity groups, retaining each native account's independent eligibility.
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

use super::{
    DiscoveryIdentityReviewKind, DiscoveryIdentityReviewReceipt,
    RevokeDiscoveryIdentityReviewInput, SubmitDiscoveryIdentityReviewInput, prepare_pair,
    relationships, revocation, submission,
};
use crate::{
    CreatorRepositoryCallError, CreatorRuntime, DiscoveryLibraryBasis, DiscoveryLibraryRow,
    DiscoveryReviewBasis,
};

#[derive(Clone, PartialEq)]
pub struct DiscoveryIdentityGroupsBasis {
    pub(crate) library: DiscoveryLibraryBasis,
    pub(crate) native_details: BTreeMap<String, (String, String)>,
    pub(crate) reviews: Vec<IdentityGroupReviewSource>,
}

#[derive(Clone, PartialEq)]
pub(crate) struct IdentityGroupReviewSource {
    pub decision_id: String,
    pub account: String,
    pub revision: u32,
    pub kind: String,
    pub candidate_ids: [String; 2],
    pub material_hash: String,
    pub request_hash: String,
    pub input_json: String,
    pub receipt_json: String,
    pub revokes_decision_id: Option<String>,
    pub created_at: i64,
    pub current_pair: Option<Box<[DiscoveryReviewBasis; 2]>>,
}

#[derive(Debug, Clone, Copy, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryIdentityGroupReviewState {
    Current,
    EvidenceChanged,
    Revoked,
    PreviousCriteria,
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryIdentityGroupReview {
    pub receipt: DiscoveryIdentityReviewReceipt,
    pub state: DiscoveryIdentityGroupReviewState,
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryIdentityGroupMember {
    pub candidate_id: String,
    pub platform: String,
    pub native_author_id: String,
    pub display_name: String,
    pub profile_url: String,
    pub verdict: String,
    pub reasons: Vec<String>,
    pub sources: Vec<DiscoveryLibraryRow>,
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryIdentityGroup {
    /// Deterministic membership key, not a persisted person fact or an editable note key.
    pub group_id: String,
    pub has_same_author_review: bool,
    pub qualified_member_count: u32,
    pub members: Vec<DiscoveryIdentityGroupMember>,
    pub decision_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryIdentityGroupCounts {
    pub native_accounts: u32,
    pub qualified_native_accounts: u32,
    pub identity_groups: u32,
    pub qualified_identity_groups: u32,
    pub qualified_native_only_groups: u32,
    pub unbound_source_hits: u32,
    pub stale_identity_reviews: u32,
}

/// Shared by peer adapters and Library. Identity-group counts do not certify that
/// all groups are distinct natural people; missing identity evidence stays visible.
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryIdentityGroupsSnapshot {
    pub campaign_id: String,
    pub criteria_revision: u32,
    pub snapshot_hash: String,
    pub groups: Vec<DiscoveryIdentityGroup>,
    pub reviews: Vec<DiscoveryIdentityGroupReview>,
    pub counts: DiscoveryIdentityGroupCounts,
}

fn receipt(
    source: &IdentityGroupReviewSource,
    campaign: &str,
) -> Result<DiscoveryIdentityReviewReceipt, String> {
    let receipt: DiscoveryIdentityReviewReceipt =
        serde_json::from_str(&source.receipt_json).map_err(|_| "identity_groups_ledger_invalid")?;
    if receipt.campaign_id != campaign
        || receipt.decision_id != source.decision_id
        || receipt.operator_account_id != source.account
        || receipt.criteria_revision != source.revision
        || receipt.kind.as_str() != source.kind
        || receipt.candidate_ids != source.candidate_ids
        || receipt.material_hash != source.material_hash
        || receipt.revokes_decision_id != source.revokes_decision_id
        || receipt.created_at != source.created_at
    {
        return Err("identity_groups_ledger_invalid".into());
    }
    if receipt.kind == DiscoveryIdentityReviewKind::Revoke {
        let input: RevokeDiscoveryIdentityReviewInput = serde_json::from_str(&source.input_json)
            .map_err(|_| "identity_groups_ledger_invalid")?;
        if input.campaign_id != campaign
            || input.expected_revision != source.revision
            || input.decision_id != source.decision_id
            || Some(&input.revokes_decision_id) != source.revokes_decision_id.as_ref()
            || receipt.policy_version != revocation::POLICY
            || revocation::request_hash(&source.account, &input)? != source.request_hash
        {
            return Err("identity_groups_ledger_invalid".into());
        }
    } else {
        let input: SubmitDiscoveryIdentityReviewInput = serde_json::from_str(&source.input_json)
            .map_err(|_| "identity_groups_ledger_invalid")?;
        let kind = match input.decision {
            super::DiscoveryIdentityDecision::SameAuthor => DiscoveryIdentityReviewKind::SameAuthor,
            super::DiscoveryIdentityDecision::DifferentAuthors => {
                DiscoveryIdentityReviewKind::DifferentAuthors
            }
            super::DiscoveryIdentityDecision::Insufficient => {
                DiscoveryIdentityReviewKind::Insufficient
            }
        };
        if input.pair.campaign_id != campaign
            || input.pair.expected_revision != source.revision
            || input.decision_id != source.decision_id
            || input.material_hash != source.material_hash
            || kind != receipt.kind
            || receipt.policy_version != submission::POLICY
            || submission::request_hash(&source.account, &input)? != source.request_hash
        {
            return Err("identity_groups_ledger_invalid".into());
        }
    }
    Ok(receipt)
}

fn current_review(source: &IdentityGroupReviewSource) -> Result<bool, String> {
    let Some(basis) = &source.current_pair else {
        return Ok(false);
    };
    let input: SubmitDiscoveryIdentityReviewInput =
        serde_json::from_str(&source.input_json).map_err(|_| "identity_groups_ledger_invalid")?;
    let pair = prepare_pair(
        &input.pair,
        crate::discovery_review::build_material(basis[0].clone())?,
        crate::discovery_review::build_material(basis[1].clone())?,
    )?;
    if pair.material_hash != source.material_hash {
        return Ok(false);
    }
    let original: DiscoveryIdentityReviewReceipt =
        serde_json::from_str(&source.receipt_json).map_err(|_| "identity_groups_ledger_invalid")?;
    if original.candidate_ids != pair.accounts.each_ref().map(|a| a.candidate_id.clone())
        || original.native_author_ids
            != pair.accounts.each_ref().map(|a| a.native_author_id.clone())
        || original.platforms
            != pair
                .accounts
                .each_ref()
                .map(|a| a.author_platform().to_owned())
    {
        return Err("identity_groups_ledger_invalid".into());
    }
    submission::validate_findings(&input, &pair)?;
    Ok(true)
}

pub(crate) fn build(
    basis: DiscoveryIdentityGroupsBasis,
) -> Result<DiscoveryIdentityGroupsSnapshot, String> {
    let library = crate::discovery_library::build_snapshot(&basis.library)?;
    let mut members: BTreeMap<String, DiscoveryIdentityGroupMember> = BTreeMap::new();
    let mut unbound_source_hits = 0;
    for row in library.rows {
        let (Some(id), Some(native)) = (&row.candidate_id, &row.native_author_id) else {
            unbound_source_hits += 1;
            continue;
        };
        let (name, url) = basis
            .native_details
            .get(id)
            .ok_or("identity_groups_native_binding_unavailable")?;
        let member = members
            .entry(id.clone())
            .or_insert_with(|| DiscoveryIdentityGroupMember {
                candidate_id: id.clone(),
                platform: row.author_platform().to_owned(),
                native_author_id: native.clone(),
                display_name: name.clone(),
                profile_url: url.clone(),
                verdict: row.verdict.clone(),
                reasons: row.reasons.clone(),
                sources: vec![],
            });
        if member.platform != row.author_platform()
            || member.native_author_id != *native
            || member.verdict != row.verdict
            || member.reasons != row.reasons
        {
            return Err("identity_groups_native_binding_conflict".into());
        }
        member.sources.push(row);
    }
    let decoded: Vec<_> = basis
        .reviews
        .iter()
        .map(|r| receipt(r, &library.campaign_id))
        .collect::<Result<_, _>>()?;
    let by_decision: BTreeMap<_, _> = decoded
        .iter()
        .map(|r| (r.decision_id.as_str(), r))
        .collect();
    let mut revoked = BTreeSet::new();
    for revocation in decoded
        .iter()
        .filter(|r| r.kind == DiscoveryIdentityReviewKind::Revoke)
    {
        let target = revocation
            .revokes_decision_id
            .as_deref()
            .ok_or("identity_groups_ledger_invalid")?;
        let original = by_decision
            .get(target)
            .ok_or("identity_groups_ledger_invalid")?;
        if original.kind == DiscoveryIdentityReviewKind::Revoke
            || original.operator_account_id != revocation.operator_account_id
            || original.candidate_ids != revocation.candidate_ids
            || original.native_author_ids != revocation.native_author_ids
            || original.platforms != revocation.platforms
            || original.material_hash != revocation.material_hash
            || !revoked.insert(target.to_owned())
        {
            return Err("identity_groups_ledger_invalid".into());
        }
    }
    drop(by_decision);
    let mut reviews = Vec::with_capacity(decoded.len());
    let mut edges = Vec::new();
    for (source, receipt) in basis.reviews.iter().zip(decoded) {
        let state = if revoked.contains(&receipt.decision_id) {
            DiscoveryIdentityGroupReviewState::Revoked
        } else if receipt.criteria_revision != library.criteria_revision {
            DiscoveryIdentityGroupReviewState::PreviousCriteria
        } else if matches!(
            receipt.kind,
            DiscoveryIdentityReviewKind::SameAuthor
                | DiscoveryIdentityReviewKind::DifferentAuthors
                | DiscoveryIdentityReviewKind::Insufficient
        ) {
            if current_review(source)?
                && receipt
                    .candidate_ids
                    .iter()
                    .all(|id| members.contains_key(id))
            {
                if receipt.kind != DiscoveryIdentityReviewKind::Insufficient {
                    edges.push((
                        receipt.candidate_ids[0].clone(),
                        receipt.candidate_ids[1].clone(),
                        receipt.kind.as_str().to_owned(),
                    ));
                }
                DiscoveryIdentityGroupReviewState::Current
            } else {
                DiscoveryIdentityGroupReviewState::EvidenceChanged
            }
        } else {
            DiscoveryIdentityGroupReviewState::Current
        };
        reviews.push(DiscoveryIdentityGroupReview { receipt, state });
    }
    let components = relationships::components(&edges);
    // A contradictory ledger must never produce a partial or contradictory grouping.
    if edges.iter().any(|(a, b, kind)| {
        kind == "different_authors"
            && components.get(a.as_str()).copied().unwrap_or(a)
                == components.get(b.as_str()).copied().unwrap_or(b)
    }) {
        return Err("identity_groups_relationship_conflict".into());
    }
    let native_accounts = members.len() as u32;
    let mut decisions_by_group: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for review in &reviews {
        if review.state == DiscoveryIdentityGroupReviewState::Current
            && review.receipt.kind == DiscoveryIdentityReviewKind::SameAuthor
        {
            let id = review.receipt.candidate_ids[0].as_str();
            let root = components.get(id).copied().unwrap_or(id);
            decisions_by_group
                .entry(root.to_owned())
                .or_default()
                .push(review.receipt.decision_id.clone());
        }
    }
    let qualified_native_accounts = members
        .values()
        .filter(|m| m.verdict == "qualified")
        .count() as u32;
    let mut grouped: BTreeMap<String, Vec<DiscoveryIdentityGroupMember>> = BTreeMap::new();
    for (id, member) in members {
        let root = components
            .get(id.as_str())
            .copied()
            .unwrap_or(&id)
            .to_owned();
        grouped.entry(root).or_default().push(member);
    }
    let mut groups = Vec::with_capacity(grouped.len());
    for (root, mut members) in grouped {
        members.sort_by(|a, b| {
            (&a.platform, &a.native_author_id).cmp(&(&b.platform, &b.native_author_id))
        });
        let native_keys: Vec<_> = members
            .iter()
            .map(|m| (&m.platform, &m.native_author_id))
            .collect();
        let group_id = crate::discovery_library::digest(&(1, native_keys))?;
        let decision_ids = decisions_by_group.remove(&root).unwrap_or_default();
        groups.push(DiscoveryIdentityGroup {
            group_id,
            has_same_author_review: members.len() > 1,
            qualified_member_count: members.iter().filter(|m| m.verdict == "qualified").count()
                as u32,
            members,
            decision_ids,
        });
    }
    groups.sort_by(|a, b| a.group_id.cmp(&b.group_id));
    let counts = DiscoveryIdentityGroupCounts {
        native_accounts,
        qualified_native_accounts,
        identity_groups: groups.len() as u32,
        qualified_identity_groups: groups
            .iter()
            .filter(|g| g.qualified_member_count > 0)
            .count() as u32,
        qualified_native_only_groups: groups
            .iter()
            .filter(|g| g.qualified_member_count > 0 && !g.has_same_author_review)
            .count() as u32,
        unbound_source_hits,
        stale_identity_reviews: reviews
            .iter()
            .filter(|r| r.state == DiscoveryIdentityGroupReviewState::EvidenceChanged)
            .count() as u32,
    };
    let snapshot_hash =
        crate::discovery_library::digest(&(1, &library.snapshot_hash, &groups, &reviews, &counts))?;
    Ok(DiscoveryIdentityGroupsSnapshot {
        campaign_id: library.campaign_id,
        criteria_revision: library.criteria_revision,
        snapshot_hash,
        groups,
        reviews,
        counts,
    })
}

impl CreatorRuntime {
    pub fn get_discovery_identity_groups(
        &self,
        campaign: &str,
        revision: u32,
    ) -> Result<DiscoveryIdentityGroupsSnapshot, String> {
        super::queries::validate_groups(campaign, revision)?;
        build(
            self.repository
                .read_discovery_identity_groups_basis(campaign, revision)?,
        )
    }

    pub async fn get_discovery_identity_groups_async(
        &self,
        campaign: String,
        revision: u32,
    ) -> Result<DiscoveryIdentityGroupsSnapshot, CreatorRepositoryCallError<String>> {
        super::queries::validate_groups(&campaign, revision)
            .map_err(CreatorRepositoryCallError::Owner)?;
        let repository = self.repository.clone();
        let basis = self
            .repository_read_segment(move || {
                repository.read_discovery_identity_groups_basis(&campaign, revision)
            })
            .await?;
        self.repository_cpu_segment(move || build(basis)).await
    }
}

#[cfg(test)]
mod tests;
