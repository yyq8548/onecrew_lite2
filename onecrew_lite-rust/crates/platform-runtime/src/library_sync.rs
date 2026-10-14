//! An explicit local handoff from a persisted discovery Campaign to the old
//! Teable Library. The Library API remains the only writer of its PGlite data.
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
};

use onecrew_creator_runtime::{DiscoveryLibraryRow, DiscoveryLibrarySnapshot, DiscoverySearchHit};
use onecrew_local_vault::VaultCreatorSyncCredentialState;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::LocalPlatform;

const MAX_ROWS: usize = 10_000;
pub const REQUIRED_LIBRARY_CAPABILITIES: [&str; 2] =
    ["discovery-contacts-v2", "recommendation-capture-v1"];
mod client;
mod identity;
use client::{LibraryClient, ProjectionRow, discover_library};
const COLUMNS: [(&str, &str); 14] = [
    ("Campaign ID", "string"),
    ("Search hit ID", "string"),
    ("Platform", "string"),
    ("Author label", "string"),
    ("Followers", "string"),
    ("Author ID", "string"),
    ("Author URL", "website"),
    ("Content title", "string"),
    ("Source URL", "website"),
    ("Search keyword", "string"),
    ("Status", "string"),
    ("Captured at (Unix s)", "string"),
    ("Match evaluation", "score"),
    ("Content category", "string"),
];
static SYNC_FLIGHTS: LazyLock<Mutex<HashMap<SyncKey, Arc<SyncFlight>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Hash, PartialEq, Eq)]
struct SyncKey {
    data_dir: PathBuf,
    campaign_id: String,
    account_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveryLibrarySyncResult {
    pub campaign_id: String,
    pub table_id: String,
    pub web_url: String,
    pub total_hits: usize,
    pub inserted_rows: usize,
}

type SyncOutcome = Result<DiscoveryLibrarySyncResult, String>;

struct SyncFlight {
    outcome: tokio::sync::watch::Sender<Option<SyncOutcome>>,
}

struct SyncFlightOwner {
    key: SyncKey,
    flight: Arc<SyncFlight>,
    finished: bool,
}

impl SyncFlightOwner {
    fn finish(mut self, result: &SyncOutcome) {
        self.flight.outcome.send_replace(Some(result.clone()));
        self.finished = true;
        self.remove();
    }

    fn remove(&self) {
        let mut flights = SYNC_FLIGHTS
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if flights
            .get(&self.key)
            .is_some_and(|current| Arc::ptr_eq(current, &self.flight))
        {
            flights.remove(&self.key);
        }
    }
}

impl Drop for SyncFlightOwner {
    fn drop(&mut self) {
        if !self.finished {
            self.flight
                .outcome
                .send_replace(Some(Err("library_sync_interrupted".into())));
            self.remove();
        }
    }
}

async fn join_sync(flight: Arc<SyncFlight>) -> SyncOutcome {
    let mut receiver = flight.outcome.subscribe();
    loop {
        if let Some(result) = receiver.borrow_and_update().clone() {
            return result;
        }
        receiver
            .changed()
            .await
            .map_err(|_| "library_sync_interrupted")?;
    }
}

#[derive(Debug, Deserialize)]
struct LibraryTable {
    id: String,
    #[serde(default)]
    name: String,
    columns: Vec<LibraryColumn>,
}
#[derive(Debug, Deserialize)]
struct LibraryColumn {
    id: String,
    name: String,
    data_type: String,
}
fn column_ids(table: &LibraryTable) -> Result<HashMap<&'static str, String>, String> {
    let mut result = HashMap::new();
    for (name, expected_type) in COLUMNS {
        let matches: Vec<_> = table
            .columns
            .iter()
            .filter(|field| field.name == name)
            .collect();
        if matches.len() != 1 || matches[0].data_type != expected_type {
            return Err("library_table_schema_changed".into());
        }
        result.insert(name, matches[0].id.clone());
    }
    Ok(result)
}

