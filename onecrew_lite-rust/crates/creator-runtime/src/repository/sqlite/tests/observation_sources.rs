use rusqlite::{Connection, params};

use crate::{
    AccountRefId, Confidence, CreateCreatorInput, CreatorId, LinkState, ObservationSourceInput,
    Platform, PlatformAccountObservationInput, SourceKind, TargetKind, UpdateCreatorInput,
    UpsertPlatformAccountInput,
    repository::{ContactRepository, CreatorRepository, TargetRepository},
};

use super::{super::creators::LOAD_OBSERVATION_SOURCES_SQL, repository};

/// The retired `onecrew.creator.profile.refresh` wrote an
/// `initial_platform_account` source with metadata reason `profile_refresh`,
/// one observation under it and the follower count; it wrote no audit kind of
/// its own. Those rows, beside the Creator's existing audit facts, stay
/// readable through the unchanged schema and read paths.
#[test]
fn historical_profile_refresh_facts_stay_readable_in_creator_reads_and_list() {
    let (_directory, repository) = repository();
    let creator = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Creator 42".into(),
            notes: None,
        })
        .expect("Creator");
    let creator_id = creator.creator.id;
    let operator = repository
        .upsert_account_ref(
            AccountRefId::new("x-refresh-operator"),
            Platform::X,
            "profile",
            "@operator",
        )
        .expect("operator account");
    let account = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::X,
            target_kind: TargetKind::User,
            external_id: "x-creator-42".into(),
        })
        .expect("platform account");
    repository
        .link_creator_account(
            &creator_id,
            &account.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .expect("account link");
    let source = repository
        .create_observation_source(ObservationSourceInput {
            kind: SourceKind::InitialPlatformAccount,
            platform: Some(Platform::X),
            source_url: Some("https://x.com/creator_42".into()),
            external_ref: Some("x-creator-42".into()),
            account_ref_id: Some(operator.id.clone()),
            metadata: serde_json::json!({ "reason": "profile_refresh" }),
        })
        .expect("historical refresh source");
    repository
        .append_platform_account_observation(PlatformAccountObservationInput {
            platform_account_id: account.id.clone(),
            source_id: source.id.clone(),
            handle: Some("creator_42".into()),
            display_name: Some("Creator 42".into()),
            profile_url: Some("https://x.com/creator_42".into()),
            public_data: serde_json::json!({
                "followers": 61_000,
                "profile_image_url": "https://pbs.twimg.com/creator_42_refreshed.jpg",
            }),
        })
        .expect("historical refresh observation");
    repository
        .update_creator(UpdateCreatorInput {
            tags: None,
            creator_id: creator_id.clone(),
            display_name: None,
            notes: None,
            score: None,
            assessment: None,
            follower_counts: Some([(Platform::X, 61_000)].into_iter().collect()),
        })
        .expect("historical refresh follower count");
    repository
        .lock()
        .expect("repository lock")
        .execute(
            "INSERT INTO audit_events (id, kind, creator_id, account_ref_id, payload_json, created_at)
             VALUES ('audit-historical-save', 'creator_saved', ?1, ?2, '{}', 1)",
            params![creator_id.as_str(), operator.id.as_str()],
        )
        .expect("historical audit fact");

    let record = repository.get_creator(&creator_id).expect("Creator read");
    let refresh_source = record
        .observation_sources
        .iter()
        .find(|candidate| candidate.id.as_str() == source.id.as_str())
        .expect("historical refresh source is projected");
    assert_eq!(
        serde_json::to_value(refresh_source).expect("serialize source")["kind"],
        "initial_platform_account"
    );
    assert_eq!(
        refresh_source.metadata,
        serde_json::json!({ "reason": "profile_refresh" })
    );
    assert_eq!(
        refresh_source
            .account_ref_id
            .as_ref()
            .map(AccountRefId::as_str),
        Some(operator.id.as_str())
    );
    let observation = record
        .account_observations
        .iter()
        .find(|candidate| candidate.source_id.as_str() == source.id.as_str())
        .expect("historical refresh observation is projected");
    assert_eq!(observation.handle.as_deref(), Some("creator_42"));
    assert_eq!(observation.public_data["followers"], 61_000);
    assert_eq!(
        record.creator.follower_counts.get(&Platform::X),
        Some(&61_000)
    );

    let summary = repository
        .list_creators(Default::default())
        .expect("Creator list")
        .items
        .into_iter()
        .find(|summary| summary.id == creator_id)
        .expect("Creator in list");
    assert_eq!(summary.primary_handle.as_deref(), Some("creator_42"));
    assert_eq!(
        summary.avatar_url.as_deref(),
        Some("https://pbs.twimg.com/creator_42_refreshed.jpg")
    );
    assert_eq!(summary.follower_counts.get(&Platform::X), Some(&61_000));
}

