use rusqlite::Connection;

use crate::CreatorDatabaseUpgradeImpact;

mod reference_v33;
mod reference_v34;
mod reference_v35;
mod reference_v36;
mod v27_to_v28;
mod v28_to_v29;
mod v29_to_v30;
mod v30_to_v31;
mod v31_to_v32;
mod v32_to_v33;
mod v33_to_v34;
mod v34_to_v35;
mod v35_to_v36;
mod v36_to_v37;
mod v37_to_v38;
mod v38_to_v39;
mod v39_to_v40;
mod v40_to_v41;
mod v41_to_v42;
mod v42_to_v43;
mod v43_to_v44;
mod v44_to_v45;
mod v45_to_v46;
mod v46_to_v47;
mod v47_to_v48;
mod v48_to_v49;
mod v49_to_v50;
mod v50_to_v51;
mod v51_to_v52;
mod v52_to_v53;
mod v53_to_v54;
mod v54_to_v55;
mod v55_to_v56;
mod v56_to_v57;
mod v57_to_v58;
pub(in crate::repository::sqlite) mod v58_to_v59;
pub(in crate::repository::sqlite) mod v59_to_v60;
pub(in crate::repository::sqlite) mod v60_to_v61;
pub(in crate::repository::sqlite) mod v61_to_v62;
pub(in crate::repository::sqlite) mod v62_to_v63;
pub(in crate::repository::sqlite) mod v63_to_v64;
pub(in crate::repository::sqlite) mod v64_to_v65;
pub(in crate::repository::sqlite) mod v65_to_v66;
pub(in crate::repository::sqlite) mod v66_to_v67;
pub(in crate::repository::sqlite) mod v67_to_v68;
pub(in crate::repository::sqlite) mod v68_to_v69;
pub(in crate::repository::sqlite) mod v69_to_v70;
pub(in crate::repository::sqlite) mod v70_to_v71;
pub(in crate::repository::sqlite) mod v71_to_v72;
pub(in crate::repository::sqlite) mod v72_to_v73;
pub(in crate::repository::sqlite) mod v73_to_v74;
pub(in crate::repository::sqlite) mod v74_to_v75;
pub(in crate::repository::sqlite) mod v75_to_v76;
pub(in crate::repository::sqlite) mod v76_to_v77;
pub(in crate::repository::sqlite) mod v77_to_v78;
pub(in crate::repository::sqlite) mod v78_to_v79;
pub(in crate::repository::sqlite) mod v79_to_v80;
pub(in crate::repository::sqlite) mod v80_to_v81;
pub(in crate::repository::sqlite) mod v81_to_v82;
pub(in crate::repository::sqlite) mod v82_to_v83;
pub(in crate::repository::sqlite) mod v83_to_v84;
pub(in crate::repository::sqlite) mod v84_to_v85;
pub(in crate::repository::sqlite) mod v85_to_v86;
pub(in crate::repository::sqlite) mod v86_to_v87;
pub(in crate::repository::sqlite) mod v87_to_v88;
pub(in crate::repository::sqlite) mod v88_to_v89;
pub(in crate::repository::sqlite) mod v89_to_v90;
mod validation;

