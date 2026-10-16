//! Authoritative, bounded Library source preparation. No provider reads or verdict writes.
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    CreatorRepositoryCallError, CreatorRuntime, DiscoveryPlatformScope, DiscoverySearchHit,
};

pub(crate) const MAX_LIBRARY_HITS: usize = 10_000;
pub(crate) const MAX_LIBRARY_SOURCE_BYTES: usize = 32 * 1024 * 1024;

/// Derived labels only; neither label changes the stored Campaign brief.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryLibraryTableName {
    pub name: String,
    pub previous_auto_name: String,
}

pub(crate) fn product_name(brief: &str) -> String {
    let first = brief
        .split(['，', ',', '。', ';', '；', '\n'])
        .next()
        .unwrap_or("")
        .trim();
    let chinese = [
        "我要推广",
        "我想推广",
        "我要卖",
        "我想卖",
        "我在卖",
        "我卖",
        "推广",
        "卖",
    ];
    let mut product = chinese
        .iter()
        .find_map(|prefix| first.strip_prefix(prefix))
        .unwrap_or(first)
        .trim();
    let lower = product.to_ascii_lowercase();
    for prefix in [
        "i want to sell ",
        "i am selling ",
        "i'm selling ",
        "i sell ",
        "selling ",
        "promote ",
    ] {
        if lower.starts_with(prefix) {
            product = product[prefix.len()..].trim();
            break;
        }
    }
    for marker in ["，", "想找", "希望找", "帮我找", "找达人", "寻找达人"] {
        if let Some(index) = product.find(marker).filter(|index| *index > 0) {
            product = product[..index].trim();
        }
    }
    product
        .chars()
        .filter(|character| !character.is_control())
        .take(120)
        .collect()
}

#[cfg(test)]
mod naming_tests {
    use super::product_name;

    #[test]
    fn names_contain_the_product_without_search_instructions() {
        for (brief, expected) in [
            (
                "我要卖键盘，找键盘测评或数码推荐达人。粉丝至少 8000，目标 2 名推荐作者",
                "键盘",
            ),
            ("手机支架\nContent language: 英文", "手机支架"),
            ("我卖蓝牙音响，粉丝10000以上", "蓝牙音响"),
            ("我要卖机械键盘 想找测评达人", "机械键盘"),
            (
                "I sell mechanical keyboards, find reviewers",
                "mechanical keyboards",
            ),
        ] {
            assert_eq!(product_name(brief), expected);
        }
    }
}

/// Repository-owned source basis; callers cannot construct or modify its facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryLibraryBasis {
    pub(crate) campaign_id: String,
    pub(crate) revision: u32,
    pub(crate) criteria_json: String,
    pub(crate) scopes_json: String,
    pub(crate) approved_at: i64,
    pub(crate) contact_required: bool,
    pub(crate) rows: Vec<LibrarySourceRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct LibrarySourceRow {
    pub hit: DiscoverySearchHit,
    pub candidate: Option<LibraryCandidateSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct LibraryCandidateSource {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub followers: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_platform: Option<String>,
    pub native_author_id: String,
    pub verdict: String,
    pub evidence_json: String,
    pub identity_receipt: String,
    pub manual_approved_at: Option<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub contacts: Vec<crate::DiscoveryContact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DiscoveryLibraryRow {
    pub hit: DiscoverySearchHit,
    /// Owner-bound count; a leading ~ preserves a rounded public estimate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub followers: Option<String>,
    /// Actual creator platform for an identity bound through a search-engine source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_platform: Option<String>,
    pub candidate_id: Option<String>,
    pub native_author_id: Option<String>,
    pub verdict: String,
    pub reasons: Vec<String>,
    pub evidence_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_rating: Option<DiscoveryLibraryMatchRating>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub contacts: Vec<crate::DiscoveryContact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize, JsonSchema)]
pub struct DiscoveryLibraryMatchRating {
    pub level: crate::DiscoveryMatchLevel,
    pub reason: String,
}

impl DiscoveryLibraryRow {
    pub fn author_platform(&self) -> &str {
        self.native_platform
            .as_deref()
            .unwrap_or(&self.hit.platform)
    }
}

/// Only the Creator owner produces this projection; there is no deserialize/input path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscoveryLibrarySnapshot {
    pub campaign_id: String,
    pub criteria_revision: u32,
    pub source_revision: u64,
    pub snapshot_hash: String,
    pub rows: Vec<DiscoveryLibraryRow>,
}

/// Read-only preparation from one repository transaction. This is deliberately
/// not a publishable/versioned DTO: source_revision is zero until the complete
/// source + identity basis is revalidated in the version-reservation transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryLibraryMaterial {
    pub sources: DiscoveryLibrarySnapshot,
    pub identity: crate::DiscoveryIdentityGroupsSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoveryLibraryVersions {
    pub source_revision: u64,
    pub identity_revision: u64,
}

/// Owner-produced publication. Both revisions were reserved after revalidating
/// the full source and review basis in one transaction. No provider calls occur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryLibraryPublication {
    pub material: DiscoveryLibraryMaterial,
    pub identity_revision: u64,
}