fn row_values(hit: &DiscoverySearchHit, ids: &HashMap<&str, String>) -> Value {
    let fields = [
        ("Campaign ID", json!(hit.campaign_id)),
        ("Search hit ID", json!(hit.id)),
        ("Platform", json!(hit.platform)),
        ("Author label", json!(hit.author_label)),
        ("Followers", Value::Null),
        ("Author ID", json!(hit.author_id)),
        ("Author URL", json!(hit.author_url)),
        ("Content title", json!(hit.title)),
        ("Source URL", json!(hit.source_url)),
        ("Search keyword", json!(hit.search_keyword)),
        ("Status", json!("Unverified search lead")),
        ("Captured at (Unix s)", json!(hit.captured_at.to_string())),
        ("Match evaluation", Value::Null),
        ("Content category", Value::Null),
    ];
    Value::Object(
        fields
            .into_iter()
            .map(|(name, value)| (ids[name].clone(), value))
            .collect(),
    )
}

fn uncertain_write(error: &str) -> bool {
    matches!(
        error,
        "library_request_unavailable" | "library_write_failed" | "library_invalid_response"
    )
}
fn matches_projection(
    source: &DiscoveryLibraryRow,
    saved: &ProjectionRow,
    snapshot: &DiscoveryLibrarySnapshot,
) -> bool {
    saved.search_hit_id == source.hit.id
        && saved.criteria_revision == snapshot.criteria_revision
        && saved.source_revision == snapshot.source_revision
        && saved.candidate_id == source.candidate_id
        && saved.native_author_id == source.native_author_id
        && saved.verdict == source.verdict
        && saved.reasons == source.reasons
        && saved.evidence_version == source.evidence_version
        && saved.contacts == source.contacts
}
fn require_same_rows(
    before: &HashMap<String, ProjectionRow>,
    after: &HashMap<String, ProjectionRow>,
) -> Result<(), String> {
    if before
        .iter()
        .any(|(hit, row)| after.get(hit).is_none_or(|new| new.row_id != row.row_id))
    {
        Err("library_row_identity_changed".into())
    } else {
        Ok(())
    }
}

impl LocalPlatform {
    /// Read the existing account-bound Library handshake without creating a table.
    pub async fn discovery_library_readiness(&self) -> Result<(), String> {
        let user = self.library_access_user().await?;
        discover_library(&user).await?;
        if self.library_access_user().await? != user {
            return Err("library_account_changed".into());
        }
        Ok(())
    }
    /// Reconcile durable Campaign hits into one old-Library table. Retrying
    /// after an uncertain HTTP result first reads the table and existing IDs.
    pub async fn sync_discovery_library(
        &self,
        campaign_id: String,
    ) -> Result<DiscoveryLibrarySyncResult, String> {
        let expected_user = self.library_access_user().await?;
        let key = SyncKey {
            data_dir: self.data_dir.clone(),
            campaign_id: campaign_id.clone(),
            account_id: expected_user.clone(),
        };
        let (flight, is_owner) = {
            let mut flights = SYNC_FLIGHTS
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if let Some(flight) = flights.get(&key) {
                (flight.clone(), false)
            } else {
                let (outcome, _) = tokio::sync::watch::channel(None);
                let flight = Arc::new(SyncFlight { outcome });
                flights.insert(key.clone(), flight.clone());
                (flight, true)
            }
        };
        if !is_owner {
            return join_sync(flight).await;
        }
        let owner = SyncFlightOwner {
            key,
            flight,
            finished: false,
        };
        let result = reconcile_stale(|| {
            self.sync_discovery_library_inner(campaign_id.clone(), expected_user.clone())
        })
        .await;
        owner.finish(&result);
        result
    }

    /// Read the same immutable recommendation projection used by XLSX; never resync customer data here.
    pub async fn capture_discovery_recommendations(
        &self,
        campaign_id: String,
        expected_user: String,
        metadata: Value,
    ) -> Result<Value, String> {
        let account = self.library_access_user().await?;
        if account != expected_user {
            return Err("table_sync_account_changed".into());
        }
        let creators = self.creators()?;
        let campaign = creators
            .get_discovery_campaign_async(campaign_id.clone())
            .await
            .map_err(|_| "discovery_campaign_unavailable")?;
        let library = discover_library(&account).await?;
        let table = library
            .campaign_table(&campaign_id)
            .await?
            .ok_or("table_sync_local_table_missing")?;
        let capture: Value = library
            .request(
                Method::POST,
                &format!("/api/v1/dynamic-tables/{}/recommendation-capture", table.id),
                Some(metadata),
            )
            .await?;
        if capture["table_id"] != table.id
            || capture["criteria_revision"].as_u64() != Some(campaign.revision as u64)
            || self.library_access_user().await? != account
        {
            return Err("table_sync_source_changed".into());
        }
        Ok(capture)
    }

