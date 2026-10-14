//! CRM v35 -> v36 (app 0.6.0): retire the persisted Campaign plan model and
//! campaign-bound Creator provenance.
//!
//! Product decision (2026-09-10): plans are never persisted again. The five
//! Campaign tables are dropped outright and counted; every campaign-bound
//! search provenance record becomes a `platform_search` record owned by the
//! operator account that actually ran the search; community-administrator
//! provenance keeps its operator account and loses the Campaign reference;
//! the Campaign column of historical audit rows is cleared. A campaign search
//! origin whose operator account cannot be recovered from its discovery
//! snapshot (`discovery_id` + candidate account) is deleted and counted,
//! because the Creator row and its observation source remain. The idempotency
//! receipts of the retired `campaign_create` / `campaign_template_create`
//! actions are deleted and counted: nothing can replay them once the tables
//! they point into are gone.
//!
//! Persisted JSON that embeds the released v35 provenance wire shapes — the
//! candidate snapshot `origin_json` / `candidate_json.$.origin` and every
//! completed replay payload that carries a candidate origin or a Creator
//! record — is rewritten through the typed v35 -> v36 structs below inside the
//! same transaction. A payload that does not parse as the released v35 shape,
//! a snapshot whose two origin copies disagree, an origin with more than one
//! operator candidate, or a converted row that would collide with a v36
//! uniqueness key is a typed blocker; nothing is defaulted or skipped. The
//! orchestrator owns the transaction and disables foreign keys before it.

use std::collections::BTreeSet;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::CreatorDatabaseUpgradeImpact;

#[cfg(test)]
mod tests;

const V36_CREATOR_CANDIDATE_SNAPSHOTS_SQL: &str = r#"
             CREATE TABLE creator_candidate_snapshots (
               discovery_id TEXT NOT NULL CHECK(length(trim(discovery_id)) > 0),
               candidate_id TEXT NOT NULL CHECK(length(trim(candidate_id)) > 0),
               origin_kind TEXT NOT NULL CHECK(origin_kind IN ('platform_search','community_administrator')),
               origin_json TEXT NOT NULL CHECK(json_valid(origin_json) AND json_type(origin_json) = 'object'),
               candidate_json TEXT NOT NULL CHECK(json_valid(candidate_json) AND json_type(candidate_json) = 'object'),
               account_platform TEXT NOT NULL CHECK(account_platform IN ('x','instagram','telegram','youtube','tiktok')),
               account_target_kind TEXT NOT NULL CHECK(account_target_kind IN ('user','bot','channel','supergroup','group','creator','business')),
               account_external_id TEXT CHECK(
                 (account_external_id IS NOT NULL AND length(trim(account_external_id)) > 0)
                 OR (account_external_id IS NULL AND account_platform = 'tiktok'
                   AND account_handle IS NOT NULL AND length(trim(account_handle)) > 0)),
               account_handle TEXT CHECK(account_handle IS NULL OR length(trim(account_handle)) > 0),
               account_display_name TEXT NOT NULL CHECK(length(trim(account_display_name)) > 0),
               administrator_role TEXT CHECK(administrator_role IN ('owner','administrator')),
               community_external_id TEXT,
               community_title TEXT,
               community_handle TEXT,
               community_target_kind TEXT CHECK(community_target_kind IN ('channel','supergroup')),
               source_kind TEXT NOT NULL CHECK(source_kind IN ('platform_search','operator_community','creator_contact_route')),
               account_ref_id TEXT NOT NULL REFERENCES account_refs(id) ON DELETE RESTRICT,
               source_creator_id TEXT REFERENCES creators(id) ON DELETE RESTRICT,
               source_contact_route_snapshot_id TEXT
                 CHECK(source_contact_route_snapshot_id IS NULL
                   OR length(trim(source_contact_route_snapshot_id)) > 0),
               source_contact_route_id TEXT REFERENCES creator_contact_routes(id) ON DELETE SET NULL,
               source_resolved_target_id TEXT REFERENCES resolved_targets(id) ON DELETE RESTRICT,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               PRIMARY KEY(discovery_id, candidate_id),
               CHECK(json_type(candidate_json, '$.candidate_id') = 'text'
                 AND json_extract(candidate_json, '$.candidate_id') = candidate_id
                 AND json_type(candidate_json, '$.origin.discovery_id') = 'text'
                 AND json_extract(candidate_json, '$.origin.discovery_id') = discovery_id
                 AND json_type(candidate_json, '$.origin.kind') = 'text'
                 AND json_extract(candidate_json, '$.origin.kind') = origin_kind
                 AND json_type(candidate_json, '$.account.platform') = 'text'
                 AND json_extract(candidate_json, '$.account.platform') = account_platform
                 AND json_type(candidate_json, '$.account.target_kind') = 'text'
                 AND json_extract(candidate_json, '$.account.target_kind') = account_target_kind
                 AND json_type(candidate_json, '$.account.external_id') IS CASE WHEN account_external_id IS NULL THEN 'null' ELSE 'text' END
                 AND json_extract(candidate_json, '$.account.external_id') IS account_external_id),
               CHECK(
                 (origin_kind = 'platform_search'
                   AND source_kind = 'platform_search'
                   AND administrator_role IS NULL
                   AND community_external_id IS NULL
                   AND community_title IS NULL
                   AND community_handle IS NULL
                   AND community_target_kind IS NULL
                   AND source_creator_id IS NULL
                   AND source_contact_route_snapshot_id IS NULL
                   AND source_contact_route_id IS NULL
                   AND source_resolved_target_id IS NULL)
                 OR
                 (origin_kind = 'community_administrator'
                   AND administrator_role IS NOT NULL
                   AND community_external_id IS NOT NULL
                   AND length(trim(community_external_id)) > 0
                   AND community_title IS NOT NULL
                   AND length(trim(community_title)) > 0
                   AND (community_handle IS NULL OR length(trim(community_handle)) > 0)
                   AND community_target_kind IS NOT NULL
                   AND account_platform = 'telegram'
                   AND account_target_kind IN ('user','bot')
                   AND ((source_kind = 'operator_community'
                     AND source_creator_id IS NULL
                     AND source_contact_route_snapshot_id IS NULL
                     AND source_contact_route_id IS NULL
                     AND source_resolved_target_id IS NULL)
                   OR (source_kind = 'creator_contact_route'
                     AND source_creator_id IS NOT NULL
                     AND source_contact_route_snapshot_id IS NOT NULL
                     AND source_resolved_target_id IS NOT NULL))))
               ,CHECK(source_contact_route_id IS NULL
                 OR source_contact_route_id = source_contact_route_snapshot_id)
             );