// This retains the pre-rewrite Q5 membership semantics as a test-only oracle.
// Historical SQL ordered only by observed_at DESC, so equal-time order was
// undefined. The source.id ASC tie-break normalizes that undefined order for a
// deterministic comparison; it is neither rowid compatibility nor a claim that
// the historical query defined this tie order. Production executes
// LOAD_OBSERVATION_SOURCES_SQL once and never runs this query.
const NORMALIZED_HISTORICAL_OBSERVATION_SOURCES_SQL: &str =
    "SELECT DISTINCT source.id, source.kind, source.platform, source.source_url,
            source.external_ref, source.account_ref_id, source.metadata_json,
            source.observed_at
     FROM observation_sources source
     LEFT JOIN platform_account_observations observation ON observation.source_id = source.id
     LEFT JOIN creator_account_links link
       ON link.platform_account_id = observation.platform_account_id
     LEFT JOIN creator_origins origin ON origin.source_id = source.id
     WHERE link.creator_id = :creator_id OR origin.creator_id = :creator_id OR EXISTS (
       SELECT 1 FROM locator_observations locator_observation
       WHERE locator_observation.source_id = source.id
         AND (
           locator_observation.creator_id = :creator_id OR
           locator_observation.platform_account_id IN (
             SELECT platform_account_id FROM creator_account_links
             WHERE creator_id = :creator_id
           )
         )
     )
     ORDER BY source.observed_at DESC, source.id ASC";

#[derive(Debug, PartialEq)]
struct SourceRow {
    id: String,
    kind: String,
    platform: Option<String>,
    source_url: Option<String>,
    external_ref: Option<String>,
    account_ref_id: Option<String>,
    metadata_json: String,
    observed_at: u64,
}

fn query_source_rows(connection: &Connection, sql: &str, creator_id: &str) -> Vec<SourceRow> {
    connection
        .prepare(sql)
        .expect("prepare observation-source query")
        .query_map(
            rusqlite::named_params! { ":creator_id": creator_id },
            |row| {
                Ok(SourceRow {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    platform: row.get(2)?,
                    source_url: row.get(3)?,
                    external_ref: row.get(4)?,
                    account_ref_id: row.get(5)?,
                    metadata_json: row.get(6)?,
                    observed_at: row.get(7)?,
                })
            },
        )
        .expect("query observation sources")
        .collect::<Result<Vec<_>, _>>()
        .expect("observation source rows")
}