    pub(crate) async fn library_access_user(&self) -> Result<String, String> {
        let vault = self.vault.clone();
        tokio::task::spawn_blocking(move || {
            match vault
                .get_creator_sync_credential()
                .map_err(|_| "library_identity_unavailable".to_owned())?
            {
                Some(VaultCreatorSyncCredentialState::Active { credential, .. }) => {
                    Ok(credential.metadata.access_user_id)
                }
                _ => Err("library_identity_unavailable".into()),
            }
        })
        .await
        .map_err(|_| "library_identity_unavailable".to_owned())?
    }

    async fn sync_discovery_library_inner(
        &self,
        campaign_id: String,
        expected_user: String,
    ) -> Result<DiscoveryLibrarySyncResult, String> {
        let creators = self.creators()?;
        let campaign = creators
            .get_discovery_campaign_async(campaign_id.clone())
            .await
            .map_err(|_| "discovery_campaign_unavailable")?;
        let publication = creators
            .prepare_discovery_library_publication_async(campaign.id.clone(), campaign.revision)
            .await
            .map_err(|_| "discovery_snapshot_unavailable")?;
        let library = discover_library(&expected_user).await?;
        if self.library_access_user().await? != expected_user {
            return Err("library_account_changed".into());
        }
        let result = self
            .sync_library_snapshot(
                &library,
                &publication.material.sources,
                &expected_user,
                campaign.criteria["category"].as_str().unwrap_or_default(),
                &creators
                    .discovery_library_table_name_async(campaign.id.clone())
                    .await
                    .map_err(|_| "discovery_table_name_unavailable")?,
            )
            .await?;
        // Provider/Library I/O happens outside the Creator transaction. Recheck
        // before and after publication so changed reviews cannot report success.
        for after_publish in [false, true] {
            if self.library_access_user().await? != expected_user {
                return Err("library_account_changed".into());
            }
            let current = creators
                .read_discovery_library_material_async(campaign.id.clone(), campaign.revision)
                .await
                .map_err(|_| "discovery_snapshot_unavailable")?;
            if current.sources.snapshot_hash != publication.material.sources.snapshot_hash
                || current.identity.snapshot_hash != publication.material.identity.snapshot_hash
            {
                return Err("library_source_stale".into());
            }
            if !after_publish {
                library
                    .publish_identity(&result.table_id, &publication)
                    .await?;
            }
        }
        Ok(result)
    }

