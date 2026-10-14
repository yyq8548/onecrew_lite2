use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};

use onecrew_creator_runtime::{
    CreatorRepositoryCallError, CreatorRepositoryDispatchFailure, CreatorTransferFileFormat,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::LocalPlatform;

const EXPORT_FILE_PREFIX: &str = "reacher-creators";
const DIGEST_FILE_PREFIX_BYTES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorExportDestination {
    SystemDesktop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorExportInput {
    pub format: CreatorTransferFileFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreatorExportResult {
    pub destination: CreatorExportDestination,
    pub format: CreatorTransferFileFormat,
    pub file_name: String,
    pub creator_count: u32,
    pub content_sha256: String,
    pub reused_existing: bool,
}

// Desktop writes are bounded independently of Creator snapshot/CPU capacity.
static DESKTOP_EXPORT_WRITES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

impl LocalPlatform {
    /// Snapshot and encode under Creator admission, then write on a bounded
    /// blocking worker. The write permit belongs to the complete worker.
    pub async fn export_creators_to_system_desktop(
        &self,
        input: CreatorExportInput,
    ) -> Result<CreatorExportResult, CreatorRepositoryCallError<String>> {
        let desktop_dir = self
            .system_desktop_dir
            .clone()
            .ok_or_else(|| {
                "the operating system Desktop directory is unavailable for Creator export"
                    .to_string()
            })
            .map_err(CreatorRepositoryCallError::Owner)?;
        let artifact = match input.format {
            CreatorTransferFileFormat::Xlsx => {
                self.creators.export_creator_workbook_xlsx_async().await?
            }
        };
        let permit = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            DESKTOP_EXPORT_WRITES.acquire(),
        )
        .await
        .map_err(|_| {
            CreatorRepositoryCallError::Dispatch(CreatorRepositoryDispatchFailure::Overloaded)
        })?
        .map_err(|_| {
            CreatorRepositoryCallError::Dispatch(CreatorRepositoryDispatchFailure::Closed)
        })?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let metadata = fs::metadata(&desktop_dir)
                .map_err(|error| desktop_io_error("inspect", error.kind()))?;
            if !metadata.is_dir() {
                return Err(
                    "the operating system Desktop export location is not a directory".into(),
                );
            }
            let file_name = format!(
                "{EXPORT_FILE_PREFIX}-{}.{}",
                &artifact.artifact_sha256[..DIGEST_FILE_PREFIX_BYTES],
                input.format.extension()
            );
            let target = desktop_dir.join(&file_name);
            let reused_existing = write_no_clobber(&desktop_dir, &target, &artifact.bytes)?;
            Ok(CreatorExportResult {
                destination: CreatorExportDestination::SystemDesktop,
                format: input.format,
                file_name,
                creator_count: artifact.creator_count,
                content_sha256: artifact.content_sha256,
                reused_existing,
            })
        })
        .await
        .map_err(|error| {
            CreatorRepositoryCallError::Dispatch(if error.is_cancelled() {
                CreatorRepositoryDispatchFailure::CancelledBeforeDispatch
            } else {
                CreatorRepositoryDispatchFailure::OutcomeUnknown
            })
        })?
        .map_err(CreatorRepositoryCallError::Owner)
    }
}

fn write_no_clobber(directory: &Path, target: &Path, bytes: &[u8]) -> Result<bool, String> {
    match fs::symlink_metadata(target) {
        Ok(metadata) => return verify_existing(target, metadata, bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(desktop_io_error("inspect target", error.kind())),
    }

    let mut temporary = tempfile::NamedTempFile::new_in(directory)
        .map_err(|error| desktop_io_error("create temporary export", error.kind()))?;
    temporary
        .write_all(bytes)
        .map_err(|error| desktop_io_error("write export", error.kind()))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| desktop_io_error("flush export", error.kind()))?;
    match temporary.persist_noclobber(target) {
        Ok(_) => Ok(false),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = fs::symlink_metadata(target)
                .map_err(|inspect| desktop_io_error("inspect raced target", inspect.kind()))?;
            verify_existing(target, metadata, bytes)
        }
        Err(error) => Err(desktop_io_error("persist export", error.error.kind())),
    }
}