pub(in crate::repository::sqlite) use v43_to_v44::{
    downgrade_reference_to_v43, strip_content_reference,
};
pub(in crate::repository::sqlite) use v44_to_v45::strip_twitter_reference;
pub(in crate::repository::sqlite) use v45_to_v46::strip_twitter_content_reference;
pub(in crate::repository::sqlite) use v46_to_v47::strip_identity_reference;
pub(in crate::repository::sqlite) use v47_to_v48::strip_profile_identity_reference;
pub(in crate::repository::sqlite) use v48_to_v49::strip_content_identity_reference;
pub(in crate::repository::sqlite) use v49_to_v50::strip_library_source_reference;
pub(in crate::repository::sqlite) use v50_to_v51::strip_review_reference;
pub(in crate::repository::sqlite) use v51_to_v52::strip_video_read_failures_reference;
pub(in crate::repository::sqlite) use v52_to_v53::strip_budget_increases_reference;
pub(in crate::repository::sqlite) use v53_to_v54::restore_review_v1_reference;
pub(in crate::repository::sqlite) use v54_to_v55::strip_target_increases_reference;
#[cfg(test)]
pub(in crate::repository::sqlite) use v55_to_v56::strip_identity_reviews_reference as oracle_strip_identity_reviews;
#[cfg(test)]
pub(in crate::repository::sqlite) use v56_to_v57::strip_library_identity_versions_reference as oracle_strip_library_identity_versions;
pub(in crate::repository::sqlite) fn strip_library_identity_versions_reference(
    connection: &Connection,
) -> Result<(), String> {
    v86_to_v87::strip_reference(connection)?;
    v85_to_v86::strip_reference(connection)?;
    v84_to_v85::strip_reference(connection)?;
    v83_to_v84::strip_reference(connection)?;
    v82_to_v83::strip_reference(connection)?;
    v81_to_v82::strip_reference(connection)?;
    v80_to_v81::strip_reference(connection)?;
    v79_to_v80::strip_reference(connection)?;
    v78_to_v79::strip_reference(connection)?;
    v76_to_v77::strip_reference(connection)?;
    v75_to_v76::strip_reference(connection)?;
    v74_to_v75::strip_reference(connection)?;
    v73_to_v74::strip_reference(connection)?;
    v72_to_v73::strip_reference(connection)?;
    v71_to_v72::strip_reference(connection)?;
    v70_to_v71::strip_reference(connection)?;
    v69_to_v70::strip_reference(connection)?;
    v68_to_v69::strip_reference(connection)?;
    v67_to_v68::strip_reference(connection)?;
    v66_to_v67::strip_reference(connection)?;
    v65_to_v66::strip_reference(connection)?;
    v64_to_v65::strip_reference(connection)?;
    v63_to_v64::strip_reference(connection)?;
    v62_to_v63::strip_reference(connection)?;
    v61_to_v62::strip_reference(connection)?;
    v59_to_v60::strip_reference(connection)?;
    v56_to_v57::strip_library_identity_versions_reference(connection)
}

pub(in crate::repository::sqlite) fn strip_identity_reviews_reference(
    connection: &rusqlite::Connection,
) -> Result<(), String> {
    strip_library_identity_versions_reference(connection)?;
    v55_to_v56::strip_identity_reviews_reference(connection)
}

pub(in crate::repository::sqlite) use reference_v33::downgrade_reference_to_v33;
pub(in crate::repository::sqlite) use reference_v34::downgrade_reference_to_v34;
pub(in crate::repository::sqlite) use reference_v36::downgrade_reference_to_v36;
pub(in crate::repository::sqlite) use v42_to_v43::{
    downgrade_reference_to_v37, downgrade_reference_to_v38, downgrade_reference_to_v39,
    downgrade_reference_to_v40, downgrade_reference_to_v41, downgrade_reference_to_v42,
};

pub(in crate::repository::sqlite) use validation::{
    downgrade_reference_to_v27, downgrade_reference_to_v28, downgrade_reference_to_v29,
    downgrade_reference_to_v30, downgrade_reference_to_v31, downgrade_reference_to_v32,
    downgrade_reference_to_v35,
};

#[derive(Clone, Copy)]
struct ForwardMigration {
    from_version: i64,
    to_version: i64,
    apply: fn(&Connection) -> Result<(), String>,
}

