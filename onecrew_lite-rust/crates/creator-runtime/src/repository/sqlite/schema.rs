use std::path::Path;

use rusqlite::Connection;

use crate::{CreatorDatabaseStatus, CreatorDatabaseUpgradeError, CreatorDatabaseUpgradeResult};

mod current;
mod inspection;
mod migrations;
#[cfg(test)]
pub(in crate::repository::sqlite) fn strip_instagram_reference(
    connection: &Connection,
) -> Result<(), String> {
    migrations::v84_to_v85::strip_reference(connection)?;
    migrations::v83_to_v84::strip_reference(connection)?;
    migrations::v82_to_v83::strip_reference(connection)?;
    migrations::v81_to_v82::strip_reference(connection)?;
    migrations::v80_to_v81::strip_reference(connection)?;
    migrations::v79_to_v80::strip_reference(connection)?;
    migrations::v78_to_v79::strip_reference(connection)?;
    migrations::v76_to_v77::strip_reference(connection)?;
    migrations::v75_to_v76::strip_reference(connection)?;
    migrations::v74_to_v75::strip_reference(connection)?;
    migrations::v73_to_v74::strip_reference(connection)?;
    migrations::v72_to_v73::strip_reference(connection)?;
    migrations::v71_to_v72::strip_reference(connection)?;
    migrations::v70_to_v71::strip_reference(connection)?;
    migrations::v69_to_v70::strip_reference(connection)?;
    migrations::v68_to_v69::strip_reference(connection)?;
    migrations::v67_to_v68::strip_reference(connection)?;
    migrations::v66_to_v67::strip_reference(connection)
}
#[cfg(test)]
pub(in crate::repository::sqlite) fn strip_contacts_reference(
    connection: &Connection,
) -> Result<(), String> {
    strip_instagram_reference(connection)?;
    migrations::v65_to_v66::strip_reference(connection)?;
    migrations::v64_to_v65::strip_reference(connection)
}
#[cfg(test)]
pub(in crate::repository::sqlite) fn strip_facebook_reference(
    connection: &Connection,
) -> Result<(), String> {
    strip_instagram_reference(connection)?;
    migrations::v65_to_v66::strip_reference(connection)?;
    migrations::v64_to_v65::strip_reference(connection)?;
    migrations::v63_to_v64::strip_reference(connection)?;
    migrations::v62_to_v63::strip_reference(connection)
}
#[cfg(test)]
pub(in crate::repository::sqlite) fn strip_reddit_reference(
    connection: &Connection,
) -> Result<(), String> {
    strip_instagram_reference(connection)?;
    migrations::v65_to_v66::strip_reference(connection)?;
    migrations::v64_to_v65::strip_reference(connection)?;
    migrations::v63_to_v64::strip_reference(connection)?;
    migrations::v62_to_v63::strip_reference(connection)?;
    migrations::v61_to_v62::strip_reference(connection)
}
#[cfg(test)]
pub(in crate::repository::sqlite) fn strip_target_increases_reference(
    connection: &Connection,
) -> Result<(), String> {
    migrations::strip_identity_reviews_reference(connection)?;
    migrations::strip_target_increases_reference(connection)
}
#[cfg(test)]
pub(in crate::repository::sqlite) use migrations::strip_identity_reviews_reference;
#[cfg(test)]
pub(in crate::repository::sqlite) use migrations::strip_library_identity_versions_reference;
#[cfg(any(test, feature = "test-support"))]
mod reference_compat;
mod upgrade;
mod validation;