    async fn sync_library_snapshot(
        &self,
        library: &LibraryClient,
        snapshot: &DiscoveryLibrarySnapshot,
        expected_user: &str,
        category: &str,
        table_name: &onecrew_creator_runtime::DiscoveryLibraryTableName,
    ) -> Result<DiscoveryLibrarySyncResult, String> {
        if snapshot.rows.is_empty() {
            return Err("library_no_search_hits".into());
        }
        let expected: HashSet<_> = snapshot
            .rows
            .iter()
            .map(|row| row.hit.id.as_str())
            .collect();
        if expected.len() != snapshot.rows.len() || snapshot.rows.len() > MAX_ROWS {
            return Err("library_capacity_exceeded".into());
        }
        let existing = library.campaign_table(&snapshot.campaign_id).await?;
        let category_added = existing.as_ref().is_some_and(|table| {
            !table
                .columns
                .iter()
                .any(|column| column.name == "Content category")
        });
        let followers_added = existing.as_ref().is_some_and(|table| {
            !table
                .columns
                .iter()
                .any(|column| column.name == "Followers")
        });
        let table = match existing {
            Some(table) if !table.columns.iter().any(|column| column.name == "Match evaluation")
                || category_added || followers_added
                || table.name == format!("OneCrew Sources {}", snapshot.campaign_id)
                || table.name == format!("OneCrew Campaign {}", snapshot.campaign_id)
                || (table.name == table_name.previous_auto_name && table.name != table_name.name) => {
                library.request(Method::POST, "/_local/discovery/campaign", Some(json!({
                    "campaign_id": snapshot.campaign_id, "name": table_name.name,
                    "previous_auto_name": table_name.previous_auto_name,
                    "columns": COLUMNS.iter().map(|(name,data_type)| json!({"name":name,"data_type":data_type})).collect::<Vec<_>>()
                }))).await?
            }
            Some(table) => table,
            None => {
                let result = library.request(Method::POST, "/_local/discovery/campaign", Some(json!({
                    "campaign_id": snapshot.campaign_id,
                    "name": table_name.name,
                    "columns": COLUMNS.iter().map(|(name,data_type)| json!({"name":name,"data_type":data_type})).collect::<Vec<_>>()
                }))).await;
                match result {
                    Ok(table) => table,
                    Err(error) if uncertain_write(&error) => {
                        match library.campaign_table(&snapshot.campaign_id).await {
                            Ok(Some(table)) => table,
                            _ => return Err(error),
                        }
                    }
                    Err(error) => return Err(error),
                }
            }
        };
        let ids = column_ids(&table)?;
        let known = library.state(&table.id, &snapshot.campaign_id).await?;
        if known.keys().any(|id| !expected.contains(id.as_str())) {
            return Err("library_foreign_source_row".into());
        }
        for batch in snapshot.rows.chunks(100) {
            if self.library_access_user().await? != expected_user {
                return Err("library_account_changed".into());
            }
            if !category_added
                && !followers_added
                && batch.iter().all(|row| {
                    known
                        .get(&row.hit.id)
                        .is_some_and(|saved| matches_projection(row, saved, snapshot))
                })
            {
                continue;
            }
            let rows: Vec<_> = batch
                .iter()
                .map(|row| {
                    let mut values = row_values(&row.hit, &ids);
                    values[&ids["Platform"]] = json!(row.author_platform());
                    values[&ids["Content category"]] = json!(category);
                    values[&ids["Followers"]] = json!(row.followers);
                    values[&ids["Match evaluation"]] = if row.verdict == "qualified"
                        || (row.verdict == "rejected"
                            && row.match_rating.as_ref().is_some_and(|r| {
                                r.level == onecrew_creator_runtime::DiscoveryMatchLevel::Low
                            })) {
                        json!(row.match_rating)
                    } else {
                        Value::Null
                    };
                    json!({
                        "search_hit_id": row.hit.id, "candidate_id": row.candidate_id,
                        "native_author_id": row.native_author_id, "verdict": row.verdict,
                        "reasons": row.reasons, "evidence_version": row.evidence_version,
                        "contacts": row.contacts,
                        "values": values
                    })
                })
                .collect();
            let result = library.request::<Value>(Method::POST, "/_local/discovery/ingest",Some(json!({
                "table_id": table.id, "campaign_id": snapshot.campaign_id,
                "criteria_revision": snapshot.criteria_revision, "source_revision": snapshot.source_revision,
                "rows": rows
            }))).await;
            if let Err(error) = result {
                if !uncertain_write(&error) {
                    return Err(error);
                }
                // Reconcile once. Never resend an uncertain write blindly.
                let recovered = library.state(&table.id, &snapshot.campaign_id).await?;
                require_same_rows(&known, &recovered)?;
                if !batch.iter().all(|row| {
                    recovered
                        .get(&row.hit.id)
                        .is_some_and(|saved| matches_projection(row, saved, snapshot))
                }) {
                    return Err(error);
                }
            }
        }
        if self.library_access_user().await? != expected_user {
            return Err("library_account_changed".into());
        }
        let final_rows = library.state(&table.id, &snapshot.campaign_id).await?;
        require_same_rows(&known, &final_rows)?;
        if final_rows.len() != snapshot.rows.len()
            || !snapshot.rows.iter().all(|row| {
                final_rows
                    .get(&row.hit.id)
                    .is_some_and(|saved| matches_projection(row, saved, snapshot))
            })
        {
            return Err("library_readback_incomplete".into());
        }
        Ok(DiscoveryLibrarySyncResult {
            campaign_id: snapshot.campaign_id.clone(),
            web_url: format!("{}/library/{}", library.base, table.id),
            table_id: table.id,
            total_hits: snapshot.rows.len(),
            inserted_rows: final_rows.len() - known.len(),
        })
    }
}