fn insert_fixture(connection: &Connection) {
    // The five target Sources share observed_at=200 and are inserted in reverse
    // id order. Sources a-d are reachable only through UNION arms 1-4,
    // respectively; source e overlaps all four arms and must still appear once.
    // The account-backed arms also exercise multi-owner claimed/rejected links,
    // while their locator rows intentionally have a NULL creator owner.
    connection
        .execute_batch(
            "INSERT INTO creators (id, display_name, notes, created_at, updated_at, archived_at)
             VALUES
               ('creator-target', 'Target', NULL, 1, 1, NULL),
               ('creator-other-a', 'Other A', NULL, 1, 1, NULL),
               ('creator-other-b', 'Other B', NULL, 1, 1, NULL);
             INSERT INTO creator_relationships
               (creator_id, stage, last_touch_at, created_at, updated_at)
             VALUES
               ('creator-target', 'prospect', NULL, 1, 1),
               ('creator-other-a', 'prospect', NULL, 1, 1),
               ('creator-other-b', 'prospect', NULL, 1, 1);
             INSERT INTO platform_accounts
               (id, platform, target_kind, external_id, created_at, updated_at)
             VALUES
               ('account-claimed-multi', 'x', 'user', 'claimed-multi', 1, 1),
               ('account-rejected-multi', 'youtube', 'creator', 'rejected-multi', 1, 1),
               ('account-other-only', 'instagram', 'user', 'other', 1, 1);
             INSERT INTO creator_account_links
               (id, creator_id, platform_account_id, state, confidence, created_at, updated_at)
             VALUES
               ('link-target-claimed', 'creator-target', 'account-claimed-multi', 'claimed', 'asserted', 1, 1),
               ('link-other-rejected', 'creator-other-a', 'account-claimed-multi', 'rejected', 'possible', 1, 1),
               ('link-target-rejected', 'creator-target', 'account-rejected-multi', 'rejected', 'possible', 1, 1),
               ('link-other-confirmed', 'creator-other-b', 'account-rejected-multi', 'confirmed', 'exact', 1, 1),
               ('link-other-only', 'creator-other-a', 'account-other-only', 'confirmed', 'exact', 1, 1);
             INSERT INTO account_refs
               (id, platform, kind, label, active, availability, created_at, updated_at)
             VALUES ('account-ref-x', 'x', 'profile', 'operator', 1, 'available', 1, 1);
             INSERT INTO observation_sources
               (id, kind, platform, source_url, external_ref, account_ref_id, metadata_json, observed_at)
             VALUES
               ('source-e-four-arm-overlap', 'initial_platform_account', 'x', 'https://x.example/overlap', 'overlap-ref', 'account-ref-x', '{\"route\":\"all-four\",\"nested\":{\"ok\":true}}', 200),
               ('source-d-account-locator', 'channel_description', 'youtube', 'https://youtube.example/rejected', 'rejected-ref', NULL, '{\"route\":\"account-locator\",\"owner\":null,\"link_state\":\"rejected\"}', 200),
               ('source-c-direct-locator', 'manual_business_email', NULL, NULL, 'direct-ref', NULL, '{\"route\":\"direct-locator\",\"owner\":\"creator-target\"}', 200),
               ('source-b-origin', 'manual_entry', NULL, 'https://creator.example/origin', 'origin-ref', NULL, '{\"route\":\"origin\",\"labels\":[\"manual\",\"stable\"]}', 200),
               ('source-a-account-observation', 'profile_bio', 'x', NULL, 'account-observation-ref', 'account-ref-x', '{\"route\":\"account-observation\",\"link_state\":\"claimed\",\"score\":7}', 200),
               ('source-other-only', 'profile_website', 'instagram', NULL, NULL, NULL, '{}', 400);
             INSERT INTO contact_locators
               (id, channel, kind, normalized_value, display_value, created_at, updated_at)
             VALUES
               ('locator-direct', 'email', 'email_address', 'direct@example.com', 'direct@example.com', 1, 1),
               ('locator-account', 'x', 'handle', 'overlap', '@overlap', 1, 1),
               ('locator-rejected', 'youtube', 'channel_id', 'rejected', 'rejected', 1, 1),
               ('locator-other', 'instagram', 'handle', 'other', '@other', 1, 1);
             INSERT INTO creator_origins
               (id, kind, creator_id, source_id, discovery_id,
                source_creator_id, source_contact_route_id, operator_account_ref_id,
                source_resolved_target_id, administrator_role, community_external_id,
                community_title, community_handle, community_target_kind,
                import_content_sha256, imported_creator_id, discovered_at)
             VALUES
               ('origin-overlap', 'manual_entry', 'creator-target', 'source-e-four-arm-overlap', NULL,
                NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, 1),
               ('origin-only', 'platform_search', 'creator-target', 'source-b-origin', 'discovery-origin-only',
                NULL, NULL, 'account-ref-x', NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, 1);
             INSERT INTO platform_account_observations
               (id, platform_account_id, source_id, handle, display_name, profile_url,
                public_json, observed_at)
             VALUES
               ('observation-overlap', 'account-claimed-multi', 'source-e-four-arm-overlap', 'overlap', 'Overlap', 'https://x.example/overlap', '{\"followers\":42,\"labels\":[\"one\",\"two\"]}', 200),
               ('observation-account-only', 'account-claimed-multi', 'source-a-account-observation', 'claimed', NULL, NULL, '{}', 200),
               ('observation-other', 'account-other-only', 'source-other-only', 'other', NULL, NULL, '{}', 400);
             INSERT INTO locator_observations
               (id, locator_id, source_id, creator_id, platform_account_id, observed_value, observed_at)
             VALUES
               ('locator-observation-overlap-direct', 'locator-direct', 'source-e-four-arm-overlap', 'creator-target', NULL, 'direct@example.com', 200),
               ('locator-observation-overlap-account', 'locator-account', 'source-e-four-arm-overlap', NULL, 'account-claimed-multi', '@overlap', 200),
               ('locator-observation-rejected-account', 'locator-rejected', 'source-d-account-locator', NULL, 'account-rejected-multi', 'rejected', 200),
               ('locator-observation-direct-only', 'locator-direct', 'source-c-direct-locator', 'creator-target', NULL, 'direct@example.com', 200),
               ('locator-observation-other', 'locator-other', 'source-other-only', NULL, 'account-other-only', '@other', 400);",
        )
        .expect("insert observation-source fixture");
}

