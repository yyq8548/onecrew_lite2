//! Test-only bridge from current DDL to frozen predecessor references.
use rusqlite::Connection;

macro_rules! legacy_reference {
    ($($name:ident),+ $(,)?) => {$ (
        #[allow(dead_code, reason = "historical reference helpers are selected by individual migration contract tests")]
        pub(in crate::repository::sqlite) fn $name(connection: &Connection) -> Result<(), String> {
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
            super::migrations::strip_content_reference(connection)?;
            super::migrations::$name(connection)
        }
    )+};
}

legacy_reference!(
    downgrade_reference_to_v28,
    downgrade_reference_to_v29,
    downgrade_reference_to_v30,
    downgrade_reference_to_v31,
    downgrade_reference_to_v32,
    downgrade_reference_to_v33,
    downgrade_reference_to_v34,
    downgrade_reference_to_v35,
    downgrade_reference_to_v36,
    downgrade_reference_to_v37,
    downgrade_reference_to_v38,
    downgrade_reference_to_v39,
    downgrade_reference_to_v40,
    downgrade_reference_to_v41,
    downgrade_reference_to_v42,
);

pub(in crate::repository::sqlite) fn downgrade_reference_to_v27(
    connection: &Connection,
) -> Result<(), String> {
    // The frozen v27 preflight must reject before the new table is removed.
    if super::user_version(connection)? == super::CURRENT_SCHEMA_VERSION {
        let group_rows: u64 = connection
            .query_row("SELECT COUNT(*) FROM creator_groups", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        let assigned_rows: u64 = connection
            .query_row(
                "SELECT COUNT(*) FROM creators WHERE group_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if group_rows != 0 || assigned_rows != 0 {
            return Err("cannot build a v27 reference while v28 Creator groups exist".into());
        }
    }
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
    super::migrations::strip_content_reference(connection)?;
    super::migrations::downgrade_reference_to_v27(connection)
}

#[allow(
    dead_code,
    reason = "v43 reference is selected by migration contract tests"
)]
pub(in crate::repository::sqlite) fn downgrade_reference_to_v43(
    connection: &Connection,
) -> Result<(), String> {
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
    super::migrations::downgrade_reference_to_v43(connection)
}

#[cfg(test)]
pub(in crate::repository::sqlite) fn downgrade_reference_to_v46(
    connection: &Connection,
) -> Result<(), String> {
    super::migrations::strip_identity_reviews_reference(connection)?;
    super::migrations::strip_target_increases_reference(connection)?;
    super::migrations::strip_budget_increases_reference(connection)?;
    super::migrations::strip_video_read_failures_reference(connection)?;
    super::migrations::strip_review_reference(connection)?;
    super::migrations::strip_library_source_reference(connection)?;
    super::migrations::strip_content_identity_reference(connection)?;
    super::migrations::strip_profile_identity_reference(connection)?;
    super::migrations::strip_identity_reference(connection)?;
    connection
        .pragma_update(None, "user_version", 46)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
pub(in crate::repository::sqlite) fn strip_twitter_content_reference(
    connection: &Connection,
) -> Result<(), String> {
    super::migrations::strip_identity_reviews_reference(connection)?;
    super::migrations::strip_target_increases_reference(connection)?;
    super::migrations::strip_budget_increases_reference(connection)?;
    super::migrations::strip_video_read_failures_reference(connection)?;
    super::migrations::strip_review_reference(connection)?;
    super::migrations::strip_library_source_reference(connection)?;
    super::migrations::strip_content_identity_reference(connection)?;
    super::migrations::strip_profile_identity_reference(connection)?;
    super::migrations::strip_identity_reference(connection)?;
    super::migrations::strip_twitter_content_reference(connection)
}

#[cfg(test)]
pub(in crate::repository::sqlite) fn strip_library_source_reference(
    connection: &Connection,
) -> Result<(), String> {
    super::migrations::strip_identity_reviews_reference(connection)?;
    super::migrations::strip_target_increases_reference(connection)?;
    super::migrations::strip_budget_increases_reference(connection)?;
    super::migrations::strip_video_read_failures_reference(connection)?;
    super::migrations::strip_review_reference(connection)?;
    super::migrations::strip_library_source_reference(connection)
}

#[cfg(test)]
pub(in crate::repository::sqlite) fn restore_review_v1_reference(
    connection: &Connection,
) -> Result<(), String> {
    super::migrations::strip_identity_reviews_reference(connection)?;
    super::migrations::strip_target_increases_reference(connection)?;
    super::migrations::restore_review_v1_reference(connection)
}

#[cfg(test)]
pub(in crate::repository::sqlite) fn strip_budget_increases_reference(
    connection: &Connection,
) -> Result<(), String> {
    super::migrations::strip_identity_reviews_reference(connection)?;
    super::migrations::strip_target_increases_reference(connection)?;
    super::migrations::strip_budget_increases_reference(connection)
}