// A changed source snapshot is a definite conflict. Re-prepare and read back
// existing row IDs before another projection. Other outcomes are never repeated.
async fn reconcile_stale<F, Fut>(mut reconcile: F) -> SyncOutcome
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = SyncOutcome>,
{
    for attempt in 0..3 {
        let result = reconcile().await;
        if result
            .as_ref()
            .err()
            .is_none_or(|code| code != "library_source_stale")
            || attempt == 2
        {
            return result;
        }
        tokio::task::yield_now().await;
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn stale_projection_reconciles_but_uncertain_writes_are_not_repeated() {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let result = reconcile_stale(|| {
            let count = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async move {
                Err(if count < 2 {
                    "library_source_stale"
                } else {
                    "library_write_failed"
                }
                .into())
            }
        })
        .await;
        assert_eq!(result.unwrap_err(), "library_write_failed");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);
        calls.store(0, std::sync::atomic::Ordering::SeqCst);
        let result = reconcile_stale(|| {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async { Err("library_write_failed".into()) }
        })
        .await;
        assert_eq!(result.unwrap_err(), "library_write_failed");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn trusted_readback_requires_exact_qualification_and_stable_row_identity() {
        let hit = DiscoverySearchHit {
            id: "hit".into(),
            campaign_id: "campaign".into(),
            platform: "twitter".into(),
            profile_ref: "fixture-profile".into(),
            search_keyword: "AI tools".into(),
            source_url: "https://x.com/fixture/status/1".into(),
            title: "Demo".into(),
            author_label: "Fixture".into(),
            captured_at: 1,
            author_id: None,
            author_url: None,
            author_observed_at: None,
        };
        let row = DiscoveryLibraryRow {
            followers: None,
            native_platform: None,
            hit,
            candidate_id: Some("candidate".into()),
            native_author_id: Some("native".into()),
            verdict: "pending".into(),
            reasons: vec!["missing_demo".into()],
            evidence_version: "a".repeat(64),
            match_rating: None,
            contacts: vec![onecrew_creator_runtime::DiscoveryContact {
                kind: onecrew_creator_runtime::DiscoveryContactKind::Email,
                original_value: "studio@example.org".into(),
                normalized_value: "studio@example.org".into(),
                source_url: "https://x.com/fixture/status/1".into(),
                source_excerpt: "Business: studio@example.org".into(),
                captured_at: 1,
                native_author_id: "native".into(),
                attribution: onecrew_creator_runtime::DiscoveryContactAttribution::AuthorPublished,
            }],
        };
        let snapshot = DiscoveryLibrarySnapshot {
            campaign_id: "campaign".into(),
            criteria_revision: 1,
            source_revision: 2,
            snapshot_hash: "b".repeat(64),
            rows: vec![row.clone()],
        };
        let saved = ProjectionRow {
            row_id: "actual-row".into(),
            search_hit_id: "hit".into(),
            candidate_id: row.candidate_id.clone(),
            native_author_id: row.native_author_id.clone(),
            verdict: row.verdict.clone(),
            reasons: row.reasons.clone(),
            evidence_version: row.evidence_version.clone(),
            contacts: row.contacts.clone(),
            criteria_revision: 1,
            source_revision: 2,
        };
        assert!(matches_projection(&row, &saved, &snapshot));
        for changed in [
            ProjectionRow {
                contacts: vec![],
                ..saved.clone()
            },
            ProjectionRow {
                verdict: "qualified".into(),
                ..saved.clone()
            },
            ProjectionRow {
                source_revision: 1,
                ..saved.clone()
            },
            ProjectionRow {
                criteria_revision: 2,
                ..saved.clone()
            },
            ProjectionRow {
                candidate_id: None,
                ..saved.clone()
            },
            ProjectionRow {
                native_author_id: Some("other".into()),
                ..saved.clone()
            },
            ProjectionRow {
                reasons: vec![],
                ..saved.clone()
            },
            ProjectionRow {
                evidence_version: "c".repeat(64),
                ..saved.clone()
            },
        ] {
            assert!(!matches_projection(&row, &changed, &snapshot));
        }
        let before = HashMap::from([("hit".into(), saved.clone())]);
        assert!(require_same_rows(&before, &before).is_ok());
        assert_eq!(
            require_same_rows(&before, &HashMap::new()).unwrap_err(),
            "library_row_identity_changed"
        );
        let replaced = HashMap::from([(
            "hit".into(),
            ProjectionRow {
                row_id: "replacement".into(),
                ..saved
            },
        )]);
        assert_eq!(
            require_same_rows(&before, &replaced).unwrap_err(),
            "library_row_identity_changed"
        );
        assert!(uncertain_write("library_invalid_response"));
        assert!(!uncertain_write("library_account_changed"));
        assert!(!uncertain_write("library_source_stale"));
    }

    fn test_table() -> LibraryTable {
        LibraryTable {
            id: "tbl1".into(),
            name: "虚拟货币_1002_1".into(),
            columns: COLUMNS
                .iter()
                .enumerate()
                .map(|(index, (name, data_type))| LibraryColumn {
                    id: format!("f{index}"),
                    name: (*name).into(),
                    data_type: (*data_type).into(),
                })
                .collect(),
        }
    }

    #[test]
    fn managed_columns_are_required_but_user_columns_survive() {
        let mut table = test_table();
        table.columns.push(LibraryColumn {
            id: "user".into(),
            name: "My notes".into(),
            data_type: "string".into(),
        });
        assert_eq!(column_ids(&table).unwrap()["Search hit ID"], "f1");
        table.columns[1].data_type = "int".into();
        assert_eq!(
            column_ids(&table).unwrap_err(),
            "library_table_schema_changed"
        );
    }

    #[test]
    fn search_hit_maps_to_unverified_library_row_by_real_field_ids() {
        let ids = column_ids(&test_table()).unwrap();
        let hit = DiscoverySearchHit {
            id: "hit-1".into(),
            campaign_id: "campaign-1".into(),
            platform: "twitter".into(),
            profile_ref: "browser-profile".into(),
            search_keyword: "AI tools".into(),
            source_url: "https://x.com/example/status/123".into(),
            title: "Tool demo".into(),
            author_label: "Example".into(),
            captured_at: 1234567890,
            author_id: None,
            author_url: None,
            author_observed_at: None,
        };
        let values = row_values(&hit, &ids);
        assert_eq!(values[&ids["Campaign ID"]], "campaign-1");
        assert_eq!(values[&ids["Search hit ID"]], "hit-1");
        assert_eq!(values[&ids["Source URL"]], hit.source_url);
        assert!(values[&ids["Author ID"]].is_null());
        assert!(values[&ids["Author URL"]].is_null());
        assert_eq!(values[&ids["Status"]], "Unverified search lead");
        assert_eq!(values.as_object().unwrap().len(), COLUMNS.len());
        let identified_hit = DiscoverySearchHit {
            author_id: Some("native-creator-1".into()),
            author_url: Some("https://x.com/example".into()),
            ..hit
        };
        let identified = row_values(&identified_hit, &ids);
        assert_eq!(identified[&ids["Author ID"]], "native-creator-1");
        assert_eq!(identified[&ids["Author URL"]], "https://x.com/example");
    }

    #[tokio::test]
    async fn unpaired_runtime_fails_before_library_access() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        assert_eq!(
            platform
                .sync_discovery_library("unknown-campaign".into())
                .await
                .unwrap_err(),
            "library_identity_unavailable"
        );
    }

    #[tokio::test]
    async fn interrupted_sync_wakes_joiners_and_allows_retry() {
        let directory = tempfile::tempdir().unwrap();
        let key = SyncKey {
            data_dir: directory.path().into(),
            campaign_id: "campaign-1".into(),
            account_id: "account-1".into(),
        };
        let (outcome, _) = tokio::sync::watch::channel(None);
        let flight = Arc::new(SyncFlight { outcome });
        SYNC_FLIGHTS
            .lock()
            .unwrap()
            .insert(key.clone(), flight.clone());
        let owner = SyncFlightOwner {
            key: key.clone(),
            flight: flight.clone(),
            finished: false,
        };
        let waiter = tokio::spawn(join_sync(flight));
        drop(owner);
        assert_eq!(
            waiter.await.unwrap().unwrap_err(),
            "library_sync_interrupted"
        );
        assert!(!SYNC_FLIGHTS.lock().unwrap().contains_key(&key));
    }
}