"#;

const V36_CREATOR_ORIGINS_SQL: &str = r#"
             CREATE TABLE creator_origins (
               id TEXT PRIMARY KEY,
               kind TEXT NOT NULL CHECK(kind IN ('platform_search','community_administrator','manual_entry','creator_import')),
               creator_id TEXT NOT NULL REFERENCES creators(id) ON DELETE RESTRICT,
               source_id TEXT NOT NULL REFERENCES observation_sources(id) ON DELETE RESTRICT,
               discovery_id TEXT CHECK(discovery_id IS NULL OR length(trim(discovery_id)) > 0),
               source_creator_id TEXT REFERENCES creators(id) ON DELETE RESTRICT,
               source_contact_route_id TEXT REFERENCES creator_contact_routes(id) ON DELETE SET NULL,
               operator_account_ref_id TEXT REFERENCES account_refs(id) ON DELETE RESTRICT,
               source_resolved_target_id TEXT REFERENCES resolved_targets(id) ON DELETE RESTRICT,
               administrator_role TEXT CHECK(administrator_role IN ('owner','administrator')),
               community_external_id TEXT,
               community_title TEXT,
               community_handle TEXT,
               community_target_kind TEXT CHECK(community_target_kind IN ('channel','supergroup')),
               import_content_sha256 TEXT REFERENCES creator_import_receipts(content_sha256) ON DELETE RESTRICT,
               imported_creator_id TEXT CHECK(imported_creator_id IS NULL OR length(trim(imported_creator_id)) > 0),
               discovered_at INTEGER NOT NULL,
               CHECK(
                 (kind = 'platform_search'
                   AND discovery_id IS NOT NULL
                   AND operator_account_ref_id IS NOT NULL
                   AND source_creator_id IS NULL
                   AND source_contact_route_id IS NULL
                   AND source_resolved_target_id IS NULL
                   AND administrator_role IS NULL
                   AND community_external_id IS NULL
                   AND community_title IS NULL
                   AND community_handle IS NULL
                   AND community_target_kind IS NULL
                   AND import_content_sha256 IS NULL
                   AND imported_creator_id IS NULL)
                 OR
                 (kind = 'community_administrator'
                   AND operator_account_ref_id IS NOT NULL
                   AND administrator_role IS NOT NULL
                   AND community_external_id IS NOT NULL
                   AND length(trim(community_external_id)) > 0
                   AND community_title IS NOT NULL
                   AND length(trim(community_title)) > 0
                   AND (community_handle IS NULL OR length(trim(community_handle)) > 0)
                   AND community_target_kind IS NOT NULL
                   AND import_content_sha256 IS NULL
                   AND imported_creator_id IS NULL
                   AND ((source_creator_id IS NULL
                     AND source_contact_route_id IS NULL
                     AND source_resolved_target_id IS NULL)
                   OR (source_creator_id IS NOT NULL
                     AND source_resolved_target_id IS NOT NULL))
                   AND discovery_id IS NOT NULL)
                 OR
                 (kind = 'manual_entry'
                   AND discovery_id IS NULL
                   AND source_creator_id IS NULL
                   AND source_contact_route_id IS NULL
                   AND operator_account_ref_id IS NULL
                   AND source_resolved_target_id IS NULL
                   AND administrator_role IS NULL
                   AND community_external_id IS NULL
                   AND community_title IS NULL
                   AND community_handle IS NULL
                   AND community_target_kind IS NULL
                   AND import_content_sha256 IS NULL
                   AND imported_creator_id IS NULL)
                 OR
                 (kind = 'creator_import'
                   AND discovery_id IS NULL
                   AND source_creator_id IS NULL
                   AND source_contact_route_id IS NULL
                   AND operator_account_ref_id IS NULL
                   AND source_resolved_target_id IS NULL
                   AND administrator_role IS NULL
                   AND community_external_id IS NULL
                   AND community_title IS NULL
                   AND community_handle IS NULL
                   AND community_target_kind IS NULL
                   AND import_content_sha256 IS NOT NULL
                   AND imported_creator_id IS NOT NULL)
               )
             );
             CREATE INDEX creator_origins_by_creator_recency
               ON creator_origins(creator_id, discovered_at DESC, source_id);
             CREATE UNIQUE INDEX unique_community_administrator_origin
               ON creator_origins(
                 source_creator_id, source_contact_route_id, operator_account_ref_id,
                 source_resolved_target_id, discovery_id, creator_id
               )
               WHERE kind = 'community_administrator';
             CREATE UNIQUE INDEX unique_operator_community_administrator_origin
               ON creator_origins(
                 operator_account_ref_id, community_external_id, discovery_id, creator_id
               )
               WHERE kind = 'community_administrator' AND source_creator_id IS NULL;
             CREATE UNIQUE INDEX unique_manual_creator_origin
               ON creator_origins(creator_id) WHERE kind = 'manual_entry';
             CREATE UNIQUE INDEX unique_platform_search_origin
               ON creator_origins(operator_account_ref_id, discovery_id, creator_id)
               WHERE kind = 'platform_search';
             CREATE UNIQUE INDEX unique_creator_import_origin
               ON creator_origins(import_content_sha256, imported_creator_id)
               WHERE kind = 'creator_import';
