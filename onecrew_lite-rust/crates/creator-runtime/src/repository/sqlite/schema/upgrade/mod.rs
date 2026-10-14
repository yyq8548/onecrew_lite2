mod backup;
mod orchestrator;

#[cfg(test)]
pub(in crate::repository::sqlite) use backup::create_online_backup;
pub(super) use orchestrator::upgrade;
#[cfg(test)]
pub(in crate::repository::sqlite) use orchestrator::{
    upgrade_with_available_space_for_test, upgrade_with_before_lock_for_test,
    upgrade_with_invalid_backup_for_test, upgrade_with_post_commit_mutation_for_test,
    upgrade_with_post_migration_mutation_for_test,
};

use crate::{CreatorDatabaseUpgradeError, CreatorDatabaseUpgradeFailureKind};

fn upgrade_error(
    kind: CreatorDatabaseUpgradeFailureKind,
    support_code: &str,
    detail: impl std::fmt::Display,
    storage_upgraded: bool,
) -> CreatorDatabaseUpgradeError {
    eprintln!("Creator data upgrade {support_code}: {detail}");
    CreatorDatabaseUpgradeError {
        kind,
        support_code: support_code.into(),
        storage_upgraded,
    }
}