const FORWARD_MIGRATIONS: &[ForwardMigration] = &[
    ForwardMigration {
        from_version: 27,
        to_version: 28,
        apply: v27_to_v28::migrate,
    },
    ForwardMigration {
        from_version: 28,
        to_version: 29,
        apply: v28_to_v29::migrate,
    },
    ForwardMigration {
        from_version: 29,
        to_version: 30,
        apply: v29_to_v30::migrate,
    },
    ForwardMigration {
        from_version: 30,
        to_version: 31,
        apply: v30_to_v31::migrate,
    },
    ForwardMigration {
        from_version: 31,
        to_version: 32,
        apply: v31_to_v32::migrate,
    },
    ForwardMigration {
        from_version: 32,
        to_version: 33,
        apply: v32_to_v33::migrate,
    },
    ForwardMigration {
        from_version: 33,
        to_version: 34,
        apply: v33_to_v34::migrate,
    },
    ForwardMigration {
        from_version: 34,
        to_version: 35,
        apply: v34_to_v35::migrate,
    },
    ForwardMigration {
        from_version: 35,
        to_version: 36,
        apply: v35_to_v36::migrate,
    },
    ForwardMigration {
        from_version: 36,
        to_version: 37,
        apply: v36_to_v37::migrate,
    },
    ForwardMigration {
        from_version: 37,
        to_version: 38,
        apply: v37_to_v38::migrate,
    },
    ForwardMigration {
        from_version: 38,
        to_version: 39,
        apply: v38_to_v39::migrate,
    },
    ForwardMigration {
        from_version: 39,
        to_version: 40,
        apply: v39_to_v40::migrate,
    },
    ForwardMigration {
        from_version: 40,
        to_version: 41,
        apply: v40_to_v41::migrate,
    },
    ForwardMigration {
        from_version: 41,
        to_version: 42,
        apply: v41_to_v42::migrate,
    },
    ForwardMigration {
        from_version: 42,
        to_version: 43,
        apply: v42_to_v43::migrate,
    },
    ForwardMigration {
        from_version: 43,
        to_version: 44,
        apply: v43_to_v44::migrate,
    },
    ForwardMigration {
        from_version: 44,
        to_version: 45,
        apply: v44_to_v45::migrate,
    },
    ForwardMigration {
        from_version: 45,
        to_version: 46,
        apply: v45_to_v46::migrate,
    },
    ForwardMigration {
        from_version: 46,
        to_version: 47,
        apply: v46_to_v47::migrate,
    },
    ForwardMigration {
        from_version: 47,
        to_version: 48,
        apply: v47_to_v48::migrate,
    },
    ForwardMigration {
        from_version: 48,
        to_version: 49,
        apply: v48_to_v49::migrate,
    },
    ForwardMigration {
        from_version: 49,
        to_version: 50,
        apply: v49_to_v50::migrate,
    },
    ForwardMigration {
        from_version: 50,
        to_version: 51,
        apply: v50_to_v51::migrate,
    },
    ForwardMigration {
        from_version: 51,
        to_version: 52,
        apply: v51_to_v52::migrate,
    },
    ForwardMigration {
        from_version: 52,
        to_version: 53,
        apply: v52_to_v53::migrate,
    },
    ForwardMigration {
        from_version: 53,
        to_version: 54,
        apply: v53_to_v54::migrate,
    },
    ForwardMigration {
        from_version: 54,
        to_version: 55,
        apply: v54_to_v55::migrate,
    },
    ForwardMigration {
        from_version: 55,
        to_version: 56,
        apply: v55_to_v56::migrate,
    },
    ForwardMigration {
        from_version: 56,
        to_version: 57,
        apply: v56_to_v57::migrate,
    },
    ForwardMigration {
        from_version: 57,
        to_version: 58,
        apply: v57_to_v58::migrate,
    },
    ForwardMigration {
        from_version: 58,
        to_version: 59,
        apply: v58_to_v59::migrate,
    },
    ForwardMigration {
        from_version: 59,
        to_version: 60,
        apply: v59_to_v60::migrate,
    },
    ForwardMigration {
        from_version: 60,
        to_version: 61,
        apply: v60_to_v61::migrate,
    },
    ForwardMigration {
        from_version: 61,
        to_version: 62,
        apply: v61_to_v62::migrate,
    },
    ForwardMigration {
        from_version: 62,
        to_version: 63,
        apply: v62_to_v63::migrate,
    },
    ForwardMigration {
        from_version: 63,
        to_version: 64,
        apply: v63_to_v64::migrate,
    },
    ForwardMigration {
        from_version: 64,
        to_version: 65,
        apply: v64_to_v65::migrate,
    },
    ForwardMigration {
        from_version: 65,
        to_version: 66,
        apply: v65_to_v66::migrate,
    },
    ForwardMigration {
        from_version: 66,
        to_version: 67,
        apply: v66_to_v67::migrate,
    },
    ForwardMigration {
        from_version: 67,
        to_version: 68,
        apply: v67_to_v68::migrate,
    },
    ForwardMigration {
        from_version: 68,
        to_version: 69,
        apply: v68_to_v69::migrate,
    },
    ForwardMigration {
        from_version: 69,
        to_version: 70,
        apply: v69_to_v70::migrate,
    },
    ForwardMigration {
        from_version: 70,
        to_version: 71,
        apply: v70_to_v71::migrate,
    },
    ForwardMigration {
        from_version: 71,
        to_version: 72,
        apply: v71_to_v72::migrate,
    },
    ForwardMigration {
        from_version: 72,
        to_version: 73,
        apply: v72_to_v73::migrate,
    },
    ForwardMigration {
        from_version: 73,
        to_version: 74,
        apply: v73_to_v74::migrate,
    },
    ForwardMigration {
        from_version: 74,
        to_version: 75,
        apply: v74_to_v75::migrate,
    },
    ForwardMigration {
        from_version: 75,
        to_version: 76,
        apply: v75_to_v76::migrate,
    },
    ForwardMigration {
        from_version: 76,
        to_version: 77,
        apply: v76_to_v77::migrate,
    },
    ForwardMigration {
        from_version: 77,
        to_version: 78,
        apply: v77_to_v78::migrate,
    },
    ForwardMigration {
        from_version: 78,
        to_version: 79,
        apply: v78_to_v79::migrate,
    },
    ForwardMigration {
        from_version: 79,
        to_version: 80,
        apply: v79_to_v80::migrate,
    },
    ForwardMigration {
        from_version: 80,
        to_version: 81,
        apply: v80_to_v81::migrate,
    },
    ForwardMigration {
        from_version: 81,
        to_version: 82,
        apply: v81_to_v82::migrate,
    },
    ForwardMigration {
        from_version: 82,
        to_version: 83,
        apply: v82_to_v83::migrate,
    },
    ForwardMigration {
        from_version: 83,
        to_version: 84,
        apply: v83_to_v84::migrate,
    },
    ForwardMigration {
        from_version: 84,
        to_version: 85,
        apply: v84_to_v85::migrate,
    },
    ForwardMigration {
        from_version: 85,
        to_version: 86,
        apply: v85_to_v86::migrate,
    },
    ForwardMigration { from_version: 86, to_version: 87, apply: v86_to_v87::migrate },
    ForwardMigration { from_version: 87, to_version: 88, apply: v87_to_v88::migrate },
    ForwardMigration { from_version: 88, to_version: 89, apply: v88_to_v89::migrate },
    ForwardMigration { from_version: 89, to_version: 90, apply: v89_to_v90::migrate },
];