fn build_material(
    basis: crate::DiscoveryIdentityGroupsBasis,
) -> Result<DiscoveryLibraryMaterial, String> {
    let sources = build_snapshot(&basis.library)?;
    let identity = crate::discovery_identity_merge::groups::build(basis)?;
    Ok(DiscoveryLibraryMaterial { sources, identity })
}

pub(crate) fn digest(value: &impl Serialize) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(crate) fn build_snapshot(
    basis: &DiscoveryLibraryBasis,
) -> Result<DiscoveryLibrarySnapshot, String> {
    let criteria: Value = serde_json::from_str(&basis.criteria_json).map_err(|e| e.to_string())?;
    let scopes: Vec<DiscoveryPlatformScope> =
        serde_json::from_str(&basis.scopes_json).map_err(|e| e.to_string())?;
    let mut rows = Vec::with_capacity(basis.rows.len());
    for source in &basis.rows {
        if !scopes.iter().any(|scope| {
            scope.platform == source.hit.platform
                && scope.profile_ref == source.hit.profile_ref
                && scope.keywords.contains(&source.hit.search_keyword)
        }) {
            return Err("library_source_scope_mismatch".into());
        }
        let (candidate_id, native_author_id, verdict, reasons, evidence, receipt) =
            if let Some(candidate) = &source.candidate {
                let evidence: Value = serde_json::from_str(&candidate.evidence_json)
                    .map_err(|_| "library_source_evidence_invalid")?;
                let mut reasons: Vec<String> =
                    serde_json::from_value(evidence["assessment"]["reasons"].clone())
                        .map_err(|_| "library_source_assessment_invalid")?;
                if !matches!(
                    candidate.verdict.as_str(),
                    "qualified" | "pending" | "rejected"
                ) || reasons.len() > 50
                    || reasons
                        .iter()
                        .any(|reason| reason.is_empty() || reason.len() > 2000)
                    || (candidate.verdict == "qualified" && !reasons.is_empty())
                {
                    return Err("library_source_assessment_invalid".into());
                }
                let receipt: Value = serde_json::from_str(&candidate.identity_receipt)
                    .map_err(|_| "library_source_identity_invalid")?;
                let verdict = if reasons.iter().any(|reason| matches!(reason.as_str(),
                    "excluded_by_customer" | "excluded_account_identity_unverified"
                    | "excluded_accounts_invalid" | "requirement_unverified"
                    | "requirement_contradicted" | "requirements_invalid"
                )) {
                    candidate.verdict.clone()
                } else if basis.contact_required {
                    if crate::discovery_contacts::score_passes(
                        &candidate.verdict,
                        &evidence,
                        &candidate.native_author_id,
                    ) && !candidate.contacts.is_empty()
                    {
                        "qualified".into()
                    } else if candidate.verdict == "rejected" {
                        "rejected".into()
                    } else {
                        if candidate.verdict == "qualified" {
                            reasons.push("contact_unavailable".into());
                        }
                        "pending".into()
                    }
                } else if candidate.manual_approved_at.is_some() {
                    reasons.insert(0, "manual_approved".into());
                    "qualified".into()
                } else {
                    candidate.verdict.clone()
                };
                (
                    Some(candidate.id.clone()),
                    Some(candidate.native_author_id.clone()),
                    verdict,
                    reasons,
                    evidence,
                    receipt,
                )
            } else {
                (
                    None,
                    None,
                    "unverified".into(),
                    vec!["author_candidate_binding_unverified".into()],
                    Value::Null,
                    Value::Null,
                )
            };
        let evidence_basis = (
            1,
            &basis.campaign_id,
            basis.revision,
            &criteria,
            &source.hit,
            &candidate_id,
            &native_author_id,
            &verdict,
            &evidence,
            &receipt,
            source
                .candidate
                .as_ref()
                .and_then(|candidate| candidate.manual_approved_at),
        );
        let evidence_version = if basis.contact_required {
            digest(&(
                2,
                &evidence_basis,
                source.candidate.as_ref().map(|c| &c.contacts),
            ))?
        } else {
            digest(&evidence_basis)?
        };
        rows.push(DiscoveryLibraryRow {
            hit: source.hit.clone(),
            followers: source.candidate.as_ref().and_then(|c| c.followers.clone()),
            native_platform: source
                .candidate
                .as_ref()
                .and_then(|c| c.native_platform.clone()),
            candidate_id,
            native_author_id,
            verdict,
            reasons,
            evidence_version,
            match_rating: if source.candidate.as_ref().is_some_and(|candidate| {
                candidate.manual_approved_at.is_some() && candidate.verdict != "qualified"
            }) {
                None
            } else if evidence["assessment"]["match_rating"].is_object() {
                Some(
                    serde_json::from_value(evidence["assessment"]["match_rating"].clone())
                        .map_err(|_| "library_match_rating_invalid")?,
                )
            } else {
                None
            },
            contacts: source
                .candidate
                .as_ref()
                .map(|c| c.contacts.clone())
                .unwrap_or_default(),
        });
    }
    if criteria["contact_preference"] == "public_email_first" {
        rows.sort_by_key(|row| (
            row.verdict != "qualified",
            !row.contacts.iter().any(|contact| contact.kind == crate::DiscoveryContactKind::Email),
        ));
    }
    let snapshot_basis = (
        1,
        &basis.campaign_id,
        basis.revision,
        &criteria,
        &scopes,
        &rows,
    );
    let snapshot_hash = if basis.contact_required {
        digest(&(2, &snapshot_basis, "score_and_contact_v1"))?
    } else {
        digest(&snapshot_basis)?
    };
    Ok(DiscoveryLibrarySnapshot {
        campaign_id: basis.campaign_id.clone(),
        criteria_revision: basis.revision,
        source_revision: 0,
        snapshot_hash,
        rows,
    })
}

