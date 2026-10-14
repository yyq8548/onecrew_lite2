use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension};

use super::{
    CURRENT_SCHEMA_VERSION,
    current::create_current_schema_unchecked,
    migrations::{
        downgrade_reference_to_v27, downgrade_reference_to_v28, downgrade_reference_to_v29,
        downgrade_reference_to_v30, downgrade_reference_to_v31, downgrade_reference_to_v32,
        downgrade_reference_to_v33, downgrade_reference_to_v34, downgrade_reference_to_v35,
        downgrade_reference_to_v36,
    },
};

const REQUIRED_TABLES: &[&str] = &[
    "account_refs",
    "account_relationships",
    "audit_events",
    "creator_origins",
    "contact_locators",
    "conversations",
    "conversation_contact_routes",
    "creator_account_links",
    "creator_contact_route_delete_operations",
    "creator_contact_route_dependencies",
    "creator_contact_routes",
    "creator_import_operations",
    "creator_import_receipts",
    "creator_groups",
    "creator_relationships",
    "creators",
    "email_attachments",
    "email_auth_connections",
    "email_drafts",
    "email_recipients",
    "email_templates",
    "email_provider_identities",
    "email_threads",
    "emails",
    "identity_conflicts",
    "idempotency_records",
    "locator_observations",
    "locator_resolutions",
    "mailbox_credential_deletion_guards",
    "mailbox_identity_mappings",
    "mailbox_refs",
    "mailbox_sync_incremental_continuations",
    "mailbox_sync_state",
    "messages",
    "observation_sources",
    "platform_account_observations",
    "platform_accounts",
    "resolved_targets",
    "reply_triage",
    "touchpoints",
];

/// Columns whose owner was retired by a released edge. They are rejected by
/// name on the current schema so drift reads as the retired concept rather
/// than as an anonymous column difference; predecessor references still
/// carry them and are compared exactly against their frozen DDL instead.
const RETIRED_CURRENT_COLUMNS: &[&str] = &["campaign_id"];

#[derive(Debug, PartialEq, Eq)]
struct ColumnDefinition {
    name: String,
    data_type: String,
    not_null: bool,
    default_value: Option<String>,
    primary_key_position: i64,
}

#[derive(Debug, PartialEq, Eq)]
struct ForeignKeyDefinition {
    id: i64,
    sequence: i64,
    target_table: String,
    source_column: String,
    target_column: Option<String>,
    on_update: String,
    on_delete: String,
    match_clause: String,
}

pub(super) fn validate_current_database(connection: &Connection) -> Result<(), String> {
    validate_integrity(connection)?;
    validate_current_schema(connection)?;
    let mut statement = connection
        .prepare("SELECT tags_json FROM creators")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    for row in rows {
        let tags: Vec<String> = serde_json::from_str(&row.map_err(|error| error.to_string())?)
            .map_err(|_| "stored Creator tags are invalid".to_string())?;
        if crate::creators::normalize_creator_tags(tags.clone())
            .map_err(|error| error.to_string())?
            != tags
        {
            return Err("stored Creator tags are not normalized".into());
        }
    }
    validate_email_archive_markers(connection)?;
    validate_no_shared_locator_owners(connection)
}

fn validate_email_archive_markers(connection: &Connection) -> Result<(), String> {
    let invalid = connection
        .query_row(
            "SELECT thread.id FROM email_threads thread
             WHERE thread.archived_at IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1 FROM emails email
                   WHERE email.thread_id = thread.id
                     AND email.direction = 'inbound'
               )
             LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some(thread_id) = invalid {
        return Err(format!(
            "email thread {thread_id} has an archive marker without an inbound message"
        ));
    }
    Ok(())
}

fn validate_v27_database(connection: &Connection) -> Result<(), String> {
    validate_integrity(connection)?;
    validate_v27_schema(connection)?;
    validate_email_archive_markers(connection)?;
    validate_no_shared_locator_owners(connection)
}

fn validate_v28_database(connection: &Connection) -> Result<(), String> {
    validate_integrity(connection)?;
    validate_v28_schema(connection)?;
    validate_email_archive_markers(connection)?;
    validate_no_shared_locator_owners(connection)
}

fn validate_v29_database(connection: &Connection) -> Result<(), String> {
    validate_integrity(connection)?;
    let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
    create_legacy_reference_schema(&reference)?;
    downgrade_reference_to_v29(&reference)?;
    validate_schema_against(connection, &reference, 29)?;
    validate_email_archive_markers(connection)?;
    validate_no_shared_locator_owners(connection)
}