#[test]
fn union_source_id_query_matches_normalized_historical_membership_and_stable_order() {
    let (_directory, repository) = repository();
    {
        let connection = repository.lock().expect("repository lock");
        insert_fixture(&connection);

        let normalized_historical = query_source_rows(
            &connection,
            NORMALIZED_HISTORICAL_OBSERVATION_SOURCES_SQL,
            "creator-target",
        );
        let rewritten =
            query_source_rows(&connection, LOAD_OBSERVATION_SOURCES_SQL, "creator-target");
        assert_eq!(rewritten, normalized_historical);
        assert_eq!(
            rewritten
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "source-a-account-observation",
                "source-b-origin",
                "source-c-direct-locator",
                "source-d-account-locator",
                "source-e-four-arm-overlap",
            ]
        );
    }

    let record = repository
        .get_creator(&CreatorId::new("creator-target"))
        .expect("load Creator through production repository path");
    assert_eq!(
        serde_json::to_value(&record.observation_sources).expect("serialize observation sources"),
        serde_json::json!([
            {
                "id": "source-a-account-observation",
                "kind": "profile_bio",
                "platform": "x",
                "source_url": null,
                "external_ref": "account-observation-ref",
                "account_ref_id": "account-ref-x",
                "metadata": {"route": "account-observation", "link_state": "claimed", "score": 7},
                "observed_at": 200
            },
            {
                "id": "source-b-origin",
                "kind": "manual_entry",
                "platform": null,
                "source_url": "https://creator.example/origin",
                "external_ref": "origin-ref",
                "account_ref_id": null,
                "metadata": {"route": "origin", "labels": ["manual", "stable"]},
                "observed_at": 200
            },
            {
                "id": "source-c-direct-locator",
                "kind": "manual_business_email",
                "platform": null,
                "source_url": null,
                "external_ref": "direct-ref",
                "account_ref_id": null,
                "metadata": {"route": "direct-locator", "owner": "creator-target"},
                "observed_at": 200
            },
            {
                "id": "source-d-account-locator",
                "kind": "channel_description",
                "platform": "youtube",
                "source_url": "https://youtube.example/rejected",
                "external_ref": "rejected-ref",
                "account_ref_id": null,
                "metadata": {"route": "account-locator", "owner": null, "link_state": "rejected"},
                "observed_at": 200
            },
            {
                "id": "source-e-four-arm-overlap",
                "kind": "initial_platform_account",
                "platform": "x",
                "source_url": "https://x.example/overlap",
                "external_ref": "overlap-ref",
                "account_ref_id": "account-ref-x",
                "metadata": {"route": "all-four", "nested": {"ok": true}},
                "observed_at": 200
            }
        ])
    );
    let overlap_observation = record
        .account_observations
        .iter()
        .find(|observation| observation.id.as_str() == "observation-overlap")
        .expect("overlap account observation");
    assert_eq!(
        overlap_observation.public_data,
        serde_json::json!({"followers": 42, "labels": ["one", "two"]})
    );
}