impl CreatorRuntime {
    pub fn prepare_discovery_library_publication(
        &self,
        campaign_id: &str,
        expected_revision: u32,
    ) -> Result<DiscoveryLibraryPublication, String> {
        let basis = self
            .repository
            .read_discovery_identity_groups_basis(campaign_id, expected_revision)?;
        let mut material = build_material(basis.clone())?;
        let versions = self.repository.reserve_discovery_library_publication(
            &basis,
            &material.sources.snapshot_hash,
            &material.identity.snapshot_hash,
        )?;
        material.sources.source_revision = versions.source_revision;
        Ok(DiscoveryLibraryPublication {
            material,
            identity_revision: versions.identity_revision,
        })
    }

    pub async fn prepare_discovery_library_publication_async(
        &self,
        campaign_id: String,
        expected_revision: u32,
    ) -> Result<DiscoveryLibraryPublication, CreatorRepositoryCallError<String>> {
        let repository = self.repository.clone();
        let basis = self
            .repository_read_segment(move || {
                repository.read_discovery_identity_groups_basis(&campaign_id, expected_revision)
            })
            .await?;
        let (basis, mut material) = self
            .repository_cpu_segment(move || {
                let material = build_material(basis.clone())?;
                Ok((basis, material))
            })
            .await?;
        let repository = self.repository.clone();
        let source_hash = material.sources.snapshot_hash.clone();
        let identity_hash = material.identity.snapshot_hash.clone();
        let versions = self
            .repository_write_segment(move || {
                repository.reserve_discovery_library_publication(
                    &basis,
                    &source_hash,
                    &identity_hash,
                )
            })
            .await?;
        material.sources.source_revision = versions.source_revision;
        Ok(DiscoveryLibraryPublication {
            material,
            identity_revision: versions.identity_revision,
        })
    }

    /// No provider call, budget reservation, publication or persistent version write.
    pub fn read_discovery_library_material(
        &self,
        campaign_id: &str,
        expected_revision: u32,
    ) -> Result<DiscoveryLibraryMaterial, String> {
        build_material(
            self.repository
                .read_discovery_identity_groups_basis(campaign_id, expected_revision)?,
        )
    }

    pub async fn read_discovery_library_material_async(
        &self,
        campaign_id: String,
        expected_revision: u32,
    ) -> Result<DiscoveryLibraryMaterial, CreatorRepositoryCallError<String>> {
        let repository = self.repository.clone();
        let basis = self
            .repository_read_segment(move || {
                repository.read_discovery_identity_groups_basis(&campaign_id, expected_revision)
            })
            .await?;
        self.repository_cpu_segment(move || build_material(basis))
            .await
    }

    /// Local version reservation only; does not search, consume budget or reassess candidates.
    pub fn prepare_discovery_library_snapshot(
        &self,
        campaign_id: &str,
        expected_revision: u32,
    ) -> Result<DiscoveryLibrarySnapshot, String> {
        let basis = self
            .repository
            .read_discovery_library_basis(campaign_id, expected_revision)?;
        let mut snapshot = build_snapshot(&basis)?;
        snapshot.source_revision = self
            .repository
            .reserve_discovery_library_version(&basis, &snapshot.snapshot_hash)?;
        Ok(snapshot)
    }

    pub async fn prepare_discovery_library_snapshot_async(
        &self,
        campaign_id: String,
        expected_revision: u32,
    ) -> Result<DiscoveryLibrarySnapshot, CreatorRepositoryCallError<String>> {
        let repository = self.repository.clone();
        let basis = self
            .repository_read_segment(move || {
                repository.read_discovery_library_basis(&campaign_id, expected_revision)
            })
            .await?;
        let (basis, mut snapshot) = self
            .repository_cpu_segment(move || {
                let snapshot = build_snapshot(&basis)?;
                Ok((basis, snapshot))
            })
            .await?;
        let repository = self.repository.clone();
        let hash = snapshot.snapshot_hash.clone();
        snapshot.source_revision = self
            .repository_write_segment(move || {
                repository.reserve_discovery_library_version(&basis, &hash)
            })
            .await?;
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests;