fn validate_v30_database(connection: &Connection) -> Result<(), String> {
    validate_integrity(connection)?;
    let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
    create_legacy_reference_schema(&reference)?;
    downgrade_reference_to_v30(&reference)?;
    validate_schema_against(connection, &reference, 30)?;
    validate_email_archive_markers(connection)?;
    validate_no_shared_locator_owners(connection)
}

fn validate_v31_database(connection: &Connection) -> Result<(), String> {
    validate_integrity(connection)?;
    let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
    create_legacy_reference_schema(&reference)?;
    downgrade_reference_to_v31(&reference)?;
    validate_schema_against(connection, &reference, 31)?;
    validate_email_archive_markers(connection)?;
    validate_no_shared_locator_owners(connection)
}
fn validate_v32_database(connection: &Connection) -> Result<(), String> {
    validate_integrity(connection)?;
    let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
    create_legacy_reference_schema(&reference)?;
    downgrade_reference_to_v32(&reference)?;
    validate_schema_against(connection, &reference, 32)?;
    validate_email_archive_markers(connection)?;
    validate_no_shared_locator_owners(connection)
}

pub(super) fn validate_database_for_version(
    connection: &Connection,
    version: i64,
) -> Result<(), String> {
    match version {
        27 => validate_v27_database(connection),
        28 => validate_v28_database(connection),
        29 => validate_v29_database(connection),
        30 => validate_v30_database(connection),
        31 => validate_v31_database(connection),
        32 => validate_v32_database(connection),
        33 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
            create_legacy_reference_schema(&reference)?;
            downgrade_reference_to_v33(&reference)?;
            validate_schema_against(connection, &reference, 33)?;
            validate_email_archive_markers(connection)?;
            validate_no_shared_locator_owners(connection)
        }
        34 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
            create_legacy_reference_schema(&reference)?;
            downgrade_reference_to_v34(&reference)?;
            validate_schema_against(connection, &reference, 34)?;
            validate_email_archive_markers(connection)?;
            validate_no_shared_locator_owners(connection)
        }
        35 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
            create_legacy_reference_schema(&reference)?;
            downgrade_reference_to_v35(&reference)?;
            validate_schema_against(connection, &reference, 35)?;
            validate_email_archive_markers(connection)?;
            validate_no_shared_locator_owners(connection)
        }
        36 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
            create_legacy_reference_schema(&reference)?;
            downgrade_reference_to_v36(&reference)?;
            validate_schema_against(connection, &reference, 36)?;
            validate_email_archive_markers(connection)?;
            validate_no_shared_locator_owners(connection)
        }
        37 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
            create_legacy_reference_schema(&reference)?;
            super::migrations::downgrade_reference_to_v37(&reference)?;
            validate_schema_against(connection, &reference, 37)?;
            validate_email_archive_markers(connection)?;
            validate_no_shared_locator_owners(connection)
        }
        38 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
            create_legacy_reference_schema(&reference)?;
            super::migrations::downgrade_reference_to_v38(&reference)?;
            validate_schema_against(connection, &reference, 38)?;
            validate_email_archive_markers(connection)?;
            validate_no_shared_locator_owners(connection)
        }
        39 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
            create_legacy_reference_schema(&reference)?;
            super::migrations::downgrade_reference_to_v39(&reference)?;
            validate_schema_against(connection, &reference, 39)
        }
        40 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_legacy_reference_schema(&reference)?;
            super::migrations::downgrade_reference_to_v40(&reference)?;
            validate_schema_against(connection, &reference, 40)
        }
        41 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_legacy_reference_schema(&reference)?;
            super::migrations::downgrade_reference_to_v41(&reference)?;
            validate_schema_against(connection, &reference, 41)
        }
        42 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_legacy_reference_schema(&reference)?;
            super::migrations::downgrade_reference_to_v42(&reference)?;
            validate_schema_against(connection, &reference, 42)
        }
        43 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::strip_identity_reviews_reference(&reference)?;
            super::migrations::strip_target_increases_reference(&reference)?;
            super::migrations::strip_budget_increases_reference(&reference)?;
            super::migrations::strip_video_read_failures_reference(&reference)?;
            super::migrations::strip_review_reference(&reference)?;
            super::migrations::strip_library_source_reference(&reference)?;
            super::migrations::strip_content_identity_reference(&reference)?;
            super::migrations::strip_profile_identity_reference(&reference)?;
            super::migrations::strip_identity_reference(&reference)?;
            super::migrations::strip_twitter_content_reference(&reference)?;
            super::migrations::strip_twitter_reference(&reference)?;
            super::migrations::downgrade_reference_to_v43(&reference)?;
            validate_schema_against(connection, &reference, 43)
        }
        44 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::strip_identity_reviews_reference(&reference)?;
            super::migrations::strip_target_increases_reference(&reference)?;
            super::migrations::strip_budget_increases_reference(&reference)?;
            super::migrations::strip_video_read_failures_reference(&reference)?;
            super::migrations::strip_review_reference(&reference)?;
            super::migrations::strip_library_source_reference(&reference)?;
            super::migrations::strip_content_identity_reference(&reference)?;
            super::migrations::strip_profile_identity_reference(&reference)?;
            super::migrations::strip_identity_reference(&reference)?;
            super::migrations::strip_twitter_content_reference(&reference)?;
            super::migrations::strip_twitter_reference(&reference)?;
            validate_schema_against(connection, &reference, 44)
        }
        45 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::strip_identity_reviews_reference(&reference)?;
            super::migrations::strip_target_increases_reference(&reference)?;
            super::migrations::strip_budget_increases_reference(&reference)?;
            super::migrations::strip_video_read_failures_reference(&reference)?;
            super::migrations::strip_review_reference(&reference)?;
            super::migrations::strip_library_source_reference(&reference)?;
            super::migrations::strip_content_identity_reference(&reference)?;
            super::migrations::strip_profile_identity_reference(&reference)?;
            super::migrations::strip_identity_reference(&reference)?;
            super::migrations::strip_twitter_content_reference(&reference)?;
            validate_schema_against(connection, &reference, 45)
        }
        46 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::strip_identity_reviews_reference(&reference)?;
            super::migrations::strip_target_increases_reference(&reference)?;
            super::migrations::strip_budget_increases_reference(&reference)?;
            super::migrations::strip_video_read_failures_reference(&reference)?;
            super::migrations::strip_review_reference(&reference)?;
            super::migrations::strip_library_source_reference(&reference)?;
            super::migrations::strip_content_identity_reference(&reference)?;
            super::migrations::strip_profile_identity_reference(&reference)?;
            super::migrations::strip_identity_reference(&reference)?;
            validate_schema_against(connection, &reference, 46)
        }
        47 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::strip_identity_reviews_reference(&reference)?;
            super::migrations::strip_target_increases_reference(&reference)?;
            super::migrations::strip_budget_increases_reference(&reference)?;
            super::migrations::strip_video_read_failures_reference(&reference)?;
            super::migrations::strip_review_reference(&reference)?;
            super::migrations::strip_library_source_reference(&reference)?;
            super::migrations::strip_content_identity_reference(&reference)?;
            super::migrations::strip_profile_identity_reference(&reference)?;
            validate_schema_against(connection, &reference, 47)
        }
        48 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::strip_identity_reviews_reference(&reference)?;
            super::migrations::strip_target_increases_reference(&reference)?;
            super::migrations::strip_budget_increases_reference(&reference)?;
            super::migrations::strip_video_read_failures_reference(&reference)?;
            super::migrations::strip_review_reference(&reference)?;
            super::migrations::strip_library_source_reference(&reference)?;
            super::migrations::strip_content_identity_reference(&reference)?;
            validate_schema_against(connection, &reference, 48)
        }
        49 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::strip_identity_reviews_reference(&reference)?;
            super::migrations::strip_target_increases_reference(&reference)?;
            super::migrations::strip_budget_increases_reference(&reference)?;
            super::migrations::strip_video_read_failures_reference(&reference)?;
            super::migrations::strip_review_reference(&reference)?;
            super::migrations::strip_library_source_reference(&reference)?;
            validate_schema_against(connection, &reference, 49)
        }
        50 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::strip_identity_reviews_reference(&reference)?;
            super::migrations::strip_target_increases_reference(&reference)?;
            super::migrations::strip_budget_increases_reference(&reference)?;
            super::migrations::strip_video_read_failures_reference(&reference)?;
            super::migrations::strip_review_reference(&reference)?;
            validate_schema_against(connection, &reference, 50)
        }
        51 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::strip_identity_reviews_reference(&reference)?;
            super::migrations::strip_target_increases_reference(&reference)?;
            super::migrations::restore_review_v1_reference(&reference)?;
            super::migrations::strip_budget_increases_reference(&reference)?;
            super::migrations::strip_video_read_failures_reference(&reference)?;
            validate_schema_against(connection, &reference, 51)
        }
        52 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::strip_identity_reviews_reference(&reference)?;
            super::migrations::strip_target_increases_reference(&reference)?;
            super::migrations::restore_review_v1_reference(&reference)?;
            super::migrations::strip_budget_increases_reference(&reference)?;
            validate_schema_against(connection, &reference, 52)
        }
        53 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::strip_identity_reviews_reference(&reference)?;
            super::migrations::strip_target_increases_reference(&reference)?;
            super::migrations::restore_review_v1_reference(&reference)?;
            validate_schema_against(connection, &reference, 53)
        }
        54 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::strip_identity_reviews_reference(&reference)?;
            super::migrations::strip_target_increases_reference(&reference)?;
            validate_schema_against(connection, &reference, 54)
        }
        55 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::strip_identity_reviews_reference(&reference)?;
            validate_schema_against(connection, &reference, 55)
        }
        56 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::strip_library_identity_versions_reference(&reference)?;
            validate_schema_against(connection, &reference, 56)
        }
        57 | 58 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::v71_to_v72::strip_reference(&reference)?;
            super::migrations::v70_to_v71::strip_reference(&reference)?;
            super::migrations::v69_to_v70::strip_reference(&reference)?;
            super::migrations::v68_to_v69::strip_reference(&reference)?;
            super::migrations::v67_to_v68::strip_reference(&reference)?;
            super::migrations::v66_to_v67::strip_reference(&reference)?;
            super::migrations::v65_to_v66::strip_reference(&reference)?;
            super::migrations::v64_to_v65::strip_reference(&reference)?;
            super::migrations::v63_to_v64::strip_reference(&reference)?;
            super::migrations::v62_to_v63::strip_reference(&reference)?;
            super::migrations::v61_to_v62::strip_reference(&reference)?;
            super::migrations::v58_to_v59::strip_language_samples_reference(&reference)?;
            super::migrations::v59_to_v60::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, version)
        }
        59 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::v71_to_v72::strip_reference(&reference)?;
            super::migrations::v70_to_v71::strip_reference(&reference)?;
            super::migrations::v69_to_v70::strip_reference(&reference)?;
            super::migrations::v68_to_v69::strip_reference(&reference)?;
            super::migrations::v67_to_v68::strip_reference(&reference)?;
            super::migrations::v66_to_v67::strip_reference(&reference)?;
            super::migrations::v65_to_v66::strip_reference(&reference)?;
            super::migrations::v64_to_v65::strip_reference(&reference)?;
            super::migrations::v63_to_v64::strip_reference(&reference)?;
            super::migrations::v62_to_v63::strip_reference(&reference)?;
            super::migrations::v61_to_v62::strip_reference(&reference)?;
            super::migrations::v60_to_v61::strip_reference(&reference)?;
            super::migrations::v59_to_v60::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, version)
        }
        60 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::v71_to_v72::strip_reference(&reference)?;
            super::migrations::v70_to_v71::strip_reference(&reference)?;
            super::migrations::v69_to_v70::strip_reference(&reference)?;
            super::migrations::v68_to_v69::strip_reference(&reference)?;
            super::migrations::v67_to_v68::strip_reference(&reference)?;
            super::migrations::v66_to_v67::strip_reference(&reference)?;
            super::migrations::v65_to_v66::strip_reference(&reference)?;
            super::migrations::v64_to_v65::strip_reference(&reference)?;
            super::migrations::v63_to_v64::strip_reference(&reference)?;
            super::migrations::v62_to_v63::strip_reference(&reference)?;
            super::migrations::v61_to_v62::strip_reference(&reference)?;
            super::migrations::v60_to_v61::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, version)
        }
        61 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::v71_to_v72::strip_reference(&reference)?;
            super::migrations::v70_to_v71::strip_reference(&reference)?;
            super::migrations::v69_to_v70::strip_reference(&reference)?;
            super::migrations::v68_to_v69::strip_reference(&reference)?;
            super::migrations::v67_to_v68::strip_reference(&reference)?;
            super::migrations::v66_to_v67::strip_reference(&reference)?;
            super::migrations::v65_to_v66::strip_reference(&reference)?;
            super::migrations::v64_to_v65::strip_reference(&reference)?;
            super::migrations::v63_to_v64::strip_reference(&reference)?;
            super::migrations::v62_to_v63::strip_reference(&reference)?;
            super::migrations::v61_to_v62::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, version)
        }
        62 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::v71_to_v72::strip_reference(&reference)?;
            super::migrations::v70_to_v71::strip_reference(&reference)?;
            super::migrations::v69_to_v70::strip_reference(&reference)?;
            super::migrations::v68_to_v69::strip_reference(&reference)?;
            super::migrations::v67_to_v68::strip_reference(&reference)?;
            super::migrations::v66_to_v67::strip_reference(&reference)?;
            super::migrations::v65_to_v66::strip_reference(&reference)?;
            super::migrations::v64_to_v65::strip_reference(&reference)?;
            super::migrations::v63_to_v64::strip_reference(&reference)?;
            super::migrations::v62_to_v63::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, version)
        }
        63 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::v71_to_v72::strip_reference(&reference)?;
            super::migrations::v70_to_v71::strip_reference(&reference)?;
            super::migrations::v69_to_v70::strip_reference(&reference)?;
            super::migrations::v68_to_v69::strip_reference(&reference)?;
            super::migrations::v67_to_v68::strip_reference(&reference)?;
            super::migrations::v66_to_v67::strip_reference(&reference)?;
            super::migrations::v65_to_v66::strip_reference(&reference)?;
            super::migrations::v64_to_v65::strip_reference(&reference)?;
            super::migrations::v63_to_v64::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, version)
        }
        64 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::v71_to_v72::strip_reference(&reference)?;
            super::migrations::v70_to_v71::strip_reference(&reference)?;
            super::migrations::v69_to_v70::strip_reference(&reference)?;
            super::migrations::v68_to_v69::strip_reference(&reference)?;
            super::migrations::v67_to_v68::strip_reference(&reference)?;
            super::migrations::v66_to_v67::strip_reference(&reference)?;
            super::migrations::v65_to_v66::strip_reference(&reference)?;
            super::migrations::v64_to_v65::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, version)
        }
        65 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::v71_to_v72::strip_reference(&reference)?;
            super::migrations::v70_to_v71::strip_reference(&reference)?;
            super::migrations::v69_to_v70::strip_reference(&reference)?;
            super::migrations::v68_to_v69::strip_reference(&reference)?;
            super::migrations::v67_to_v68::strip_reference(&reference)?;
            super::migrations::v66_to_v67::strip_reference(&reference)?;
            super::migrations::v65_to_v66::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, version)
        }
        66 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::v71_to_v72::strip_reference(&reference)?;
            super::migrations::v70_to_v71::strip_reference(&reference)?;
            super::migrations::v69_to_v70::strip_reference(&reference)?;
            super::migrations::v68_to_v69::strip_reference(&reference)?;
            super::migrations::v67_to_v68::strip_reference(&reference)?;
            super::migrations::v66_to_v67::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, version)
        }
        67 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::v71_to_v72::strip_reference(&reference)?;
            super::migrations::v70_to_v71::strip_reference(&reference)?;
            super::migrations::v69_to_v70::strip_reference(&reference)?;
            super::migrations::v68_to_v69::strip_reference(&reference)?;
            super::migrations::v67_to_v68::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, version)
        }
        68 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::v71_to_v72::strip_reference(&reference)?;
            super::migrations::v70_to_v71::strip_reference(&reference)?;
            super::migrations::v69_to_v70::strip_reference(&reference)?;
            super::migrations::v68_to_v69::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, version)
        }
        69 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::v71_to_v72::strip_reference(&reference)?;
            super::migrations::v70_to_v71::strip_reference(&reference)?;
            super::migrations::v69_to_v70::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, version)
        }
        70 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::v71_to_v72::strip_reference(&reference)?;
            super::migrations::v70_to_v71::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, version)
        }
        71 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            super::migrations::v71_to_v72::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, version)
        }
        72 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            super::migrations::v72_to_v73::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, version)
        }
        73 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            super::migrations::v73_to_v74::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, version)
        }
        74 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            super::migrations::v74_to_v75::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, 74)
        }
        76 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v77_to_v78::strip_reference(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, 76)
        }
        77 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v77_to_v78::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, 77)
        }
        75 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            super::migrations::v76_to_v77::strip_reference(&reference)?;
            super::migrations::v75_to_v76::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, 75)
        }
        78 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_v78_reference_schema(&reference)?;
            validate_schema_against(connection, &reference, 78)
        }
        79 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_current_schema_unchecked(&reference)?;
            super::migrations::v86_to_v87::strip_reference(&reference)?;
            super::migrations::v84_to_v85::strip_reference(&reference)?;
            super::migrations::v83_to_v84::strip_reference(&reference)?;
            super::migrations::v82_to_v83::strip_reference(&reference)?;
            super::migrations::v81_to_v82::strip_reference(&reference)?;
            super::migrations::v80_to_v81::strip_reference(&reference)?;
            super::migrations::v79_to_v80::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, 79)
        }
        80 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_current_schema_unchecked(&reference)?;
            super::migrations::v86_to_v87::strip_reference(&reference)?;
            super::migrations::v84_to_v85::strip_reference(&reference)?;
            super::migrations::v83_to_v84::strip_reference(&reference)?;
            super::migrations::v82_to_v83::strip_reference(&reference)?;
            super::migrations::v81_to_v82::strip_reference(&reference)?;
            super::migrations::v80_to_v81::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, 80)
        }
        81 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_current_schema_unchecked(&reference)?;
            super::migrations::v86_to_v87::strip_reference(&reference)?;
            super::migrations::v84_to_v85::strip_reference(&reference)?;
            super::migrations::v83_to_v84::strip_reference(&reference)?;
            super::migrations::v82_to_v83::strip_reference(&reference)?;
            super::migrations::v81_to_v82::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, 81)
        }
        82 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_current_schema_unchecked(&reference)?;
            super::migrations::v86_to_v87::strip_reference(&reference)?;
            super::migrations::v84_to_v85::strip_reference(&reference)?;
            super::migrations::v83_to_v84::strip_reference(&reference)?;
            super::migrations::v82_to_v83::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, 82)
        }
        83 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_current_schema_unchecked(&reference)?;
            super::migrations::v86_to_v87::strip_reference(&reference)?;
            super::migrations::v84_to_v85::strip_reference(&reference)?;
            super::migrations::v83_to_v84::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, 83)
        }
        84 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_current_schema_unchecked(&reference)?;
            super::migrations::v86_to_v87::strip_reference(&reference)?;
            super::migrations::v84_to_v85::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, 84)
        }
        85 => {
            validate_integrity(connection)?;
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_current_schema_unchecked(&reference)?;
            super::migrations::v86_to_v87::strip_reference(&reference)?;
            super::migrations::v85_to_v86::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, 85)
        }
        86 => {
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_current_schema_unchecked(&reference)?;
            super::migrations::v86_to_v87::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, 86)
        }
        87 => {
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_current_schema_unchecked(&reference)?;
            super::migrations::v87_to_v88::strip_reference(&reference)?;
            validate_schema_against(connection, &reference, 87)
        }
        88 => {
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_current_schema_unchecked(&reference)?;
            validate_schema_against(connection, &reference, 88)
        }
        89 => {
            let reference = Connection::open_in_memory().map_err(|e| e.to_string())?;
            create_current_schema_unchecked(&reference)?;
            validate_schema_against(connection, &reference, 89)
        }
        CURRENT_SCHEMA_VERSION => validate_current_database(connection),
        _ => Err(format!("unsupported Creator schema version {version}")),
    }
}