#[test]
fn production_query_collects_four_narrow_paths_before_primary_key_lookup() {
    let normalized = LOAD_OBSERVATION_SOURCES_SQL
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    assert_eq!(normalized.matches(" union ").count(), 3, "{normalized}");
    assert_eq!(
        normalized
            .matches("select locator_observation.source_id")
            .count(),
        2,
        "{normalized}"
    );
    assert!(
        normalized
            .contains("join observation_sources as source on source.id = source_ids.source_id"),
        "{normalized}"
    );
    assert!(
        normalized.ends_with("order by source.observed_at desc, source.id asc;"),
        "{normalized}"
    );
    for forbidden in ["select distinct", "left join", " exists ", " or ", "rowid"] {
        assert!(
            !normalized.contains(forbidden),
            "found {forbidden:?} in {normalized}"
        );
    }

    let (_directory, repository) = repository();
    let connection = repository.lock().expect("repository lock");
    insert_fixture(&connection);
    connection
        .execute_batch("PRAGMA automatic_index = OFF;")
        .expect("disable automatic indexes for persistent-index evidence");
    let explain_sql = format!("EXPLAIN QUERY PLAN {LOAD_OBSERVATION_SOURCES_SQL}");
    let plan = connection
        .prepare(&explain_sql)
        .expect("prepare rewritten Q5 plan")
        .query_map(params!["missing-creator"], |row| row.get::<_, String>(3))
        .expect("query rewritten Q5 plan")
        .collect::<Result<Vec<_>, _>>()
        .expect("rewritten Q5 plan details");
    assert!(
        plan.iter().all(|detail| !detail.contains("CORRELATED")),
        "{plan:#?}"
    );
    assert!(
        plan.iter().all(|detail| !detail.contains("LEFT-JOIN")),
        "{plan:#?}"
    );
    assert!(
        plan.iter().all(|detail| !detail.contains("DISTINCT")),
        "{plan:#?}"
    );
    assert!(
        plan.iter()
            .any(|detail| detail.contains("USE TEMP B-TREE FOR ORDER BY")),
        "the explicit observed_at/source.id order must be represented in the production Q5 plan: {plan:#?}"
    );
    for expected in [
        "SEARCH origin USING COVERING INDEX creator_origins_by_creator_recency (creator_id=?)",
        "SEARCH locator_observation USING COVERING INDEX locator_observations_by_creator_source (creator_id=?)",
        "SEARCH locator_observation USING COVERING INDEX locator_observations_by_platform_account_source (platform_account_id=?)",
    ] {
        assert!(
            plan.iter().any(|detail| detail.contains(expected)),
            "missing {expected:?} in production Q5 plan: {plan:#?}"
        );
    }
    for forbidden in [
        "SCAN origin",
        "SCAN locator_observation",
        "SCAN link",
        "SCAN observation",
        "SCAN source USING",
    ] {
        assert!(
            plan.iter().all(|detail| !detail.contains(forbidden)),
            "found {forbidden:?} in production Q5 plan: {plan:#?}"
        );
    }
}