/// Count the full chain's typed impact read-only on the source: the Task
/// retirement (edges from 33), the released-payload checks of the tags edge
/// (from 34), and the Campaign retirement (every supported source). Each edge
/// reports only the rows its own transform removes or converts, so the locked
/// revalidation can compare the whole struct for equality.
pub(in crate::repository::sqlite::schema) fn collect_chain_migration_impact(
    connection: &Connection,
    from_version: i64,
) -> Result<CreatorDatabaseUpgradeImpact, MigrationPreflightError> {
    if from_version <= 34 {
        v34_to_v35::validate_source_payloads(connection)?;
    }
    let mut impact = if from_version <= 33 {
        v33_to_v34::impact(connection)?
    } else {
        CreatorDatabaseUpgradeImpact::default()
    };
    if from_version <= 35 {
        v35_to_v36::preflight(connection, &mut impact)?;
    }
    if from_version <= 36 {
        v36_to_v37::preflight(connection, from_version, &mut impact)?;
    }
    Ok(impact)
}

#[derive(Debug)]
pub(in crate::repository::sqlite::schema) enum MigrationPreflightError {
    UnsafeData(String),
    Blocked(crate::CreatorDatabaseUpgradeBlocker),
}
impl From<String> for MigrationPreflightError {
    fn from(error: String) -> Self {
        Self::UnsafeData(error)
    }
}
impl std::fmt::Display for MigrationPreflightError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsafeData(error) => formatter.write_str(error),
            Self::Blocked(blocker) => formatter.write_str(blocker.safe_message()),
        }
    }
}

pub(in crate::repository::sqlite) fn apply_forward_migrations(
    connection: &Connection,
    from_version: i64,
    to_version: i64,
) -> Result<(), String> {
    let mut version = from_version;
    while version < to_version {
        let step = FORWARD_MIGRATIONS
            .iter()
            .find(|migration| migration.from_version == version)
            .ok_or_else(|| format!("no Creator migration registered from version {version}"))?;
        if step.to_version != version + 1 || step.to_version > to_version {
            return Err(format!(
                "invalid Creator migration registry step: {} -> {}",
                step.from_version, step.to_version
            ));
        }
        (step.apply)(connection)?;
        version = step.to_version;
    }
    if version != to_version {
        return Err(format!(
            "Creator migration registry did not reach requested version {to_version}"
        ));
    }
    Ok(())
}