fn validate_no_shared_locator_owners(connection: &Connection) -> Result<(), String> {
    let collision = connection
        .query_row(
            "SELECT locator_id, COUNT(DISTINCT creator_id)
             FROM creator_contact_routes
             GROUP BY locator_id
             HAVING COUNT(DISTINCT creator_id) > 1
             LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some((locator_id, owner_count)) = collision {
        return Err(format!(
            "contact locator {locator_id} has {owner_count} Creator owners"
        ));
    }
    Ok(())
}

pub(super) fn validate_integrity(connection: &Connection) -> Result<(), String> {
    let integrity = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;
    if integrity != "ok" {
        return Err(format!(
            "local business data integrity check failed: {integrity}"
        ));
    }
    let foreign_key_failure = connection
        .query_row("PRAGMA foreign_key_check", [], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some((table, row_id)) = foreign_key_failure {
        return Err(format!(
            "local business data foreign key check failed at {table} row {row_id}"
        ));
    }
    Ok(())
}

fn validate_v27_schema(connection: &Connection) -> Result<(), String> {
    let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
    create_legacy_reference_schema(&reference)?;
    downgrade_reference_to_v27(&reference)?;
    validate_schema_against(connection, &reference, 27)
}

fn validate_v28_schema(connection: &Connection) -> Result<(), String> {
    let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
    create_legacy_reference_schema(&reference)?;
    downgrade_reference_to_v28(&reference)?;
    validate_schema_against(connection, &reference, 28)
}

fn create_v78_reference_schema(connection: &Connection) -> Result<(), String> {
    create_current_schema_unchecked(connection)?;
    super::migrations::v86_to_v87::strip_reference(connection)?;
    super::migrations::v84_to_v85::strip_reference(connection)?;
    super::migrations::v83_to_v84::strip_reference(connection)?;
    super::migrations::v82_to_v83::strip_reference(connection)?;
    super::migrations::v81_to_v82::strip_reference(connection)?;
    super::migrations::v80_to_v81::strip_reference(connection)?;
    super::migrations::v79_to_v80::strip_reference(connection)?;
    super::migrations::v78_to_v79::strip_reference(connection)
}

pub(super) fn validate_current_schema(connection: &Connection) -> Result<(), String> {
    let reference = Connection::open_in_memory().map_err(|error| error.to_string())?;
    create_current_schema_unchecked(&reference)?;

    validate_schema_against(connection, &reference, CURRENT_SCHEMA_VERSION)
}

/// Build an earlier reference from current DDL without changing the version:
/// frozen predecessor downgrade helpers expect CURRENT_SCHEMA_VERSION.
fn create_legacy_reference_schema(connection: &Connection) -> Result<(), String> {
    create_v78_reference_schema(connection)?;
    super::migrations::v76_to_v77::strip_reference(connection)?;
    super::migrations::v75_to_v76::strip_reference(connection)?;
    super::migrations::v74_to_v75::strip_reference(connection)?;
    super::migrations::strip_identity_reviews_reference(connection)?;
    super::migrations::strip_target_increases_reference(connection)?;
    super::migrations::strip_budget_increases_reference(connection)?;
    super::migrations::strip_video_read_failures_reference(connection)?;
    super::migrations::strip_review_reference(connection)?;
    super::migrations::strip_library_source_reference(connection)?;
    super::migrations::strip_content_identity_reference(connection)?;
    super::migrations::strip_profile_identity_reference(connection)?;
    super::migrations::strip_identity_reference(connection)?;
    super::migrations::strip_twitter_content_reference(connection)?;
    super::migrations::strip_twitter_reference(connection)?;
    super::migrations::strip_content_reference(connection)
}

fn validate_schema_against(
    connection: &Connection,
    reference: &Connection,
    version: i64,
) -> Result<(), String> {
    if version < 89 {
        super::migrations::v88_to_v89::strip_reference(reference)?;
    }
    if version < 90 {
        super::migrations::v89_to_v90::strip_reference(reference)?;
    }
    let expected_tables = load_table_names(reference)?;
    let actual_tables = load_table_names(connection)?;
    if actual_tables != expected_tables {
        let missing = expected_tables
            .iter()
            .filter(|table| !actual_tables.contains(table))
            .cloned()
            .collect::<Vec<_>>();
        let unexpected = actual_tables
            .iter()
            .filter(|table| !expected_tables.contains(table))
            .cloned()
            .collect::<Vec<_>>();
        return Err(schema_mismatch_for(
            version,
            format!(
                "table set differs (missing: {}; unexpected: {})",
                display_names(&missing),
                display_names(&unexpected)
            ),
        ));
    }

    for table in REQUIRED_TABLES {
        let actual_columns = load_columns(connection, table)?;
        if actual_columns.iter().any(|column| column.name == "lead_id") {
            return Err(schema_mismatch_for(
                version,
                format!("forbidden legacy column {table}.lead_id is present"),
            ));
        }
        if version == CURRENT_SCHEMA_VERSION
            && let Some(column) = actual_columns
                .iter()
                .find(|column| RETIRED_CURRENT_COLUMNS.contains(&column.name.as_str()))
        {
            return Err(schema_mismatch_for(
                version,
                format!("{} is not allowed on table {table}", column.name),
            ));
        }

        let expected_columns = load_columns(reference, table)?;
        if actual_columns != expected_columns {
            return Err(schema_mismatch_for(
                version,
                format!("column definition differs for table {table}"),
            ));
        }

        let expected_foreign_keys = load_foreign_keys(reference, table)?;
        let actual_foreign_keys = load_foreign_keys(connection, table)?;
        if actual_foreign_keys != expected_foreign_keys {
            return Err(schema_mismatch_for(
                version,
                format!("foreign key definition differs for table {table}"),
            ));
        }
    }

    let expected_objects = load_schema_objects(reference)?;
    let actual_objects = load_schema_objects(connection)?;
    if actual_objects != expected_objects {
        let differing_object = expected_objects
            .keys()
            .chain(actual_objects.keys())
            .find(|name| expected_objects.get(*name) != actual_objects.get(*name))
            .map(String::as_str)
            .unwrap_or("unknown");
        return Err(schema_mismatch_for(
            version,
            format!("index or structural constraint differs at schema object {differing_object}"),
        ));
    }
    Ok(())
}

fn load_table_names(connection: &Connection) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_schema
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

fn load_columns(connection: &Connection, table: &str) -> Result<Vec<ColumnDefinition>, String> {
    let sql = format!("PRAGMA table_info({})", quote_identifier(table));
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    statement
        .query_map([], |row| {
            Ok(ColumnDefinition {
                name: row.get(1)?,
                data_type: row.get(2)?,
                not_null: row.get(3)?,
                default_value: row.get(4)?,
                primary_key_position: row.get(5)?,
            })
        })
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

fn load_foreign_keys(
    connection: &Connection,
    table: &str,
) -> Result<Vec<ForeignKeyDefinition>, String> {
    let sql = format!("PRAGMA foreign_key_list({})", quote_identifier(table));
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    statement
        .query_map([], |row| {
            Ok(ForeignKeyDefinition {
                id: row.get(0)?,
                sequence: row.get(1)?,
                target_table: row.get(2)?,
                source_column: row.get(3)?,
                target_column: row.get(4)?,
                on_update: row.get(5)?,
                on_delete: row.get(6)?,
                match_clause: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

fn load_schema_objects(connection: &Connection) -> Result<BTreeMap<String, String>, String> {
    let mut statement = connection
        .prepare(
            "SELECT type || ':' || name, sql FROM sqlite_schema
             WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'
             ORDER BY type, name",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map([], |row| {
            let name = row.get::<_, String>(0)?;
            let sql = row.get::<_, String>(1)?;
            Ok((name, normalize_sql(&sql)))
        })
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

fn normalize_sql(sql: &str) -> String {
    // SQLite preserves the source spelling of `sqlite_schema.sql`.  Compare
    // tokens rather than that incidental spelling so equivalent DDL such as
    // `CHECK (x)` and `CHECK(x)` remains compatible while quoted literals are
    // kept byte-for-byte (apart from the existing ASCII case fold).
    let mut normalized = String::with_capacity(sql.len());
    let mut token = String::new();
    let mut in_quote: Option<char> = None;
    let mut quoted_token = false;
    let mut need_space = false;
    let flush =
        |normalized: &mut String, token: &mut String, need_space: &mut bool, lowercase: bool| {
            if token.is_empty() {
                return;
            }
            if *need_space && !normalized.is_empty() {
                normalized.push(' ');
            }
            if lowercase {
                normalized.push_str(&token.to_ascii_lowercase());
            } else {
                normalized.push_str(token);
            }
            token.clear();
            *need_space = true;
        };

    let chars: Vec<char> = sql.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let character = chars[index];
        if let Some(quote) = in_quote {
            token.push(character);
            if character == quote {
                if index + 1 < chars.len() && chars[index + 1] == quote {
                    token.push(chars[index + 1]);
                    index += 1;
                } else {
                    in_quote = None;
                    flush(&mut normalized, &mut token, &mut need_space, false);
                    quoted_token = false;
                }
            }
        } else if matches!(character, '\'' | '"' | '`') {
            flush(&mut normalized, &mut token, &mut need_space, true);
            in_quote = Some(character);
            quoted_token = true;
            token.push(character);
        } else if character.is_whitespace() {
            flush(&mut normalized, &mut token, &mut need_space, true);
        } else if matches!(character, '(' | ')' | ',' | ';') {
            flush(&mut normalized, &mut token, &mut need_space, true);
            normalized.push(character);
            need_space = false;
        } else {
            token.push(character);
        }
        index += 1;
    }
    flush(&mut normalized, &mut token, &mut need_space, !quoted_token);
    normalized
}

#[cfg(test)]
pub(crate) fn normalize_sql_for_test(sql: &str) -> String {
    normalize_sql(sql)
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn display_names(names: &[String]) -> String {
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

fn schema_mismatch_for(version: i64, detail: impl std::fmt::Display) -> String {
    format!(
        "Creator schema version {version} does not match its required structure: {detail}. keep the existing data unchanged and start the local service with an application version that supports its upgrade path"
    )
}

#[cfg(test)]
mod tests {
    use super::normalize_sql;

    #[test]
    fn normalize_sql_ignores_whitespace_around_structural_punctuation() {
        assert_eq!(
            normalize_sql("CHECK ( x , y )"),
            normalize_sql("CHECK(x,y)")
        );
    }

    #[test]
    fn normalize_sql_keeps_quoted_literal_whitespace() {
        assert_ne!(
            normalize_sql("CHECK(message = 'a b')"),
            normalize_sql("CHECK(message = 'ab')")
        );
        assert_ne!(
            normalize_sql("CHECK(message = 'Codex CLI')"),
            normalize_sql("CHECK(message = 'codex cli')")
        );
    }
}