fn verify_existing(target: &Path, metadata: fs::Metadata, expected: &[u8]) -> Result<bool, String> {
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Creator export target exists but is not a regular file".into());
    }
    let expected_len = u64::try_from(expected.len())
        .map_err(|_| "Creator export is too large for the local filesystem".to_string())?;
    if metadata.len() != expected_len {
        return Err("Creator export target exists with different content".into());
    }
    let existing = File::open(target)
        .map_err(|error| desktop_io_error("open existing export", error.kind()))?;
    let mut bytes = Vec::with_capacity(expected.len());
    existing
        .take(expected_len.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| desktop_io_error("read existing export", error.kind()))?;
    if bytes != expected {
        return Err("Creator export target exists with different content".into());
    }
    Ok(true)
}

fn desktop_io_error(action: &str, kind: std::io::ErrorKind) -> String {
    format!("failed to {action} on the operating system Desktop: {kind}")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use onecrew_creator_runtime::{CreatorTransferFileFormat, ManualCreatorCreateInput};

    use crate::{CreatorExportInput, LocalPlatform, LocalPlatformHostPaths};

    fn xlsx_input() -> CreatorExportInput {
        CreatorExportInput {
            format: CreatorTransferFileFormat::Xlsx,
        }
    }

    #[tokio::test]
    async fn export_is_atomic_reusable_and_never_projects_the_absolute_path() {
        let data_dir = tempfile::tempdir().unwrap();
        let desktop_dir = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open_with_host_paths(
            data_dir.path(),
            LocalPlatformHostPaths::with_system_desktop(desktop_dir.path().to_path_buf()),
        )
        .unwrap();
        platform
            .creators()
            .unwrap()
            .create_manual_creator(ManualCreatorCreateInput {
                tags: None,
                display_name: "Exported Creator".into(),
                notes: Some("excluded Creator note".into()),
                contact_routes: Vec::new(),
            })
            .unwrap();

        let first = platform
            .export_creators_to_system_desktop(xlsx_input())
            .await
            .unwrap();
        assert!(!first.reused_existing);
        let second = platform
            .export_creators_to_system_desktop(xlsx_input())
            .await
            .unwrap();
        assert!(second.reused_existing);
        assert_eq!(first.file_name, second.file_name);
        let exported = fs::read(desktop_dir.path().join(&first.file_name)).unwrap();
        let bundle = onecrew_creator_runtime::CreatorBundle::from_xlsx_bytes(&exported).unwrap();
        assert_eq!(bundle.creators[0].display_name, "Exported Creator");
        assert!(!String::from_utf8_lossy(&exported).contains("excluded Creator note"));
        assert!(first.file_name.ends_with(".xlsx"));

        let result_json = serde_json::to_string(&first).unwrap();
        assert!(!result_json.contains(&desktop_dir.path().display().to_string()));
    }

    #[tokio::test]
    async fn unavailable_desktop_fails_only_the_export_capability() {
        let data_dir = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open_with_host_paths(
            data_dir.path(),
            LocalPlatformHostPaths::without_system_desktop(),
        )
        .unwrap();
        assert_eq!(
            platform
                .export_creators_to_system_desktop(xlsx_input())
                .await
                .unwrap_err()
                .to_string(),
            "the operating system Desktop directory is unavailable for Creator export"
        );
        assert!(platform.creators().is_ok());
    }

    #[tokio::test]
    async fn conflicting_existing_file_is_never_overwritten() {
        let data_dir = tempfile::tempdir().unwrap();
        let desktop_dir = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open_with_host_paths(
            data_dir.path(),
            LocalPlatformHostPaths::with_system_desktop(desktop_dir.path().to_path_buf()),
        )
        .unwrap();
        let first = platform
            .export_creators_to_system_desktop(xlsx_input())
            .await
            .unwrap();
        fs::write(desktop_dir.path().join(&first.file_name), b"different").unwrap();
        assert_eq!(
            platform
                .export_creators_to_system_desktop(xlsx_input())
                .await
                .unwrap_err()
                .to_string(),
            "Creator export target exists with different content"
        );
        assert_eq!(
            fs::read(desktop_dir.path().join(first.file_name)).unwrap(),
            b"different"
        );
    }
}