"#;

const V36_AUDIT_EVENTS_SQL: &str = r#"
             CREATE TABLE audit_events (
               id TEXT PRIMARY KEY,
               kind TEXT NOT NULL,
               creator_id TEXT REFERENCES creators(id) ON DELETE RESTRICT,
               account_ref_id TEXT REFERENCES account_refs(id) ON DELETE RESTRICT,
               resolved_target_id TEXT REFERENCES resolved_targets(id) ON DELETE RESTRICT,
               mailbox_ref_id TEXT REFERENCES mailbox_refs(id) ON DELETE RESTRICT,
               contact_locator_id TEXT REFERENCES contact_locators(id) ON DELETE RESTRICT,
               contact_route_id TEXT REFERENCES creator_contact_routes(id) ON DELETE RESTRICT,
               email_id TEXT REFERENCES emails(id) ON DELETE RESTRICT,
               payload_json TEXT NOT NULL DEFAULT '{}',
               created_at INTEGER NOT NULL
             );
"#;

const RETIRED_DISCRIMINATORS: &[&str] = &[
    "\"campaign_discovery\"",
    "\"ad_hoc_search\"",
    "\"campaign_community\"",
];

/// The `idempotency_records.action` values the retired Campaign repository
/// wrote (its create receipts); these rows are deleted, never rewritten.
const RETIRED_CAMPAIGN_ACTIONS_SQL: &str = "('campaign_create','campaign_template_create')";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AdministratorRole {
    Owner,
    Administrator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CommunityTargetKind {
    Channel,
    Supergroup,
}

/// Community reference; unchanged between v35 and v36.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunityReference {
    external_id: String,
    title: String,
    handle: Option<String>,
    target_kind: CommunityTargetKind,
}

/// Released v35 `CreatorCandidateOrigin` wire shape.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum V35CandidateOrigin {
    CampaignDiscovery {
        discovery_id: String,
        campaign_id: String,
        account_ref_id: String,
        query: String,
        search_kind: String,
    },
    CommunityAdministrator {
        discovery_id: String,
        role: AdministratorRole,
        community: CommunityReference,
        source: V35CommunitySource,
    },
    AdHocSearch {
        discovery_id: String,
        account_ref_id: String,
        query: String,
        search_kind: String,
    },
}

/// Released v35 `CommunityAdministratorSource` wire shape.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum V35CommunitySource {
    CampaignCommunity {
        campaign_id: String,
        operator_account_ref_id: String,
    },
    CreatorContactRoute {
        source_creator_id: String,
        source_contact_route_id: Option<String>,
        operator_account_ref_id: String,
        source_resolved_target_id: String,
    },
}

/// The two released v35 `CreatorOrigin` search variants as embedded in replay
/// payloads. The other variants carry no Campaign reference in their own
/// fields and are left untouched (their community `source` is rewritten by
/// discriminator).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum V35CreatorSearchOrigin {
    CampaignDiscovery {
        id: String,
        discovery_id: String,
        campaign_id: String,
        creator_id: String,
        source_id: String,
        discovered_at: u64,
    },
    AdHocSearch {
        id: String,
        discovery_id: String,
        account_ref_id: String,
        creator_id: String,
        source_id: String,
        discovered_at: u64,
    },
}

/// Target v36 `CreatorCandidateOrigin` wire shape; field order equals the
/// owner model's so the stored `origin_json` matches what the repository
/// serializes (proved in `tests`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum V36CandidateOrigin {
    PlatformSearch {
        discovery_id: String,
        account_ref_id: String,
        query: String,
        search_kind: String,
    },
    CommunityAdministrator {
        discovery_id: String,
        role: AdministratorRole,
        community: CommunityReference,
        source: V36CommunitySource,
    },
}