#[cfg(test)]
pub(super) use current::NO_SHARED_CONTACT_ROUTE_TRIGGERS_SQL;
use current::create_current_schema_unchecked;
use inspection::EARLIEST_MIGRATABLE_SCHEMA_VERSION;
pub(in crate::repository::sqlite::schema) use inspection::user_version;
pub(super) use migrations::apply_forward_migrations;
use migrations::collect_chain_migration_impact;
#[cfg(test)]
pub(in crate::repository::sqlite) use migrations::strip_review_reference;
#[cfg(test)]
pub(in crate::repository::sqlite) use migrations::strip_twitter_reference;
#[cfg(test)]
pub(in crate::repository::sqlite) use migrations::strip_video_read_failures_reference;
#[cfg(any(test, feature = "test-support"))]
#[allow(
    unused_imports,
    reason = "the v43 reference helper is used by the Creator migration contract tests"
)]
pub(in crate::repository::sqlite) use reference_compat::downgrade_reference_to_v43;
#[cfg(test)]
pub(in crate::repository::sqlite) use reference_compat::restore_review_v1_reference;
#[cfg(test)]
pub(in crate::repository::sqlite) use reference_compat::strip_budget_increases_reference;
#[cfg(test)]
pub(in crate::repository::sqlite) use reference_compat::strip_library_source_reference;
#[cfg(any(test, feature = "test-support"))]
#[allow(
    unused_imports,
    reason = "the v30 reference helper is consumed by migration contract tests"
)]
pub(in crate::repository::sqlite) use reference_compat::{
    downgrade_reference_to_v27, downgrade_reference_to_v28, downgrade_reference_to_v29,
    downgrade_reference_to_v30, downgrade_reference_to_v31, downgrade_reference_to_v32,
    downgrade_reference_to_v33, downgrade_reference_to_v34, downgrade_reference_to_v35,
    downgrade_reference_to_v36, downgrade_reference_to_v37, downgrade_reference_to_v38,
    downgrade_reference_to_v39, downgrade_reference_to_v40, downgrade_reference_to_v41,
    downgrade_reference_to_v42,
};
#[cfg(test)]
pub(in crate::repository::sqlite) use reference_compat::{
    downgrade_reference_to_v46, strip_twitter_content_reference,
};
#[cfg(test)]
pub(super) use upgrade::{
    create_online_backup, upgrade_with_available_space_for_test, upgrade_with_before_lock_for_test,
    upgrade_with_invalid_backup_for_test, upgrade_with_post_commit_mutation_for_test,
    upgrade_with_post_migration_mutation_for_test,
};
#[cfg(test)]
pub(super) use validation::normalize_sql_for_test;
pub(in crate::repository::sqlite::schema) use validation::{
    validate_current_database, validate_database_for_version, validate_integrity,
};

pub const CURRENT_SCHEMA_VERSION: i64 = 90;

pub(super) fn initialize(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .map_err(|error| error.to_string())?;

    let user_version = connection
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .map_err(|error| error.to_string())?;
    let table_count = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?;

    if table_count == 0 && user_version == 0 {
        create_current_schema(connection)?;
        return Ok(());
    }

    match user_version {
        CURRENT_SCHEMA_VERSION => validate_current_database(connection),
        version
            if (EARLIEST_MIGRATABLE_SCHEMA_VERSION..CURRENT_SCHEMA_VERSION).contains(&version) =>
        {
            validate_database_for_version(connection, user_version).map_err(|error| {
                format!(
                    "Creator database cannot be safely upgraded from version {user_version}: {error}"
                )
            })?;
            Err(format!(
                "Creator database upgrade required: {user_version} -> {CURRENT_SCHEMA_VERSION}; start the local service to inspect, back up, and upgrade"
            ))
        }
        _ => Err(format!(
            "unsupported Creator schema version {user_version}; expected {CURRENT_SCHEMA_VERSION}. keep the existing data unchanged and use an application version with a supported upgrade path"
        )),
    }
}

fn create_current_schema(connection: &Connection) -> Result<(), String> {
    create_current_schema_unchecked(connection)?;
    validate_current_database(connection)
}

pub(super) fn upgrade(
    path: &Path,
) -> Result<CreatorDatabaseUpgradeResult, CreatorDatabaseUpgradeError> {
    upgrade::upgrade(path)
}

pub(super) fn inspect(path: &Path) -> CreatorDatabaseStatus {
    inspection::inspect(path)
}
