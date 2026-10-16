use std::{
    fmt,
    fs::{self, OpenOptions},
    path::Path,
};

use fslock::LockFile;
use onecrew_creator_runtime::{
    CreatorDatabaseStatus, CreatorDatabaseUpgradeFailureKind, CreatorDatabaseUpgradeImpact,
    CreatorRuntime,
};
use onecrew_local_domain::Platform;
use onecrew_local_vault::{
    LocalVault, VaultDataStatus, VaultDataUpgradeFailureKind, VaultDataUpgradeStatus,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const UPGRADE_TITLE: &str = "需要升级本地数据";
pub const UPGRADE_MESSAGE: &str = "新版本更新了本地数据格式。升级前会自动创建可恢复备份；本地业务记录及远端邮件或私信不会删除。升级期间相关功能将暂时不可用，请勿退出应用。";
pub const UPGRADE_PRIMARY_ACTION: &str = "备份并升级";
pub const UPGRADE_SECONDARY_ACTION: &str = "退出应用";
pub const UPGRADE_SUCCESS_MESSAGE: &str = "升级完成。可恢复备份已保留；远端邮件或私信未被删除。";
pub const UPGRADE_FAILURE_MESSAGE: &str = "升级未完成，原有数据未被替换。你可以安全重试。";
pub const UPGRADE_APPLIED_INITIALIZATION_FAILED_MESSAGE: &str =
    "本地数据已升级，但相关功能尚未恢复。请重新初始化；如仍失败，请联系支持。";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocalDataState {
    Fresh,
    Ready,
    UpgradeRequired,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocalDataStore {
    Account,
    Business,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocalDataFailureKind {
    InsufficientSpace,
    Busy,
    BackupFailed,
    NewerVersion,
    NotWritable,
    InitializationFailed,
    UnsafeData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalDataUpgradeTarget {
    pub store: LocalDataStore,
    pub from_version: i64,
    pub to_version: i64,
    pub impact: Option<CreatorDatabaseUpgradeImpact>,
}

impl LocalDataUpgradeTarget {
    fn account(from_version: i64, to_version: i64) -> Self {
        Self {
            store: LocalDataStore::Account,
            from_version,
            to_version,
            impact: None,
        }
    }

    fn business(from_version: i64, to_version: i64, impact: CreatorDatabaseUpgradeImpact) -> Self {
        Self {
            store: LocalDataStore::Business,
            from_version,
            to_version,
            impact: Some(impact),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalDataPreflight {
    pub state: LocalDataState,
    pub targets: Vec<LocalDataUpgradeTarget>,
    pub recoverable_backup: bool,
    pub remote_data_deleted: bool,
    pub blocked_store: Option<LocalDataStore>,
    pub title: Option<String>,
    pub message: Option<String>,
    pub primary_action: Option<String>,
    pub secondary_action: Option<String>,
    pub failure_kind: Option<LocalDataFailureKind>,
    pub support_code: Option<String>,
}

impl LocalDataPreflight {
    fn plain(state: LocalDataState) -> Self {
        Self {
            state,
            targets: Vec::new(),
            recoverable_backup: false,
            remote_data_deleted: false,
            blocked_store: None,
            title: None,
            message: None,
            primary_action: None,
            secondary_action: None,
            failure_kind: None,
            support_code: None,
        }
    }

    fn upgrade_required(targets: Vec<LocalDataUpgradeTarget>) -> Self {
        debug_assert!(!targets.is_empty());
        Self {
            state: LocalDataState::UpgradeRequired,
            targets,
            recoverable_backup: true,
            remote_data_deleted: false,
            blocked_store: None,
            title: Some(UPGRADE_TITLE.into()),
            message: Some(UPGRADE_MESSAGE.into()),
            primary_action: Some(UPGRADE_PRIMARY_ACTION.into()),
            secondary_action: Some(UPGRADE_SECONDARY_ACTION.into()),
            failure_kind: None,
            support_code: None,
        }
    }

    fn blocked(
        store: Option<LocalDataStore>,
        kind: LocalDataFailureKind,
        support_code: &str,
    ) -> Self {
        Self {
            state: LocalDataState::Blocked,
            targets: Vec::new(),
            recoverable_backup: false,
            remote_data_deleted: false,
            blocked_store: store,
            title: Some(UPGRADE_TITLE.into()),
            message: Some(blocked_message(kind).into()),
            primary_action: None,
            secondary_action: Some(UPGRADE_SECONDARY_ACTION.into()),
            failure_kind: Some(kind),
            support_code: Some(support_code.into()),
        }
    }

    pub fn initialization_blocked() -> Self {
        Self::blocked(
            None,
            LocalDataFailureKind::InitializationFailed,
            "LOCAL_DATA_INITIALIZATION_FAILED",
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocalDataUpgradeStatus {
    Completed,
    AlreadyCurrent,
    UpgradeAppliedButInitializationFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocalDataStoreUpgradeStatus {
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalDataStoreUpgradeResult {
    pub store: LocalDataStore,
    pub status: LocalDataStoreUpgradeStatus,
    pub from_version: i64,
    pub to_version: i64,
    pub recovery_point_created: bool,
    pub impact: Option<CreatorDatabaseUpgradeImpact>,
}

impl LocalDataStoreUpgradeResult {
    fn completed(target: &LocalDataUpgradeTarget, recovery_point_created: bool) -> Self {
        Self {
            store: target.store,
            status: LocalDataStoreUpgradeStatus::Completed,
            from_version: target.from_version,
            to_version: target.to_version,
            recovery_point_created,
            impact: target.impact.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalDataUpgradeResult {
    pub status: LocalDataUpgradeStatus,
    pub message: String,
    pub support_code: Option<String>,
    pub results: Vec<LocalDataStoreUpgradeResult>,
    pub remote_data_deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalDataUpgradeError {
    pub kind: LocalDataFailureKind,
    pub message: String,
    pub support_code: String,
    pub storage_upgraded: bool,
    pub results: Vec<LocalDataStoreUpgradeResult>,
}

impl fmt::Display for LocalDataUpgradeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} ({})", self.message, self.support_code)
    }
}

impl std::error::Error for LocalDataUpgradeError {}

pub(crate) fn inspect_data_dir(data_dir: &Path) -> LocalDataPreflight {
    if let Err(support_code) = validate_data_dir_permissions(data_dir) {
        return LocalDataPreflight::blocked(None, LocalDataFailureKind::NotWritable, support_code);
    }
    let vault = LocalVault::inspect(data_dir.join("agent-platform.db"));
    let creators = CreatorRuntime::inspect_database(data_dir.join("crm.db"));

    if let VaultDataStatus::Unsupported {
        created_by_newer_version,
        reason,
    } = &vault.status
    {
        eprintln!("local account preflight failed: {reason}");
        return LocalDataPreflight::blocked(
            Some(LocalDataStore::Account),
            if *created_by_newer_version {
                LocalDataFailureKind::NewerVersion
            } else {
                LocalDataFailureKind::UnsafeData
            },
            if *created_by_newer_version {
                "LOCAL_DATA_NEWER_FORMAT"
            } else {
                "LOCAL_DATA_ACCOUNT_UNSAFE"
            },
        );
    }
    if let CreatorDatabaseStatus::Blocked { blocker, .. } = &creators {
        return LocalDataPreflight::blocked(
            Some(LocalDataStore::Business),
            LocalDataFailureKind::UnsafeData,
            blocker.support_code(),
        );
    }
    if let CreatorDatabaseStatus::Unsupported { version, reason } = &creators {
        eprintln!("local business preflight failed at version {version}: {reason}");
        return LocalDataPreflight::blocked(
            Some(LocalDataStore::Business),
            if *version > onecrew_creator_runtime::CREATOR_SCHEMA_VERSION {
                LocalDataFailureKind::NewerVersion
            } else {
                LocalDataFailureKind::UnsafeData
            },
            if *version > onecrew_creator_runtime::CREATOR_SCHEMA_VERSION {
                "LOCAL_DATA_NEWER_FORMAT"
            } else {
                "LOCAL_DATA_BUSINESS_UNSAFE"
            },
        );
    }
    if let Err(error) = validate_profile_directories(data_dir, &vault.profile_directories) {
        eprintln!("local profile preflight failed: {error}");
        return LocalDataPreflight::blocked(
            Some(LocalDataStore::Account),
            LocalDataFailureKind::UnsafeData,
            "LOCAL_DATA_PROFILE_UNSAFE",
        );
    }

    match (&vault.status, creators) {
        (VaultDataStatus::Fresh, CreatorDatabaseStatus::Fresh) => {
            LocalDataPreflight::plain(LocalDataState::Fresh)
        }
        (
            VaultDataStatus::Ready,
            CreatorDatabaseStatus::UpgradeRequired {
                from_version,
                to_version,
                impact,
            },
        ) => LocalDataPreflight::upgrade_required(vec![LocalDataUpgradeTarget::business(
            from_version,
            to_version,
            impact,
        )]),
        (
            VaultDataStatus::UpgradeRequired {
                from_version,
                to_version,
            },
            CreatorDatabaseStatus::Fresh | CreatorDatabaseStatus::Ready,
        ) => LocalDataPreflight::upgrade_required(vec![LocalDataUpgradeTarget::account(
            *from_version,
            *to_version,
        )]),
        (
            VaultDataStatus::UpgradeRequired {
                from_version: account_from,
                to_version: account_to,
            },
            CreatorDatabaseStatus::UpgradeRequired {
                from_version: business_from,
                to_version: business_to,
                impact,
            },
        ) => LocalDataPreflight::upgrade_required(vec![
            LocalDataUpgradeTarget::account(*account_from, *account_to),
            LocalDataUpgradeTarget::business(business_from, business_to, impact),
        ]),
        (VaultDataStatus::Fresh, CreatorDatabaseStatus::UpgradeRequired { .. }) => {
            LocalDataPreflight::blocked(
                Some(LocalDataStore::Account),
                LocalDataFailureKind::UnsafeData,
                "LOCAL_DATA_INCOMPLETE",
            )
        }
        (VaultDataStatus::Fresh, CreatorDatabaseStatus::Ready) => LocalDataPreflight::blocked(
            Some(LocalDataStore::Account),
            LocalDataFailureKind::UnsafeData,
            "LOCAL_DATA_INCOMPLETE",
        ),
        (VaultDataStatus::Ready, CreatorDatabaseStatus::Fresh | CreatorDatabaseStatus::Ready) => {
            LocalDataPreflight::plain(LocalDataState::Ready)
        }
        (VaultDataStatus::Unsupported { .. }, _)
        | (_, CreatorDatabaseStatus::Unsupported { .. } | CreatorDatabaseStatus::Blocked { .. }) => {
            unreachable!("handled above")
        }
    }
}

pub(crate) fn upgrade_data_dir(
    data_dir: &Path,
) -> Result<LocalDataUpgradeResult, LocalDataUpgradeError> {
    let mut lock = acquire_data_dir_lock(data_dir)?;
    let result = upgrade_data_dir_locked(data_dir);
    if let Err(error) = lock.unlock() {
        eprintln!("local data lifecycle lock release failed: {error}");
    }
    result
}

fn upgrade_data_dir_locked(
    data_dir: &Path,
) -> Result<LocalDataUpgradeResult, LocalDataUpgradeError> {
    upgrade_data_dir_locked_with_post_upgrade_hook(data_dir, || {})
}

fn upgrade_data_dir_locked_with_post_upgrade_hook(
    data_dir: &Path,
    post_upgrade_hook: impl FnOnce(),
) -> Result<LocalDataUpgradeResult, LocalDataUpgradeError> {
    upgrade_data_dir_locked_with_hooks(data_dir, |_| {}, post_upgrade_hook)
}

fn upgrade_data_dir_locked_with_hooks(
    data_dir: &Path,
    mut after_store_commit: impl FnMut(LocalDataStore),
    post_upgrade_hook: impl FnOnce(),
) -> Result<LocalDataUpgradeResult, LocalDataUpgradeError> {
    let preflight = inspect_data_dir(data_dir);
    match preflight.state {
        LocalDataState::Ready => {
            return Ok(LocalDataUpgradeResult {
                status: LocalDataUpgradeStatus::AlreadyCurrent,
                message: UPGRADE_SUCCESS_MESSAGE.into(),
                support_code: None,
                results: Vec::new(),
                remote_data_deleted: false,
            });
        }
        LocalDataState::UpgradeRequired => {}
        _ => {
            return Err(LocalDataUpgradeError {
                kind: preflight
                    .failure_kind
                    .unwrap_or(LocalDataFailureKind::UnsafeData),
                message: blocked_message(
                    preflight
                        .failure_kind
                        .unwrap_or(LocalDataFailureKind::UnsafeData),
                )
                .into(),
                support_code: preflight
                    .support_code
                    .unwrap_or_else(|| "LOCAL_DATA_NOT_UPGRADABLE".into()),
                storage_upgraded: false,
                results: Vec::new(),
            });
        }
    }
    probe_data_dir_writable(data_dir)?;
    let mut results = Vec::with_capacity(preflight.targets.len());
    for target in &preflight.targets {
        let result = match target.store {
            LocalDataStore::Account => {
                let upgraded = LocalVault::upgrade(data_dir.join("agent-platform.db"))
                    .map_err(|error| account_upgrade_error(error, target, results.clone()))?;
                if upgraded.status != VaultDataUpgradeStatus::Completed
                    || upgraded.from_version != Some(target.from_version)
                    || upgraded.to_version != target.to_version
                    || upgraded.backup_path.is_none()
                {
                    if upgraded.status == VaultDataUpgradeStatus::Completed {
                        results.push(LocalDataStoreUpgradeResult::completed(
                            target,
                            upgraded.backup_path.is_some(),
                        ));
                    }
                    return Err(LocalDataUpgradeError {
                        kind: LocalDataFailureKind::UnsafeData,
                        message: UPGRADE_APPLIED_INITIALIZATION_FAILED_MESSAGE.into(),
                        support_code: "LOCAL_DATA_ACCOUNT_RESULT_MISMATCH".into(),
                        storage_upgraded: !results.is_empty(),
                        results,
                    });
                }
                LocalDataStoreUpgradeResult::completed(target, true)
            }
            LocalDataStore::Business => {
                let upgraded = CreatorRuntime::upgrade_database(data_dir.join("crm.db"))
                    .map_err(|error| business_upgrade_error(error, target, results.clone()))?;
                let upgraded_impact: CreatorDatabaseUpgradeImpact = upgraded.impact.clone();
                if upgraded.from_version != target.from_version
                    || upgraded.to_version != target.to_version
                    || upgraded.backup_path.trim().is_empty()
                    || target.impact.as_ref() != Some(&upgraded_impact)
                {
                    results.push(LocalDataStoreUpgradeResult::completed(
                        target,
                        !upgraded.backup_path.trim().is_empty(),
                    ));
                    return Err(LocalDataUpgradeError {
                        kind: LocalDataFailureKind::UnsafeData,
                        message: UPGRADE_APPLIED_INITIALIZATION_FAILED_MESSAGE.into(),
                        support_code: "LOCAL_DATA_BUSINESS_RESULT_MISMATCH".into(),
                        storage_upgraded: true,
                        results,
                    });
                }
                LocalDataStoreUpgradeResult::completed(target, true)
            }
        };
        results.push(result);
        after_store_commit(target.store);
    }
    post_upgrade_hook();
    let verified = inspect_data_dir(data_dir);
    if verified.state != LocalDataState::Ready {
        return Err(LocalDataUpgradeError {
            kind: verified
                .failure_kind
                .unwrap_or(LocalDataFailureKind::UnsafeData),
            message: UPGRADE_APPLIED_INITIALIZATION_FAILED_MESSAGE.into(),
            support_code: verified
                .support_code
                .unwrap_or_else(|| "LOCAL_DATA_POSTFLIGHT_FAILED".into()),
            storage_upgraded: !results.is_empty(),
            results,
        });
    }
    Ok(LocalDataUpgradeResult {
        status: LocalDataUpgradeStatus::Completed,
        message: UPGRADE_SUCCESS_MESSAGE.into(),
        support_code: None,
        results,
        remote_data_deleted: false,
    })
}

fn account_upgrade_error(
    error: onecrew_local_vault::VaultDataUpgradeError,
    target: &LocalDataUpgradeTarget,
    mut results: Vec<LocalDataStoreUpgradeResult>,
) -> LocalDataUpgradeError {
    let kind = match error.kind {
        VaultDataUpgradeFailureKind::Busy => LocalDataFailureKind::Busy,
        VaultDataUpgradeFailureKind::BackupFailed => LocalDataFailureKind::BackupFailed,
        VaultDataUpgradeFailureKind::NewerVersion => LocalDataFailureKind::NewerVersion,
        VaultDataUpgradeFailureKind::UnsafeData => LocalDataFailureKind::UnsafeData,
        VaultDataUpgradeFailureKind::InitializationFailed => {
            LocalDataFailureKind::InitializationFailed
        }
    };
    if error.storage_upgraded {
        results.push(LocalDataStoreUpgradeResult::completed(target, true));
    }
    let storage_upgraded = error.storage_upgraded || !results.is_empty();
    LocalDataUpgradeError {
        kind,
        message: if storage_upgraded {
            UPGRADE_APPLIED_INITIALIZATION_FAILED_MESSAGE.into()
        } else {
            blocked_message(kind).into()
        },
        support_code: public_account_support_code(&error.support_code),
        storage_upgraded,
        results,
    }
}

fn business_upgrade_error(
    error: onecrew_creator_runtime::CreatorDatabaseUpgradeError,
    target: &LocalDataUpgradeTarget,
    mut results: Vec<LocalDataStoreUpgradeResult>,
) -> LocalDataUpgradeError {
    let kind = match error.kind {
        CreatorDatabaseUpgradeFailureKind::Busy => LocalDataFailureKind::Busy,
        CreatorDatabaseUpgradeFailureKind::InsufficientSpace => {
            LocalDataFailureKind::InsufficientSpace
        }
        CreatorDatabaseUpgradeFailureKind::BackupFailed => LocalDataFailureKind::BackupFailed,
        CreatorDatabaseUpgradeFailureKind::UnsafeData => LocalDataFailureKind::UnsafeData,
    };
    if error.storage_upgraded {
        results.push(LocalDataStoreUpgradeResult::completed(target, true));
    }
    let storage_upgraded = error.storage_upgraded || !results.is_empty();
    LocalDataUpgradeError {
        kind,
        message: if storage_upgraded {
            UPGRADE_APPLIED_INITIALIZATION_FAILED_MESSAGE.into()
        } else {
            blocked_message(kind).into()
        },
        support_code: error.support_code,
        storage_upgraded,
        results,
    }
}

fn public_account_support_code(owner_code: &str) -> String {
    owner_code
        .strip_prefix("LOCAL_VAULT_")
        .map(|suffix| format!("LOCAL_DATA_ACCOUNT_{suffix}"))
        .unwrap_or_else(|| "LOCAL_DATA_ACCOUNT_UPGRADE_FAILED".into())
}

pub(crate) fn with_data_dir_lock<T>(
    data_dir: &Path,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let mut lock = acquire_data_dir_lock(data_dir).map_err(|error| error.to_string())?;
    let result = operation();
    if let Err(error) = lock.unlock() {
        eprintln!("local data lifecycle lock release failed: {error}");
    }
    result
}

fn acquire_data_dir_lock(data_dir: &Path) -> Result<LockFile, LocalDataUpgradeError> {
    fs::create_dir_all(data_dir).map_err(|error| {
        local_error(
            LocalDataFailureKind::UnsafeData,
            "LOCAL_DATA_ROOT_UNAVAILABLE",
            error,
            false,
        )
    })?;
    let lock_path = data_dir.join(".local-data-lifecycle.lock");
    let mut lock = LockFile::open(&lock_path).map_err(|error| {
        local_error(
            LocalDataFailureKind::Busy,
            "LOCAL_DATA_LOCK_OPEN_FAILED",
            error,
            false,
        )
    })?;
    restrict_private(&lock_path).map_err(|error| {
        local_error(
            LocalDataFailureKind::UnsafeData,
            "LOCAL_DATA_LOCK_PERMISSION_FAILED",
            error,
            false,
        )
    })?;
    match lock.try_lock() {
        Ok(true) => Ok(lock),
        Ok(false) => Err(LocalDataUpgradeError {
            kind: LocalDataFailureKind::Busy,
            message: blocked_message(LocalDataFailureKind::Busy).into(),
            support_code: "LOCAL_DATA_BUSY".into(),
            storage_upgraded: false,
            results: Vec::new(),
        }),
        Err(error) => Err(local_error(
            LocalDataFailureKind::Busy,
            "LOCAL_DATA_LOCK_FAILED",
            error,
            false,
        )),
    }
}

fn probe_data_dir_writable(data_dir: &Path) -> Result<(), LocalDataUpgradeError> {
    let probe = data_dir.join(format!(".local-data-write-probe-{}", Uuid::new_v4()));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|error| {
            local_error(
                LocalDataFailureKind::BackupFailed,
                "LOCAL_DATA_TARGET_NOT_WRITABLE",
                error,
                false,
            )
        })?;
    fs::remove_file(&probe).map_err(|error| {
        local_error(
            LocalDataFailureKind::BackupFailed,
            "LOCAL_DATA_WRITE_PROBE_CLEANUP_FAILED",
            error,
            false,
        )
    })
}

fn validate_data_dir_permissions(data_dir: &Path) -> Result<(), &'static str> {
    if data_dir.exists() {
        let metadata = fs::symlink_metadata(data_dir).map_err(|_| "LOCAL_DATA_ROOT_UNAVAILABLE")?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("LOCAL_DATA_ROOT_NOT_DIRECTORY");
        }
        validate_directory_permission_bits(&metadata)?;
        fs::read_dir(data_dir).map_err(|_| "LOCAL_DATA_ROOT_UNREADABLE")?;
        return Ok(());
    }

    let mut ancestor = data_dir.parent();
    while let Some(path) = ancestor {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err("LOCAL_DATA_PARENT_NOT_DIRECTORY");
                }
                validate_directory_permission_bits(&metadata)?;
                fs::read_dir(path).map_err(|_| "LOCAL_DATA_PARENT_UNREADABLE")?;
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ancestor = path.parent();
            }
            Err(_) => return Err("LOCAL_DATA_PARENT_UNAVAILABLE"),
        }
    }
    Err("LOCAL_DATA_PARENT_UNAVAILABLE")
}

fn validate_directory_permission_bits(metadata: &fs::Metadata) -> Result<(), &'static str> {
    if metadata.permissions().readonly() {
        return Err("LOCAL_DATA_NOT_WRITABLE");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o300 != 0o300 {
            return Err("LOCAL_DATA_NOT_WRITABLE");
        }
    }
    Ok(())
}

fn validate_profile_directories(
    data_dir: &Path,
    profiles: &[onecrew_local_vault::VaultProfileDirectory],
) -> Result<(), String> {
    let root = data_dir.join("profiles");
    let canonical_root = if profiles
        .iter()
        .any(|profile| profile.platform != Platform::Telegram)
    {
        let metadata = fs::symlink_metadata(&root).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("connected account profile root is not a regular directory".into());
        }
        validate_owner_readable(&metadata)?;
        fs::read_dir(&root).map_err(|error| error.to_string())?;
        fs::canonicalize(&root).map_err(|error| error.to_string())?
    } else {
        root
    };
    for profile in profiles {
        if profile.platform == Platform::Telegram {
            if !profile.browser_profile_dir.trim().is_empty() {
                return Err("Telegram account unexpectedly references browser profile data".into());
            }
            continue;
        }
        if profile.browser_profile_dir.trim().is_empty() {
            return Err("connected browser account is missing its profile directory".into());
        }
        let registered_path = Path::new(&profile.browser_profile_dir);
        let metadata = fs::symlink_metadata(registered_path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("connected browser account profile is not a regular directory".into());
        }
        validate_owner_readable(&metadata)?;
        fs::read_dir(registered_path).map_err(|error| error.to_string())?;
        let path = fs::canonicalize(registered_path).map_err(|error| error.to_string())?;
        if !path.starts_with(&canonical_root) {
            return Err(
                "connected browser account profile is outside the managed data root".into(),
            );
        }
    }
    Ok(())
}

fn validate_owner_readable(_metadata: &fs::Metadata) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let required = if _metadata.is_dir() { 0o500 } else { 0o400 };
        if _metadata.permissions().mode() & required != required {
            return Err("connected browser account profile is not readable".into());
        }
    }
    Ok(())
}

fn blocked_message(kind: LocalDataFailureKind) -> &'static str {
    match kind {
        LocalDataFailureKind::InsufficientSpace => {
            "可用空间不足。请释放本机空间后重试，原有数据未被修改。"
        }
        LocalDataFailureKind::Busy => "本地数据正被其他进程使用。请关闭其他应用实例后重试。",
        LocalDataFailureKind::BackupFailed => {
            "恢复备份创建失败，升级尚未开始。请检查本机存储后重试。"
        }
        LocalDataFailureKind::NewerVersion => "这些本地数据由更新版本的应用创建。请先更新应用。",
        LocalDataFailureKind::NotWritable => {
            "本地数据位置不可写。请检查本机文件权限后重新打开应用。"
        }
        LocalDataFailureKind::InitializationFailed => {
            "本地数据检查已完成，但相关功能未能初始化。请重新打开应用；如仍失败，请联系支持。"
        }
        LocalDataFailureKind::UnsafeData => "无法安全读取本地数据，原有数据未被修改。请联系支持。",
    }
}

fn local_error(
    kind: LocalDataFailureKind,
    support_code: &str,
    detail: impl fmt::Display,
    storage_upgraded: bool,
) -> LocalDataUpgradeError {
    eprintln!("local data operation {support_code}: {detail}");
    LocalDataUpgradeError {
        kind,
        message: if storage_upgraded {
            UPGRADE_APPLIED_INITIALIZATION_FAILED_MESSAGE.into()
        } else {
            blocked_message(kind).into()
        },
        support_code: support_code.into(),
        storage_upgraded,
        results: Vec::new(),
    }
}

#[cfg(unix)]
fn restrict_private(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict_private(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use onecrew_creator_runtime::{
        CreatorDatabaseStatus, CreatorId, CreatorOrigin, ManualCreatorCreateInput,
    };
    use onecrew_local_domain::ProfileSecret;

    fn create_current_data(directory: &Path) {
        drop(crate::LocalPlatform::open(directory).unwrap());
    }

    fn downgrade_account_to_v2(directory: &Path) {
        let connection = rusqlite::Connection::open(directory.join("agent-platform.db")).unwrap();
        connection
            .pragma_update(None, "user_version", 2_i64)
            .unwrap();
    }

    fn downgrade_business_to_v27(directory: &Path) {
        CreatorRuntime::downgrade_database_to_v27_for_test(directory.join("crm.db")).unwrap();
    }

    fn downgrade_business_to_v28(directory: &Path) {
        CreatorRuntime::downgrade_database_to_v28_for_test(directory.join("crm.db")).unwrap();
    }

    fn downgrade_business_to_v31(directory: &Path) {
        CreatorRuntime::downgrade_database_to_v31_for_test(directory.join("crm.db")).unwrap();
    }

    fn downgrade_business_to_v35(directory: &Path) {
        CreatorRuntime::downgrade_database_to_v35_for_test(directory.join("crm.db")).unwrap();
    }

    /// The released v35 wire shape of a campaign search candidate origin.
    fn v35_campaign_search_origin(discovery_id: &str) -> serde_json::Value {
        serde_json::json!({
            "kind": "campaign_discovery",
            "discovery_id": discovery_id,
            "campaign_id": "campaign-retired",
            "account_ref_id": "operator-x",
            "query": "crypto",
            "search_kind": "users"
        })
    }

    /// Frozen released v35 payload. Keep this historical wire shape independent
    /// of live Creator models so retiring discovery cannot rewrite the fixture.
    fn v35_candidate_json(candidate_id: &str, discovery_id: &str, external_id: &str) -> String {
        serde_json::json!({
            "candidate_id": candidate_id,
            "origin": v35_campaign_search_origin(discovery_id),
            "account": {
                "platform": "x",
                "external_id": external_id,
                "target_kind": "user",
                "handle": "creator",
                "display_name": "Converted Creator",
                "profile_url": null,
            },
            "public_data": { "followers": 7 },
            "contact_discovery": {
                "routes": [],
                "enrichments": [],
            },
            "identity_resolution": { "status": "new" },
            "allowed_actions": [],
        })
        .to_string()
    }

    /// Seed the v35 predecessor with the provenance the 35 -> 36 edge converts:
    /// a Campaign with one operator binding, a Creator whose campaign search
    /// origin is attributable through its discovery snapshot, a Creator whose
    /// snapshot is gone (its account observation still cites the source), a
    /// campaign-attributed audit row, the two retired Campaign create receipts,
    /// and an unrelated receipt that must survive.
    fn seed_v35_campaign_provenance(directory: &Path) {
        let connection = rusqlite::Connection::open(directory.join("crm.db")).unwrap();
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 INSERT INTO account_refs(id, platform, kind, label, created_at, updated_at)
                   VALUES ('operator-x', 'x', 'profile', 'X operator', 1, 1);
                 INSERT INTO campaigns(id, name, status, goal, created_at, updated_at)
                   VALUES ('campaign-retired', 'Retired', 'running', 'goal', 1, 1);
                 INSERT INTO campaign_account_bindings
                   (id, campaign_id, account_ref_id, purpose, created_at)
                   VALUES ('binding-x', 'campaign-retired', 'operator-x', 'discovery', 1);
                 INSERT INTO creators(id, display_name, created_at, updated_at) VALUES
                   ('creator-converted', 'Converted Creator', 1, 1),
                   ('creator-unattributable', 'Unattributable Creator', 1, 1);
                 INSERT INTO creator_relationships(creator_id, stage, created_at, updated_at)
                   SELECT id, 'prospect', 1, 1 FROM creators;
                 INSERT INTO platform_accounts
                   (id, platform, target_kind, external_id, created_at, updated_at) VALUES
                   ('account-converted', 'x', 'user', 'x-converted-1', 1, 1),
                   ('account-unattributable', 'x', 'user', 'x-lost-1', 1, 1);
                 INSERT INTO creator_account_links
                   (id, creator_id, platform_account_id, state, confidence, created_at, updated_at) VALUES
                   ('link-converted', 'creator-converted', 'account-converted', 'confirmed', 'exact', 1, 1),
                   ('link-unattributable', 'creator-unattributable', 'account-unattributable', 'confirmed', 'exact', 1, 1);
                 INSERT INTO observation_sources
                   (id, kind, platform, external_ref, account_ref_id, observed_at) VALUES
                   ('source-converted', 'initial_platform_account', 'x',
                    'candidate:discovery-converted:x:user:x-converted-1', 'operator-x', 1),
                   ('source-unattributable', 'initial_platform_account', 'x',
                    'candidate:discovery-lost:x:user:x-lost-1', 'operator-x', 1);
                 INSERT INTO platform_account_observations
                   (id, platform_account_id, source_id, handle, display_name, observed_at) VALUES
                   ('observation-converted', 'account-converted', 'source-converted',
                    'creator', 'Converted Creator', 1),
                   ('observation-unattributable', 'account-unattributable', 'source-unattributable',
                    'lost', 'Unattributable Creator', 1);
                 INSERT INTO creator_origins
                   (id, kind, creator_id, source_id, discovery_id, campaign_id,
                    operator_account_ref_id, discovered_at) VALUES
                   ('origin-converted', 'campaign_discovery', 'creator-converted',
                    'source-converted', 'discovery-converted', 'campaign-retired', NULL, 1),
                   ('origin-unattributable', 'campaign_discovery', 'creator-unattributable',
                    'source-unattributable', 'discovery-lost', 'campaign-retired', NULL, 1);
                 INSERT INTO audit_events
                   (id, kind, creator_id, account_ref_id, campaign_id, payload_json, created_at)
                   VALUES ('audit-campaign', 'creator_saved', 'creator-converted', 'operator-x',
                           'campaign-retired', '{}', 1);
                 INSERT INTO idempotency_records
                   (key, action, request_fingerprint, state, result_json, created_at, updated_at) VALUES
                   ('campaign-receipt', 'campaign_create', 'fp', 'completed',
                    '{\"campaign_id\":\"campaign-retired\"}', 1, 1),
                   ('template-receipt', 'campaign_template_create', 'fp', 'completed',
                    '{\"template_id\":\"template-retired\"}', 1, 1),
                   ('relationship-kept', 'relationship_update', 'fp', 'completed',
                    '{\"creator_id\":\"creator-converted\",\"stage\":\"engaged\"}', 1, 1);",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO creator_candidate_snapshots
                   (discovery_id, candidate_id, origin_kind, origin_json, candidate_json,
                    account_platform, account_target_kind, account_external_id, account_handle,
                    account_display_name, source_kind, campaign_id, account_ref_id,
                    created_at, updated_at)
                 VALUES ('discovery-converted', 'x:user:x-converted-1', 'campaign_discovery',
                         ?1, ?2, 'x', 'user', 'x-converted-1', 'creator', 'Converted Creator',
                         'campaign_discovery', 'campaign-retired', 'operator-x', 1, 1)",
                rusqlite::params![
                    v35_campaign_search_origin("discovery-converted").to_string(),
                    v35_candidate_json(
                        "x:user:x-converted-1",
                        "discovery-converted",
                        "x-converted-1"
                    ),
                ],
            )
            .unwrap();
    }

    /// The typed impact `seed_v35_campaign_provenance` must produce.
    fn seeded_v35_impact() -> CreatorDatabaseUpgradeImpact {
        CreatorDatabaseUpgradeImpact {
            removed_campaigns: 1,
            removed_campaign_account_bindings: 1,
            converted_campaign_search_snapshots: 1,
            removed_candidate_snapshots: 1,
            converted_campaign_search_origins: 1,
            removed_unattributable_campaign_origins: 1,
            cleared_audit_campaign_refs: 1,
            removed_campaign_idempotency_records: 2,
            ..Default::default()
        }
    }

    fn target_stores(preflight: &LocalDataPreflight) -> Vec<LocalDataStore> {
        preflight
            .targets
            .iter()
            .map(|target| target.store)
            .collect()
    }

    #[test]
    fn public_upgrade_copy_is_stable_and_hides_storage_implementation() {
        let required = LocalDataPreflight::upgrade_required(vec![
            LocalDataUpgradeTarget::account(2, 4),
            LocalDataUpgradeTarget::business(
                27,
                onecrew_creator_runtime::CREATOR_SCHEMA_VERSION,
                CreatorDatabaseUpgradeImpact::default(),
            ),
        ]);
        let blocked = LocalDataPreflight::blocked(
            Some(LocalDataStore::Account),
            LocalDataFailureKind::UnsafeData,
            "LOCAL_DATA_ACCOUNT_UNSAFE",
        );
        let result = LocalDataUpgradeResult {
            status: LocalDataUpgradeStatus::Completed,
            message: UPGRADE_SUCCESS_MESSAGE.into(),
            support_code: None,
            results: required
                .targets
                .iter()
                .map(|target| LocalDataStoreUpgradeResult::completed(target, true))
                .collect(),
            remote_data_deleted: false,
        };
        assert_eq!(required.title.as_deref(), Some(UPGRADE_TITLE));
        assert!(blocked.primary_action.is_none());
        for value in [
            serde_json::to_string(&required).unwrap(),
            serde_json::to_string(&blocked).unwrap(),
            serde_json::to_string(&result).unwrap(),
        ] {
            for forbidden in [
                "crm.db",
                "agent-platform.db",
                "vault",
                "database",
                "schema",
                "backup_path",
                "secret",
                "v27",
                "v12",
                "v13",
            ] {
                assert!(!value.to_ascii_lowercase().contains(forbidden), "{value}");
            }
        }
    }

    #[test]
    fn newer_account_storage_maps_to_the_typed_newer_version_failure() {
        let directory = tempfile::tempdir().unwrap();
        let connection =
            rusqlite::Connection::open(directory.path().join("agent-platform.db")).unwrap();
        connection
            .execute_batch("CREATE TABLE future (id TEXT PRIMARY KEY); PRAGMA user_version = 99;")
            .unwrap();
        drop(connection);
        fs::write(directory.path().join("vault.key"), [9_u8; 32]).unwrap();
        #[cfg(unix)]
        restrict_private(&directory.path().join("vault.key")).unwrap();
        let status = inspect_data_dir(directory.path());
        assert_eq!(status.state, LocalDataState::Blocked);
        assert_eq!(
            status.failure_kind,
            Some(LocalDataFailureKind::NewerVersion)
        );
    }

    #[test]
    fn ambiguous_v36_delete_impact_blocks_before_backup_with_a_safe_owner_code() {
        let directory = tempfile::tempdir().unwrap();
        let platform = crate::LocalPlatform::open(directory.path()).unwrap();
        let creator = platform
            .creators
            .create_manual_creator(ManualCreatorCreateInput {
                display_name: "Preserved Creator".into(),
                notes: None,
                tags: None,
                contact_routes: Vec::new(),
            })
            .unwrap();
        drop(platform);
        let database = directory.path().join("crm.db");
        CreatorRuntime::downgrade_database_to_v36_for_test(&database).unwrap();
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        let original = r#"{"private_fixture":"must-not-appear-in-status"}"#;
        connection
            .execute(
                "INSERT INTO creator_contact_route_delete_operations
                (id, creator_id, contact_route_id, plan_fingerprint, impact_json, status,
                 result_json, expires_at, created_at, updated_at)
             VALUES ('ambiguous-impact', ?1, 'old-route', 'old-plan', ?2, 'prepared',
                     NULL, 100, 1, 1)",
                rusqlite::params![creator.creator_id.as_str(), original],
            )
            .unwrap();
        drop(connection);

        let preflight = inspect_data_dir(directory.path());
        assert_eq!(preflight.state, LocalDataState::Blocked);
        assert_eq!(
            preflight.failure_kind,
            Some(LocalDataFailureKind::UnsafeData)
        );
        assert_eq!(
            preflight.support_code.as_deref(),
            Some("LOCAL_DATA_CONTACT_ROUTE_DELETE_IMPACT_UNSAFE")
        );
        assert!(preflight.targets.is_empty());
        assert!(!preflight.recoverable_backup);
        assert!(
            !serde_json::to_string(&preflight)
                .unwrap()
                .contains("must-not-appear-in-status")
        );
        let failure = upgrade_data_dir(directory.path()).unwrap_err();
        assert_eq!(
            failure.support_code,
            "LOCAL_DATA_CONTACT_ROUTE_DELETE_IMPACT_UNSAFE"
        );
        assert!(!failure.storage_upgraded);
        assert!(failure.results.is_empty());
        assert!(!fs::read_dir(directory.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".backup-")
        }));
        let connection = rusqlite::Connection::open(&database).unwrap();
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        let stored: String = connection.query_row(
            "SELECT impact_json FROM creator_contact_route_delete_operations WHERE id='ambiguous-impact'",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(version, 36);
        assert_eq!(stored, original);
    }

    #[test]
    fn lifecycle_lock_reports_busy_without_running_operation() {
        let directory = tempfile::tempdir().unwrap();
        let mut lock = acquire_data_dir_lock(directory.path()).unwrap();
        let error = upgrade_data_dir(directory.path()).unwrap_err();
        assert_eq!(error.kind, LocalDataFailureKind::Busy);
        lock.unlock().unwrap();
    }

    #[test]
    fn lifecycle_lock_rejects_ordinary_open_of_fresh_and_current_data_dirs() {
        for populated in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            if populated {
                create_current_data(directory.path());
            }
            let mut lock = acquire_data_dir_lock(directory.path()).unwrap();
            let Err(error) = crate::LocalPlatform::open(directory.path()) else {
                panic!("a held lifecycle lock must reject an ordinary open");
            };
            assert!(error.ends_with("(LOCAL_DATA_BUSY)"), "{error}");
            assert_eq!(
                directory.path().join("agent-platform.db").exists(),
                populated
            );
            assert_eq!(directory.path().join("crm.db").exists(), populated);
            let artifacts = fs::read_dir(directory.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    name.contains(".backup-") || name.starts_with(".local-data-write-probe-")
                })
                .count();
            assert_eq!(artifacts, 0, "a rejected open leaves no artifacts");
            lock.unlock().unwrap();
            drop(crate::LocalPlatform::open(directory.path()).unwrap());
            assert_eq!(
                inspect_data_dir(directory.path()).state,
                LocalDataState::Ready
            );
        }
    }

    #[test]
    fn preflight_rejects_a_non_directory_data_root_without_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let data_root = directory.path().join("app-data");
        fs::write(&data_root, b"not a directory").unwrap();
        let before = fs::read(&data_root).unwrap();
        let status = inspect_data_dir(&data_root);
        assert_eq!(status.state, LocalDataState::Blocked);
        assert_eq!(status.failure_kind, Some(LocalDataFailureKind::NotWritable));
        assert_eq!(fs::read(&data_root).unwrap(), before);
    }

    #[cfg(unix)]
    #[test]
    fn read_only_preflight_detects_unwritable_root_without_creating_probe_or_storage() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let data_root = directory.path().join("app-data");
        fs::create_dir(&data_root).unwrap();
        fs::set_permissions(&data_root, fs::Permissions::from_mode(0o500)).unwrap();
        let status = inspect_data_dir(&data_root);
        assert_eq!(status.state, LocalDataState::Blocked);
        assert_eq!(status.failure_kind, Some(LocalDataFailureKind::NotWritable));
        assert!(!data_root.join("crm.db").exists());
        assert!(!data_root.join("agent-platform.db").exists());
        assert!(!data_root.join("vault.key").exists());
        fs::set_permissions(&data_root, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(fs::read_dir(&data_root).unwrap().count(), 0);
    }

    #[test]
    fn profile_preflight_rejects_missing_and_out_of_root_directories() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("profiles")).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let profile = |path: &Path| onecrew_local_vault::VaultProfileDirectory {
            profile_id: Uuid::new_v4(),
            platform: Platform::X,
            browser_profile_dir: path.to_string_lossy().into_owned(),
        };
        assert!(
            validate_profile_directories(
                directory.path(),
                &[profile(&directory.path().join("profiles/missing"))]
            )
            .is_err()
        );
        assert!(
            validate_profile_directories(directory.path(), &[profile(outside.path())]).is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn profile_preflight_rejects_unreadable_directories() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let profile_path = directory.path().join("profiles/profile-1");
        fs::create_dir_all(&profile_path).unwrap();
        fs::set_permissions(&profile_path, fs::Permissions::from_mode(0o000)).unwrap();
        let profile = onecrew_local_vault::VaultProfileDirectory {
            profile_id: Uuid::new_v4(),
            platform: Platform::X,
            browser_profile_dir: profile_path.to_string_lossy().into_owned(),
        };
        assert!(validate_profile_directories(directory.path(), &[profile]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn profile_preflight_does_not_traverse_internal_symlink_cycles_or_cache_contents() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let profile_path = directory.path().join("profiles/profile-1");
        let cache_path = profile_path.join("Cache");
        fs::create_dir_all(&cache_path).unwrap();
        for index in 0..256 {
            fs::write(cache_path.join(format!("entry-{index}")), b"cache").unwrap();
        }
        symlink(".", profile_path.join("cycle")).unwrap();
        let profile = onecrew_local_vault::VaultProfileDirectory {
            profile_id: Uuid::new_v4(),
            platform: Platform::X,
            browser_profile_dir: profile_path.to_string_lossy().into_owned(),
        };
        validate_profile_directories(directory.path(), &[profile])
            .expect("preflight validates registered roots without traversing Chromium internals");
    }

    #[test]
    fn v35_upgrade_projects_campaign_retirement_impact_and_preserves_accounts_and_creators() {
        let directory = tempfile::tempdir().unwrap();
        let account_path = directory.path().join("agent-platform.db");
        let vault = LocalVault::open(&account_path).unwrap();
        vault
            .upsert(
                Platform::Telegram,
                "operator",
                &ProfileSecret {
                    provider_account_id: "telegram-operator".into(),
                    cookies: Vec::new(),
                    x_chat_passcode: None,
                    instagram_session_settings: None,
                    telegram_api_id: Some(1),
                    telegram_api_hash: Some("hash".into()),
                    telegram_session: Some("session".into()),
                    profile_name: String::new(),
                    browser_profile_dir: String::new(),
                    fingerprint_seed: String::new(),
                    proxy: None,
                    proxy_egress: None,
                },
            )
            .unwrap();
        drop(vault);
        fs::create_dir(directory.path().join("profiles")).unwrap();
        let profile_sentinel = directory.path().join("profiles/keep-me");
        fs::write(&profile_sentinel, b"profile-bytes").unwrap();
        create_current_data(directory.path());
        downgrade_business_to_v35(directory.path());
        seed_v35_campaign_provenance(directory.path());
        assert!(matches!(
            CreatorRuntime::inspect_database(directory.path().join("crm.db")),
            CreatorDatabaseStatus::UpgradeRequired { .. }
        ));

        let preflight = inspect_data_dir(directory.path());
        assert_eq!(preflight.state, LocalDataState::UpgradeRequired);
        assert_eq!(target_stores(&preflight), vec![LocalDataStore::Business]);
        assert_eq!(preflight.targets[0].from_version, 35);
        assert_eq!(
            preflight.targets[0].to_version,
            onecrew_creator_runtime::CREATOR_SCHEMA_VERSION
        );
        assert_eq!(preflight.targets[0].impact, Some(seeded_v35_impact()));
        assert!(preflight.recoverable_backup);
        assert!(!preflight.remote_data_deleted);
        let account_bytes = fs::read(&account_path).unwrap();
        let key_bytes = fs::read(directory.path().join("vault.key")).unwrap();

        let upgraded = upgrade_data_dir(directory.path()).unwrap();
        assert_eq!(upgraded.status, LocalDataUpgradeStatus::Completed);
        assert_eq!(upgraded.results.len(), 1);
        assert_eq!(upgraded.results[0].store, LocalDataStore::Business);
        assert_eq!(upgraded.results[0].from_version, 35);
        assert_eq!(
            upgraded.results[0].to_version,
            onecrew_creator_runtime::CREATOR_SCHEMA_VERSION
        );
        assert!(upgraded.results[0].recovery_point_created);
        assert_eq!(upgraded.results[0].impact, Some(seeded_v35_impact()));
        assert!(!upgraded.remote_data_deleted);
        assert_eq!(fs::read(&account_path).unwrap(), account_bytes);
        assert_eq!(
            fs::read(directory.path().join("vault.key")).unwrap(),
            key_bytes
        );
        assert_eq!(fs::read(&profile_sentinel).unwrap(), b"profile-bytes");
        assert_eq!(
            inspect_data_dir(directory.path()).state,
            LocalDataState::Ready
        );
        let receipts: Vec<String> = rusqlite::Connection::open(directory.path().join("crm.db"))
            .unwrap()
            .prepare("SELECT key || ':' || action FROM idempotency_records ORDER BY key")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            receipts,
            vec!["relationship-kept:relationship_update".to_string()],
            "the retired Campaign receipts are removed and the unrelated one survives"
        );

        let reopened = crate::LocalPlatform::open(directory.path()).unwrap();
        assert_eq!(reopened.list_profiles().unwrap().len(), 1);
        let creators = reopened.creators().unwrap();
        let converted = creators
            .get_creator(&CreatorId::new("creator-converted"))
            .unwrap();
        assert_eq!(converted.creator.display_name, "Converted Creator");
        assert!(matches!(
            converted.origins.as_slice(),
            [CreatorOrigin::PlatformSearch {
                discovery_id,
                account_ref_id,
                creator_id,
                ..
            }] if discovery_id.as_str() == "discovery-converted"
                && account_ref_id.as_str() == "operator-x"
                && creator_id.as_str() == "creator-converted"
        ));
        assert_eq!(converted.observation_sources.len(), 1);
        let unattributable = creators
            .get_creator(&CreatorId::new("creator-unattributable"))
            .unwrap();
        assert_eq!(
            unattributable.creator.display_name,
            "Unattributable Creator"
        );
        assert!(unattributable.origins.is_empty());
        assert_eq!(unattributable.account_observations.len(), 1);
        assert_eq!(unattributable.observation_sources.len(), 1);
        for record in [&converted, &unattributable] {
            let wire = serde_json::to_string(record).unwrap();
            assert!(!wire.to_ascii_lowercase().contains("campaign"), "{wire}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn postflight_failure_reports_applied_upgrade_and_allows_initialization_retry() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        drop(crate::LocalPlatform::open(directory.path()).unwrap());
        onecrew_creator_runtime::CreatorRuntime::downgrade_database_to_v27_for_test(
            directory.path().join("crm.db"),
        )
        .unwrap();
        let key_path = directory.path().join("vault.key");
        let error = upgrade_data_dir_locked_with_post_upgrade_hook(directory.path(), || {
            fs::set_permissions(&key_path, fs::Permissions::from_mode(0o644)).unwrap();
        })
        .unwrap_err();
        assert!(error.storage_upgraded);
        assert_eq!(
            target_stores(&inspect_data_dir(directory.path())),
            Vec::new()
        );
        assert_eq!(error.results.len(), 1);
        assert_eq!(error.results[0].store, LocalDataStore::Business);
        assert_eq!(
            CreatorRuntime::inspect_database(directory.path().join("crm.db")),
            CreatorDatabaseStatus::Ready
        );

        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            upgrade_data_dir(directory.path()).unwrap().status,
            LocalDataUpgradeStatus::AlreadyCurrent
        );
        crate::LocalPlatform::open(directory.path()).unwrap();
    }

    #[test]
    fn combined_preflight_projects_fresh_current_and_each_upgrade_target_set() {
        let fresh = tempfile::tempdir().unwrap();
        let status = inspect_data_dir(fresh.path());
        assert_eq!(status.state, LocalDataState::Fresh);
        assert!(status.targets.is_empty());

        let current = tempfile::tempdir().unwrap();
        create_current_data(current.path());
        let status = inspect_data_dir(current.path());
        assert_eq!(status.state, LocalDataState::Ready);
        assert!(status.targets.is_empty());

        let account_only = tempfile::tempdir().unwrap();
        create_current_data(account_only.path());
        downgrade_account_to_v2(account_only.path());
        let status = inspect_data_dir(account_only.path());
        assert_eq!(status.state, LocalDataState::UpgradeRequired);
        assert_eq!(target_stores(&status), vec![LocalDataStore::Account]);
        assert_eq!(
            (status.targets[0].from_version, status.targets[0].to_version),
            (2, 4)
        );
        assert!(status.targets[0].impact.is_none());

        let account_without_business = tempfile::tempdir().unwrap();
        drop(LocalVault::open(account_without_business.path().join("agent-platform.db")).unwrap());
        downgrade_account_to_v2(account_without_business.path());
        let status = inspect_data_dir(account_without_business.path());
        assert_eq!(status.state, LocalDataState::UpgradeRequired);
        assert_eq!(target_stores(&status), vec![LocalDataStore::Account]);
        assert!(!account_without_business.path().join("crm.db").exists());

        let business_only = tempfile::tempdir().unwrap();
        create_current_data(business_only.path());
        downgrade_business_to_v27(business_only.path());
        let status = inspect_data_dir(business_only.path());
        assert_eq!(status.state, LocalDataState::UpgradeRequired);
        assert_eq!(target_stores(&status), vec![LocalDataStore::Business]);
        assert_eq!(status.targets[0].from_version, 27);
        assert_eq!(
            status.targets[0].to_version,
            onecrew_creator_runtime::CREATOR_SCHEMA_VERSION
        );
        assert_eq!(
            status.targets[0].impact,
            Some(CreatorDatabaseUpgradeImpact::default())
        );

        let simultaneous = tempfile::tempdir().unwrap();
        create_current_data(simultaneous.path());
        downgrade_account_to_v2(simultaneous.path());
        downgrade_business_to_v27(simultaneous.path());
        let status = inspect_data_dir(simultaneous.path());
        assert_eq!(status.state, LocalDataState::UpgradeRequired);
        assert_eq!(
            target_stores(&status),
            vec![LocalDataStore::Account, LocalDataStore::Business]
        );
        assert!(status.recoverable_backup);
        assert!(!status.remote_data_deleted);

        let incomplete = tempfile::tempdir().unwrap();
        create_current_data(incomplete.path());
        fs::remove_file(incomplete.path().join("agent-platform.db")).unwrap();
        fs::remove_file(incomplete.path().join("vault.key")).unwrap();
        let status = inspect_data_dir(incomplete.path());
        assert_eq!(status.state, LocalDataState::Blocked);
        assert_eq!(status.blocked_store, Some(LocalDataStore::Account));
        assert!(status.targets.is_empty());
    }

    #[test]
    fn v28_business_predecessor_is_a_safe_one_shot_automatic_upgrade() {
        let directory = tempfile::tempdir().unwrap();
        create_current_data(directory.path());
        downgrade_business_to_v28(directory.path());

        let preflight = inspect_data_dir(directory.path());
        assert_eq!(preflight.state, LocalDataState::UpgradeRequired);
        assert_eq!(target_stores(&preflight), vec![LocalDataStore::Business]);
        assert_eq!(preflight.targets[0].from_version, 28);
        assert_eq!(
            preflight.targets[0].to_version,
            onecrew_creator_runtime::CREATOR_SCHEMA_VERSION
        );
        assert_eq!(
            preflight.targets[0].impact,
            Some(CreatorDatabaseUpgradeImpact::default())
        );
        assert!(preflight.recoverable_backup);
        assert!(!preflight.remote_data_deleted);

        let upgraded = upgrade_data_dir(directory.path()).unwrap();
        assert_eq!(upgraded.status, LocalDataUpgradeStatus::Completed);
        assert_eq!(upgraded.results.len(), 1);
        assert_eq!(upgraded.results[0].store, LocalDataStore::Business);
        assert_eq!(upgraded.results[0].from_version, 28);
        assert_eq!(
            upgraded.results[0].to_version,
            onecrew_creator_runtime::CREATOR_SCHEMA_VERSION
        );
        assert!(upgraded.results[0].recovery_point_created);
        assert_eq!(
            inspect_data_dir(directory.path()).state,
            LocalDataState::Ready
        );

        let second_attempt = upgrade_data_dir(directory.path()).unwrap();
        assert_eq!(
            second_attempt.status,
            LocalDataUpgradeStatus::AlreadyCurrent
        );
        assert!(second_attempt.results.is_empty());
    }

    #[test]
    fn v3_v31_upgrade_preserves_legacy_resend_as_an_unavailable_mailbox() {
        let directory = tempfile::tempdir().unwrap();
        let platform = crate::LocalPlatform::open(directory.path()).unwrap();
        let credential = platform.vault
            .upsert_email_secret_record(
                onecrew_local_vault::EmailSecretRecordKind::ResendApiKey,
                "Legacy Resend",
                serde_json::json!({
                    "email_address": "legacy@example.com",
                    "display_name": "Legacy Sender"
                }),
                &onecrew_local_vault::EmailSecretRecordSecret::new("re_preserved_across_upgrade"),
            )
            .unwrap();
        platform.creators.reconcile_mailbox_refs(vec![onecrew_creator_runtime::MailboxRefInput {
            id: onecrew_creator_runtime::MailboxRefId::new(credential.id.to_string()),
            provider: "resend".into(),
            label: credential.label.clone(),
            email_address: "legacy@example.com".into(),
            display_name: Some("Legacy Sender".into()),
            capabilities: vec![onecrew_creator_runtime::EmailCapability::Send],
            available: true,
            last_error: None,
        }]).unwrap();
        drop(platform);

        let vault_path = directory.path().join("agent-platform.db");
        let vault = rusqlite::Connection::open(&vault_path).unwrap();
        vault
            .execute(
                "UPDATE service_credentials
                 SET provider = 'resend'
                 WHERE id = ?1 AND provider = 'email_resend_api_key'",
                [credential.id.to_string()],
            )
            .unwrap();
        vault.pragma_update(None, "user_version", 3_i64).unwrap();
        drop(vault);
        downgrade_business_to_v31(directory.path());

        let preflight = inspect_data_dir(directory.path());
        assert_eq!(preflight.state, LocalDataState::UpgradeRequired);
        assert_eq!(
            target_stores(&preflight),
            vec![LocalDataStore::Account, LocalDataStore::Business]
        );
        assert_eq!(
            preflight
                .targets
                .iter()
                .map(|target| (target.from_version, target.to_version))
                .collect::<Vec<_>>(),
            vec![
                (3, 4),
                (31, onecrew_creator_runtime::CREATOR_SCHEMA_VERSION)
            ]
        );

        let upgraded = upgrade_data_dir(directory.path()).unwrap();
        assert_eq!(upgraded.status, LocalDataUpgradeStatus::Completed);
        assert_eq!(upgraded.results.len(), 2);
        let reopened = crate::LocalPlatform::open(directory.path()).unwrap();
        let credentials = reopened.vault.list_email_secret_records(
            onecrew_local_vault::EmailSecretRecordKind::ResendApiKey,
            true,
        ).unwrap();
        assert_eq!(credentials.len(), 1);
        assert_eq!(credentials[0].id, credential.id);
        let mailboxes = reopened.creators.list_mailboxes().unwrap();
        assert_eq!(mailboxes.len(), 1);
        assert!(!mailboxes[0].available);
        assert_eq!(mailboxes[0].email_address, "legacy@example.com");
        let binding = reopened.creators.resolve_mailbox_auth_binding_blocking(
            &mailboxes[0].mailbox_ref_id,
        ).unwrap();
        assert!(!binding.available);
        assert!(!binding.auth_connection.available);
        assert_eq!(binding.auth_connection.auth_connection_id,
            onecrew_creator_runtime::resend_auth_connection_id(credential.id));
        assert_eq!(binding.auth_connection.secret_record_id.as_str(), credential.id.to_string());
        drop(reopened);

        let vault = rusqlite::Connection::open(vault_path).unwrap();
        let provider = vault
            .query_row(
                "SELECT provider FROM service_credentials WHERE id = ?1",
                [credential.id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(provider, "email_resend_api_key");
    }

    #[test]
    fn account_only_upgrade_uses_owner_recovery_point_and_leaves_business_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        create_current_data(directory.path());
        let platform = crate::LocalPlatform::open(directory.path()).unwrap();
        let sentinel = platform
            .creators()
            .unwrap()
            .create_manual_creator(ManualCreatorCreateInput {
                tags: None,
                display_name: "Account Evolution Sentinel".into(),
                notes: Some("business data must remain unchanged".into()),
                contact_routes: Vec::new(),
            })
            .unwrap();
        drop(platform);
        let business_path = directory.path().join("crm.db");
        let business_bytes = fs::read(&business_path).unwrap();
        let key_bytes = fs::read(directory.path().join("vault.key")).unwrap();
        downgrade_account_to_v2(directory.path());

        let upgraded = upgrade_data_dir(directory.path()).unwrap();
        assert_eq!(upgraded.status, LocalDataUpgradeStatus::Completed);
        assert_eq!(upgraded.results.len(), 1);
        assert_eq!(upgraded.results[0].store, LocalDataStore::Account);
        assert!(upgraded.results[0].recovery_point_created);
        assert_eq!(
            (
                upgraded.results[0].from_version,
                upgraded.results[0].to_version
            ),
            (2, 4)
        );
        assert_eq!(fs::read(&business_path).unwrap(), business_bytes);
        assert_eq!(
            fs::read(directory.path().join("vault.key")).unwrap(),
            key_bytes
        );
        assert_eq!(
            LocalVault::inspect(directory.path().join("agent-platform.db")).status,
            VaultDataStatus::Ready
        );
        assert_eq!(
            CreatorRuntime::inspect_database(&business_path),
            CreatorDatabaseStatus::Ready
        );
        assert_eq!(
            crate::LocalPlatform::open(directory.path())
                .unwrap()
                .creators()
                .unwrap()
                .get_creator(&sentinel.creator_id)
                .unwrap()
                .creator
                .display_name,
            "Account Evolution Sentinel"
        );
    }

    #[test]
    fn simultaneous_upgrade_reports_each_owner_commit_and_valid_recovery_point() {
        let directory = tempfile::tempdir().unwrap();
        create_current_data(directory.path());
        downgrade_account_to_v2(directory.path());
        downgrade_business_to_v27(directory.path());

        let upgraded = upgrade_data_dir(directory.path()).unwrap();
        assert_eq!(
            upgraded
                .results
                .iter()
                .map(|result| result.store)
                .collect::<Vec<_>>(),
            vec![LocalDataStore::Account, LocalDataStore::Business]
        );
        assert!(
            upgraded
                .results
                .iter()
                .all(|result| result.recovery_point_created)
        );
        assert_eq!(
            inspect_data_dir(directory.path()).state,
            LocalDataState::Ready
        );

        let recovery_points = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.contains(".backup-"))
            })
            .collect::<Vec<_>>();
        let account_recovery = recovery_points
            .iter()
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("agent-platform.db.backup-"))
            })
            .unwrap();
        let business_recovery = recovery_points
            .iter()
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("crm.db.backup-"))
            })
            .unwrap();
        assert!(matches!(
            LocalVault::inspect(account_recovery).status,
            VaultDataStatus::UpgradeRequired { .. }
        ));
        assert!(matches!(
            CreatorRuntime::inspect_database(business_recovery),
            CreatorDatabaseStatus::UpgradeRequired { .. }
        ));

        let retry = upgrade_data_dir(directory.path()).unwrap();
        assert_eq!(retry.status, LocalDataUpgradeStatus::AlreadyCurrent);
        assert!(retry.results.is_empty());
    }

    #[test]
    fn first_store_commit_second_store_failure_retries_only_remaining_target() {
        let directory = tempfile::tempdir().unwrap();
        create_current_data(directory.path());
        downgrade_account_to_v2(directory.path());
        downgrade_business_to_v27(directory.path());
        let business_path = directory.path().join("crm.db");
        let held_business_path = directory.path().join("business-data-held-for-test");

        let mut lock = acquire_data_dir_lock(directory.path()).unwrap();
        let error = upgrade_data_dir_locked_with_hooks(
            directory.path(),
            |store| {
                if store == LocalDataStore::Account {
                    fs::rename(&business_path, &held_business_path).unwrap();
                }
            },
            || {},
        )
        .unwrap_err();
        lock.unlock().unwrap();
        fs::rename(&held_business_path, &business_path).unwrap();

        assert!(error.storage_upgraded);
        assert_eq!(error.results.len(), 1);
        assert_eq!(error.results[0].store, LocalDataStore::Account);
        assert!(error.results[0].recovery_point_created);
        assert_eq!(
            LocalVault::inspect(directory.path().join("agent-platform.db")).status,
            VaultDataStatus::Ready
        );
        assert!(matches!(
            CreatorRuntime::inspect_database(&business_path),
            CreatorDatabaseStatus::UpgradeRequired { .. }
        ));
        let remaining = inspect_data_dir(directory.path());
        assert_eq!(remaining.state, LocalDataState::UpgradeRequired);
        assert_eq!(target_stores(&remaining), vec![LocalDataStore::Business]);

        let retried = upgrade_data_dir(directory.path()).unwrap();
        assert_eq!(retried.results.len(), 1);
        assert_eq!(retried.results[0].store, LocalDataStore::Business);
        assert_eq!(
            inspect_data_dir(directory.path()).state,
            LocalDataState::Ready
        );
    }

    #[test]
    fn explicit_upgrade_rejects_fresh_and_is_idempotent_when_current() {
        let fresh = tempfile::tempdir().unwrap();
        let error = upgrade_data_dir(fresh.path()).unwrap_err();
        assert!(!error.storage_upgraded);
        assert!(error.results.is_empty());
        assert!(!fresh.path().join("agent-platform.db").exists());
        assert!(!fresh.path().join("crm.db").exists());

        let current = tempfile::tempdir().unwrap();
        create_current_data(current.path());
        let result = upgrade_data_dir(current.path()).unwrap();
        assert_eq!(result.status, LocalDataUpgradeStatus::AlreadyCurrent);
        assert!(result.results.is_empty());
    }
}