/// Target v36 `CommunityAdministratorSource` wire shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum V36CommunitySource {
    OperatorAccount {
        operator_account_ref_id: String,
    },
    CreatorContactRoute {
        source_creator_id: String,
        source_contact_route_id: Option<String>,
        operator_account_ref_id: String,
        source_resolved_target_id: String,
    },
}

/// Target v36 `CreatorOrigin::PlatformSearch` wire shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum V36CreatorSearchOrigin {
    PlatformSearch {
        id: String,
        discovery_id: String,
        account_ref_id: String,
        creator_id: String,
        source_id: String,
        discovered_at: u64,
    },
}

impl From<V35CommunitySource> for V36CommunitySource {
    fn from(source: V35CommunitySource) -> Self {
        match source {
            V35CommunitySource::CampaignCommunity {
                operator_account_ref_id,
                ..
            } => Self::OperatorAccount {
                operator_account_ref_id,
            },
            V35CommunitySource::CreatorContactRoute {
                source_creator_id,
                source_contact_route_id,
                operator_account_ref_id,
                source_resolved_target_id,
            } => Self::CreatorContactRoute {
                source_creator_id,
                source_contact_route_id,
                operator_account_ref_id,
                source_resolved_target_id,
            },
        }
    }
}

impl From<V35CandidateOrigin> for V36CandidateOrigin {
    fn from(origin: V35CandidateOrigin) -> Self {
        match origin {
            V35CandidateOrigin::CampaignDiscovery {
                discovery_id,
                account_ref_id,
                query,
                search_kind,
                ..
            }
            | V35CandidateOrigin::AdHocSearch {
                discovery_id,
                account_ref_id,
                query,
                search_kind,
            } => Self::PlatformSearch {
                discovery_id,
                account_ref_id,
                query,
                search_kind,
            },
            V35CandidateOrigin::CommunityAdministrator {
                discovery_id,
                role,
                community,
                source,
            } => Self::CommunityAdministrator {
                discovery_id,
                role,
                community,
                source: source.into(),
            },
        }
    }
}

impl V36CandidateOrigin {
    /// The `(origin_kind, source_kind)` column pair this origin projects to.
    fn column_kinds(&self) -> (&'static str, &'static str) {
        match self {
            Self::PlatformSearch { .. } => ("platform_search", "platform_search"),
            Self::CommunityAdministrator {
                source: V36CommunitySource::OperatorAccount { .. },
                ..
            } => ("community_administrator", "operator_community"),
            Self::CommunityAdministrator {
                source: V36CommunitySource::CreatorContactRoute { .. },
                ..
            } => ("community_administrator", "creator_contact_route"),
        }
    }
}

/// Platform-account identity used to match a Creator to its discovery
/// snapshot: `(platform, target_kind, external_id)`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct AccountKey {
    platform: String,
    target_kind: String,
    external_id: String,
}

fn to_json_string<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

fn to_json_value<T: Serialize>(value: &T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

/// The operator account of the `campaign_discovery` snapshot that produced a
/// Creator with one of `accounts` in discovery `discovery_id`. `None` means
/// no snapshot attributes the search; more than one distinct operator is
/// ambiguous and blocks.
fn recover_operator(
    connection: &Connection,
    discovery_id: &str,
    accounts: &[AccountKey],
) -> Result<Option<String>, String> {
    let mut operators = BTreeSet::new();
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT account_ref_id FROM creator_candidate_snapshots
             WHERE discovery_id = ?1 AND origin_kind = 'campaign_discovery'
               AND account_platform = ?2 AND account_target_kind = ?3
               AND account_external_id = ?4",
        )
        .map_err(|error| error.to_string())?;
    for account in accounts {
        let rows = statement
            .query_map(
                params![
                    discovery_id,
                    account.platform,
                    account.target_kind,
                    account.external_id
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| error.to_string())?;
        for row in rows {
            operators.insert(row.map_err(|error| error.to_string())?);
        }
    }
    if operators.len() > 1 {
        return Err("a campaign search origin has more than one operator account candidate".into());
    }
    Ok(operators.into_iter().next())
}

fn creator_accounts(connection: &Connection, creator_id: &str) -> Result<Vec<AccountKey>, String> {
    let mut statement = connection
        .prepare(
            "SELECT account.platform, account.target_kind, account.external_id
             FROM creator_account_links link
             JOIN platform_accounts account ON account.id = link.platform_account_id
             WHERE link.creator_id = ?1
             ORDER BY account.id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([creator_id], |row| {
            Ok(AccountKey {
                platform: row.get(0)?,
                target_kind: row.get(1)?,
                external_id: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotRewrite {
    discovery_id: String,
    candidate_id: String,
    origin_kind: &'static str,
    source_kind: &'static str,
    origin_json: String,
    candidate_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OriginRewrite {
    id: String,
    kind: &'static str,
    /// A recovered operator account; `None` keeps the stored one.
    operator_account_ref_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CampaignRetirementImpact {
    removed_campaigns: u64,
    removed_campaign_templates: u64,
    removed_campaign_account_bindings: u64,
    removed_campaign_delete_operations: u64,
    removed_campaign_template_delete_operations: u64,
    converted_campaign_search_snapshots: u64,
    converted_campaign_search_origins: u64,
    removed_unattributable_campaign_origins: u64,
    converted_campaign_community_sources: u64,
    cleared_audit_campaign_refs: u64,
    removed_campaign_idempotency_records: u64,
}

/// Everything the edge will write, computed read-only from the source so the
/// preflight impact and the locked revalidation compare equal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Plan {
    impact: CampaignRetirementImpact,
    snapshots: Vec<SnapshotRewrite>,
    origins: Vec<OriginRewrite>,
    removed_origin_ids: Vec<String>,
    replay_rewrites: Vec<(String, String)>,
}

fn count(connection: &Connection, sql: &str) -> Result<u64, String> {
    connection
        .query_row(sql, [], |row| row.get::<_, u64>(0))
        .map_err(|error| error.to_string())
}

fn plan(connection: &Connection) -> Result<Plan, String> {
    let mut plan = Plan {
        impact: CampaignRetirementImpact {
            removed_campaigns: count(connection, "SELECT COUNT(*) FROM campaigns")?,
            removed_campaign_templates: count(
                connection,
                "SELECT COUNT(*) FROM campaign_templates",
            )?,
            removed_campaign_account_bindings: count(
                connection,
                "SELECT COUNT(*) FROM campaign_account_bindings",
            )?,
            removed_campaign_delete_operations: count(
                connection,
                "SELECT COUNT(*) FROM campaign_delete_operations",
            )?,
            removed_campaign_template_delete_operations: count(
                connection,
                "SELECT COUNT(*) FROM campaign_template_delete_operations",
            )?,
            cleared_audit_campaign_refs: count(
                connection,
                "SELECT COUNT(*) FROM audit_events WHERE campaign_id IS NOT NULL",
            )?,
            removed_campaign_idempotency_records: count(
                connection,
                &format!(
                    "SELECT COUNT(*) FROM idempotency_records
                     WHERE action IN {RETIRED_CAMPAIGN_ACTIONS_SQL}"
                ),
            )?,
            ..Default::default()
        },
        ..Default::default()
    };
    plan_snapshots(connection, &mut plan)?;
    plan_origins(connection, &mut plan)?;
    plan_replay_payloads(connection, &mut plan)?;
    Ok(plan)
}

fn plan_snapshots(connection: &Connection, plan: &mut Plan) -> Result<(), String> {
    let mut statement = connection
        .prepare(
            "SELECT discovery_id, candidate_id, origin_kind, source_kind, origin_json, candidate_json
             FROM creator_candidate_snapshots
             WHERE origin_kind IN ('campaign_discovery','ad_hoc_search')
                OR source_kind = 'campaign_community'
             ORDER BY discovery_id, candidate_id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (discovery_id, candidate_id, origin_kind, source_kind, origin_json, candidate_json) =
            row.map_err(|error| error.to_string())?;
        let origin: V35CandidateOrigin = serde_json::from_str(&origin_json).map_err(|_| {
            "a candidate snapshot origin does not match the released v35 shape".to_string()
        })?;
        let mut candidate: Map<String, Value> = serde_json::from_str(&candidate_json)
            .map_err(|_| "a candidate snapshot payload is not a JSON object".to_string())?;
        let embedded = candidate
            .get("origin")
            .cloned()
            .ok_or_else(|| "a candidate snapshot payload has no origin".to_string())?;
        let embedded: V35CandidateOrigin = serde_json::from_value(embedded).map_err(|_| {
            "a candidate snapshot payload origin does not match the released v35 shape".to_string()
        })?;
        if embedded != origin {
            return Err("a candidate snapshot origin disagrees with its payload origin".into());
        }
        let stored_kinds = match &origin {
            V35CandidateOrigin::CampaignDiscovery { .. } => {
                ("campaign_discovery", "campaign_discovery")
            }
            V35CandidateOrigin::AdHocSearch { .. } => ("ad_hoc_search", "ad_hoc_search"),
            V35CandidateOrigin::CommunityAdministrator {
                source: V35CommunitySource::CampaignCommunity { .. },
                ..
            } => ("community_administrator", "campaign_community"),
            V35CandidateOrigin::CommunityAdministrator {
                source: V35CommunitySource::CreatorContactRoute { .. },
                ..
            } => ("community_administrator", "creator_contact_route"),
        };
        if stored_kinds != (origin_kind.as_str(), source_kind.as_str()) {
            return Err("a candidate snapshot origin disagrees with its stored kinds".into());
        }
        match &origin {
            V35CandidateOrigin::CampaignDiscovery { .. } => {
                plan.impact.converted_campaign_search_snapshots += 1;
            }
            V35CandidateOrigin::CommunityAdministrator { .. } => {
                plan.impact.converted_campaign_community_sources += 1;
            }
            V35CandidateOrigin::AdHocSearch { .. } => {}
        }
        let converted = V36CandidateOrigin::from(origin);
        let (new_origin_kind, new_source_kind) = converted.column_kinds();
        candidate.insert("origin".into(), to_json_value(&converted)?);
        plan.snapshots.push(SnapshotRewrite {
            discovery_id,
            candidate_id,
            origin_kind: new_origin_kind,
            source_kind: new_source_kind,
            origin_json: to_json_string(&converted)?,
            candidate_json: Value::Object(candidate).to_string(),
        });
    }
    Ok(())
}

fn plan_origins(connection: &Connection, plan: &mut Plan) -> Result<(), String> {
    let mut statement = connection
        .prepare(
            "SELECT id, kind, creator_id, discovery_id, operator_account_ref_id, community_external_id
             FROM creator_origins
             WHERE kind IN ('campaign_discovery','ad_hoc_search')
                OR (kind = 'community_administrator' AND campaign_id IS NOT NULL)
             ORDER BY id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let mut search_keys = BTreeSet::new();
    let mut community_keys = BTreeSet::new();
    for (id, kind, creator_id, discovery_id, operator, community_external_id) in rows {
        let discovery_id =
            discovery_id.ok_or_else(|| "a search origin has no discovery id".to_string())?;
        match kind.as_str() {
            "ad_hoc_search" => {
                let operator = operator
                    .ok_or_else(|| "a search origin has no operator account".to_string())?;
                if !search_keys.insert((operator, discovery_id, creator_id)) {
                    return Err("converted platform search origins collide".into());
                }
                plan.origins.push(OriginRewrite {
                    id,
                    kind: "platform_search",
                    operator_account_ref_id: None,
                });
            }
            "campaign_discovery" => {
                let accounts = creator_accounts(connection, &creator_id)?;
                match recover_operator(connection, &discovery_id, &accounts)? {
                    Some(operator) => {
                        if !search_keys.insert((operator.clone(), discovery_id, creator_id)) {
                            return Err("converted platform search origins collide".into());
                        }
                        plan.impact.converted_campaign_search_origins += 1;
                        plan.origins.push(OriginRewrite {
                            id,
                            kind: "platform_search",
                            operator_account_ref_id: Some(operator),
                        });
                    }
                    None => {
                        plan.impact.removed_unattributable_campaign_origins += 1;
                        plan.removed_origin_ids.push(id);
                    }
                }
            }
            _ => {
                let operator = operator
                    .ok_or_else(|| "a community origin has no operator account".to_string())?;
                let community_external_id = community_external_id
                    .ok_or_else(|| "a community origin has no community id".to_string())?;
                if !community_keys.insert((
                    operator,
                    community_external_id,
                    discovery_id,
                    creator_id,
                )) {
                    return Err("converted community administrator origins collide".into());
                }
                plan.impact.converted_campaign_community_sources += 1;
                plan.origins.push(OriginRewrite {
                    id,
                    kind: "community_administrator",
                    operator_account_ref_id: None,
                });
            }
        }
    }
    Ok(())
}

fn plan_replay_payloads(connection: &Connection, plan: &mut Plan) -> Result<(), String> {
    // Receipts of the retired Campaign actions are deleted below, so they are
    // never planned for a rewrite.
    let mut statement = connection
        .prepare(&format!(
            "SELECT key, result_json FROM idempotency_records
             WHERE result_json IS NOT NULL AND action NOT IN {RETIRED_CAMPAIGN_ACTIONS_SQL}
             ORDER BY key"
        ))
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (key, raw) = row.map_err(|error| error.to_string())?;
        if !RETIRED_DISCRIMINATORS
            .iter()
            .any(|discriminator| raw.contains(discriminator))
        {
            continue;
        }
        let mut payload: Value = serde_json::from_str(&raw)
            .map_err(|_| "a replay payload carrying retired provenance is not JSON".to_string())?;
        if rewrite_replay_value(connection, &mut payload, &[])? {
            plan.replay_rewrites.push((key, payload.to_string()));
        }
    }
    Ok(())
}

enum Rewritten {
    Value(Value),
    Removed,
}

/// Rewrite one object that carries a retired discriminator; `None` when the
/// object is not a retired-kind object at all.
fn rewrite_retired_object(
    connection: &Connection,
    object: &Map<String, Value>,
    accounts: &[AccountKey],
) -> Result<Option<Rewritten>, String> {
    let Some(kind) = object.get("kind").and_then(Value::as_str) else {
        return Ok(None);
    };
    match kind {
        "campaign_discovery" | "ad_hoc_search" => {
            let value = Value::Object(object.clone());
            if let Ok(origin) = serde_json::from_value::<V35CreatorSearchOrigin>(value.clone()) {
                return Ok(Some(match origin {
                    V35CreatorSearchOrigin::AdHocSearch {
                        id,
                        discovery_id,
                        account_ref_id,
                        creator_id,
                        source_id,
                        discovered_at,
                    } => {
                        Rewritten::Value(to_json_value(&V36CreatorSearchOrigin::PlatformSearch {
                            id,
                            discovery_id,
                            account_ref_id,
                            creator_id,
                            source_id,
                            discovered_at,
                        })?)
                    }
                    V35CreatorSearchOrigin::CampaignDiscovery {
                        id,
                        discovery_id,
                        creator_id,
                        source_id,
                        discovered_at,
                        ..
                    } => match recover_operator(connection, &discovery_id, accounts)? {
                        Some(account_ref_id) => Rewritten::Value(to_json_value(
                            &V36CreatorSearchOrigin::PlatformSearch {
                                id,
                                discovery_id,
                                account_ref_id,
                                creator_id,
                                source_id,
                                discovered_at,
                            },
                        )?),
                        None => Rewritten::Removed,
                    },
                }));
            }
            let origin: V35CandidateOrigin = serde_json::from_value(value).map_err(|_| {
                "a replay payload search origin does not match the released v35 shape".to_string()
            })?;
            Ok(Some(Rewritten::Value(to_json_value(
                &V36CandidateOrigin::from(origin),
            )?)))
        }
        "campaign_community" => {
            let source: V35CommunitySource = serde_json::from_value(Value::Object(object.clone()))
                .map_err(|_| {
                    "a replay payload community source does not match the released v35 shape"
                        .to_string()
                })?;
            Ok(Some(Rewritten::Value(to_json_value(
                &V36CommunitySource::from(source),
            )?)))
        }
        _ => Ok(None),
    }
}

/// The platform accounts of an embedded Creator record, which attribute the
/// campaign search origins listed beside them.
fn embedded_accounts(record: &Map<String, Value>) -> Result<Vec<AccountKey>, String> {
    let Some(accounts) = record.get("platform_accounts") else {
        return Ok(Vec::new());
    };
    serde_json::from_value::<Vec<AccountKey>>(accounts.clone())
        .map_err(|_| "a replay payload Creator record has malformed platform accounts".to_string())
}

/// Walk one persisted payload, rewriting every retired provenance object in
/// place. Returns whether anything changed.
fn rewrite_replay_value(
    connection: &Connection,
    value: &mut Value,
    accounts: &[AccountKey],
) -> Result<bool, String> {
    let mut changed = false;
    match value {
        Value::Array(items) => {
            let mut rewritten = Vec::with_capacity(items.len());
            for mut item in std::mem::take(items) {
                if let Value::Object(object) = &item
                    && let Some(outcome) = rewrite_retired_object(connection, object, accounts)?
                {
                    changed = true;
                    if let Rewritten::Value(replacement) = outcome {
                        rewritten.push(replacement);
                    }
                    continue;
                }
                changed |= rewrite_replay_value(connection, &mut item, accounts)?;
                rewritten.push(item);
            }
            *items = rewritten;
        }
        Value::Object(object) => {
            if let Some(outcome) = rewrite_retired_object(connection, object, accounts)? {
                return match outcome {
                    Rewritten::Value(replacement) => {
                        *value = replacement;
                        Ok(true)
                    }
                    Rewritten::Removed => Err(
                        "a replay payload campaign search origin outside a list has no operator"
                            .into(),
                    ),
                };
            }
            let record_accounts = if object.contains_key("origins") {
                embedded_accounts(object)?
            } else {
                Vec::new()
            };
            let accounts = if record_accounts.is_empty() {
                accounts
            } else {
                record_accounts.as_slice()
            };
            for (_, child) in object.iter_mut() {
                changed |= rewrite_replay_value(connection, child, accounts)?;
            }
        }
        _ => {}
    }
    Ok(changed)
}

/// Read-only preflight: validate every payload this edge rewrites and report
/// the typed Campaign retirement counts.
pub(super) fn preflight(
    connection: &Connection,
    impact: &mut CreatorDatabaseUpgradeImpact,
) -> Result<(), String> {
    let plan = plan(connection)?;
    impact.removed_campaigns = plan.impact.removed_campaigns;
    impact.removed_campaign_templates = plan.impact.removed_campaign_templates;
    impact.removed_campaign_account_bindings = plan.impact.removed_campaign_account_bindings;
    impact.removed_campaign_delete_operations = plan.impact.removed_campaign_delete_operations;
    impact.removed_campaign_template_delete_operations =
        plan.impact.removed_campaign_template_delete_operations;
    impact.converted_campaign_search_snapshots = plan.impact.converted_campaign_search_snapshots;
    impact.converted_campaign_search_origins = plan.impact.converted_campaign_search_origins;
    impact.removed_unattributable_campaign_origins =
        plan.impact.removed_unattributable_campaign_origins;
    impact.converted_campaign_community_sources = plan.impact.converted_campaign_community_sources;
    impact.cleared_audit_campaign_refs = plan.impact.cleared_audit_campaign_refs;
    impact.removed_campaign_idempotency_records = plan.impact.removed_campaign_idempotency_records;
    Ok(())
}

fn expect_rows(changed: usize, expected: u64, what: &str) -> Result<(), String> {
    if u64::try_from(changed).is_ok_and(|changed| changed == expected) {
        Ok(())
    } else {
        Err(format!(
            "{what} changed {changed} rows instead of {expected}"
        ))
    }
}

fn expect_one_row(changed: usize, what: &str) -> Result<(), String> {
    expect_rows(changed, 1, what)
}

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    let plan = plan(connection)?;
    connection
        .execute_batch(
            "CREATE TABLE creator_candidate_snapshots_v36_copy AS
               SELECT * FROM creator_candidate_snapshots;
             CREATE TABLE creator_origins_v36_copy AS SELECT * FROM creator_origins;",
        )
        .map_err(|error| error.to_string())?;
    for snapshot in &plan.snapshots {
        let changed = connection
            .execute(
                "UPDATE creator_candidate_snapshots_v36_copy
                 SET origin_kind = ?3, source_kind = ?4, campaign_id = NULL,
                     origin_json = ?5, candidate_json = ?6
                 WHERE discovery_id = ?1 AND candidate_id = ?2",
                params![
                    snapshot.discovery_id,
                    snapshot.candidate_id,
                    snapshot.origin_kind,
                    snapshot.source_kind,
                    snapshot.origin_json,
                    snapshot.candidate_json,
                ],
            )
            .map_err(|error| error.to_string())?;
        expect_one_row(changed, "candidate snapshot rewrite")?;
    }
    for origin in &plan.origins {
        let changed = connection
            .execute(
                "UPDATE creator_origins_v36_copy
                 SET kind = ?2, campaign_id = NULL,
                     operator_account_ref_id = COALESCE(?3, operator_account_ref_id)
                 WHERE id = ?1",
                params![origin.id, origin.kind, origin.operator_account_ref_id],
            )
            .map_err(|error| error.to_string())?;
        expect_one_row(changed, "creator origin rewrite")?;
    }
    for id in &plan.removed_origin_ids {
        let changed = connection
            .execute("DELETE FROM creator_origins_v36_copy WHERE id = ?1", [id])
            .map_err(|error| error.to_string())?;
        expect_one_row(changed, "unattributable campaign origin removal")?;
    }
    connection
        .execute_batch(&format!(
            "DROP TABLE creator_candidate_snapshots;
             {V36_CREATOR_CANDIDATE_SNAPSHOTS_SQL}
             INSERT INTO creator_candidate_snapshots
               (discovery_id, candidate_id, origin_kind, origin_json, candidate_json,
                account_platform, account_target_kind, account_external_id, account_handle,
                account_display_name, administrator_role, community_external_id, community_title,
                community_handle, community_target_kind, source_kind, account_ref_id,
                source_creator_id, source_contact_route_snapshot_id, source_contact_route_id,
                source_resolved_target_id, created_at, updated_at)
             SELECT discovery_id, candidate_id, origin_kind, origin_json, candidate_json,
                    account_platform, account_target_kind, account_external_id, account_handle,
                    account_display_name, administrator_role, community_external_id, community_title,
                    community_handle, community_target_kind, source_kind, account_ref_id,
                    source_creator_id, source_contact_route_snapshot_id, source_contact_route_id,
                    source_resolved_target_id, created_at, updated_at
             FROM creator_candidate_snapshots_v36_copy;
             DROP TABLE creator_candidate_snapshots_v36_copy;
             DROP TABLE creator_origins;
             {V36_CREATOR_ORIGINS_SQL}
             INSERT INTO creator_origins
               (id, kind, creator_id, source_id, discovery_id, source_creator_id,
                source_contact_route_id, operator_account_ref_id, source_resolved_target_id,
                administrator_role, community_external_id, community_title, community_handle,
                community_target_kind, import_content_sha256, imported_creator_id, discovered_at)
             SELECT id, kind, creator_id, source_id, discovery_id, source_creator_id,
                    source_contact_route_id, operator_account_ref_id, source_resolved_target_id,
                    administrator_role, community_external_id, community_title, community_handle,
                    community_target_kind, import_content_sha256, imported_creator_id, discovered_at
             FROM creator_origins_v36_copy;
             DROP TABLE creator_origins_v36_copy;
             CREATE TABLE audit_events_v36_copy AS SELECT * FROM audit_events;
             DROP TABLE audit_events;
             {V36_AUDIT_EVENTS_SQL}
             INSERT INTO audit_events
               (id, kind, creator_id, account_ref_id, resolved_target_id, mailbox_ref_id,
                contact_locator_id, contact_route_id, email_id, payload_json, created_at)
             SELECT id, kind, creator_id, account_ref_id, resolved_target_id, mailbox_ref_id,
                    contact_locator_id, contact_route_id, email_id, payload_json, created_at
             FROM audit_events_v36_copy;
             DROP TABLE audit_events_v36_copy;
             DROP TABLE campaign_account_bindings;
             DROP TABLE campaign_delete_operations;
             DROP TABLE campaign_template_delete_operations;
             DROP TABLE campaign_templates;
             DROP TABLE campaigns;"
        ))
        .map_err(|error| error.to_string())?;
    let removed = connection
        .execute(
            &format!(
                "DELETE FROM idempotency_records WHERE action IN {RETIRED_CAMPAIGN_ACTIONS_SQL}"
            ),
            [],
        )
        .map_err(|error| error.to_string())?;
    expect_rows(
        removed,
        plan.impact.removed_campaign_idempotency_records,
        "retired campaign receipt removal",
    )?;
    for (key, payload) in &plan.replay_rewrites {
        let changed = connection
            .execute(
                "UPDATE idempotency_records SET result_json = ?2 WHERE key = ?1",
                params![key, payload],
            )
            .map_err(|error| error.to_string())?;
        expect_one_row(changed, "replay payload rewrite")?;
    }
    Ok(())
}
