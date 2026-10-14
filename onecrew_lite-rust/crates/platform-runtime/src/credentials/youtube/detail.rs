use std::time::{SystemTime, UNIX_EPOCH};

use crate::PlatformChangeScope;
use crate::credentials::services::{YoutubeCredentialTest, YoutubeCredentialTestError};
use onecrew_local_domain::{ServiceCredential, ServiceProvider};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::LocalPlatform;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct YoutubePlatformGetInput {
    pub credential_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct YoutubePlatformTestInput {
    pub credential_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct YoutubePlatformTestResult {
    pub credential_id: String,
    pub ok: bool,
    pub checked_at: u64,
    pub channel_id: Option<String>,
    pub issue: Option<YoutubeCredentialRejectionIssue>,
    pub message: String,
}

pub use crate::credentials::services::YoutubeCredentialRejectionIssue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YoutubePlatformTestError {
    InvalidCredentialId,
    CredentialUnavailable,
    RateLimited,
    /// YouTube answered with a server error or an unexpected missing channel.
    Transient,
    /// YouTube could not be reached: no HTTP response.
    NetworkUnreachable,
    AmbiguousForbidden,
    Infrastructure,
}

impl YoutubePlatformTestError {
    pub const fn safe_message(self) -> &'static str {
        match self {
            Self::InvalidCredentialId => "credential_id is invalid",
            Self::CredentialUnavailable => "YouTube credential is unavailable",
            Self::RateLimited => "YouTube credential validation is rate limited",
            // The Web credential test reports both as temporarily unavailable;
            // the typed connection check keeps them apart.
            Self::Transient | Self::NetworkUnreachable => {
                "YouTube credential validation is temporarily unavailable"
            }
            Self::AmbiguousForbidden => "YouTube returned an ambiguous forbidden response",
            Self::Infrastructure => "YouTube credential validation is unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum YoutubeCredentialAvailability {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum YoutubeCredentialIssue {
    InvalidMetadata,
    CredentialUnreadable,
    Disabled,
    CoolingDown,
    RecordedFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct YoutubePlatformDetail {
    pub credential_id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub availability: YoutubeCredentialAvailability,
    pub issues: Vec<YoutubeCredentialIssue>,
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_until: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<u64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum YoutubePlatformGetOutcome {
    Found { detail: YoutubePlatformDetail },
    NotFound { credential_id: String },
}

impl LocalPlatform {
    /// Pure local read of one YouTube credential. Validation and Creator
    /// reconciliation remain explicit lifecycle operations.
    pub fn platform_youtube_get(
        &self,
        input: YoutubePlatformGetInput,
    ) -> Result<YoutubePlatformGetOutcome, String> {
        let Ok(credential_id) = Uuid::parse_str(&input.credential_id) else {
            return Ok(YoutubePlatformGetOutcome::NotFound {
                credential_id: input.credential_id,
            });
        };
        let credential = self
            .vault
            .list_service_credentials(ServiceProvider::Youtube, false)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|credential| credential.id == credential_id);
        Ok(match credential {
            Some(credential) => YoutubePlatformGetOutcome::Found {
                detail: project_detail(
                    credential,
                    self.vault.get_service_credential(credential_id).is_ok(),
                ),
            },
            None => YoutubePlatformGetOutcome::NotFound {
                credential_id: input.credential_id,
            },
        })
    }

    /// Performs a provider read probe, then reconciles and publishes the
    /// resulting local credential health. This is not a pure read even though
    /// it performs no provider write.
    pub async fn platform_youtube_test(
        &self,
        input: YoutubePlatformTestInput,
    ) -> Result<YoutubePlatformTestResult, YoutubePlatformTestError> {
        let result = self
            .credential_services
            .test_youtube_credential(&input.credential_id)
            .await
            .map(project_test_result)
            .map_err(map_test_error);
        let reconcile = self.reconcile_vault_youtube_refs_blocking().await;
        self.finish_youtube_platform_test(result, reconcile)
    }

    pub(super) fn finish_youtube_platform_test(
        &self,
        result: Result<YoutubePlatformTestResult, YoutubePlatformTestError>,
        reconcile: Result<Vec<ServiceCredential>, String>,
    ) -> Result<YoutubePlatformTestResult, YoutubePlatformTestError> {
        // These provider outcomes follow a completed health write. Local
        // admission/credential failures do not establish a vault change.
        if result.is_ok()
            || matches!(
                &result,
                Err(YoutubePlatformTestError::RateLimited
                    | YoutubePlatformTestError::Transient
                    | YoutubePlatformTestError::NetworkUnreachable
                    | YoutubePlatformTestError::AmbiguousForbidden)
            )
        {
            self.publish_changes([PlatformChangeScope::ServiceCredentials]);
        }
        match (result, reconcile) {
            (Ok(value), Ok(_)) => Ok(value),
            (Ok(_), Err(_)) => Err(YoutubePlatformTestError::Infrastructure),
            (Err(error), Ok(_)) => Err(error),
            (Err(_), Err(_)) => Err(YoutubePlatformTestError::Infrastructure),
        }
    }
}

fn project_test_result(result: YoutubeCredentialTest) -> YoutubePlatformTestResult {
    YoutubePlatformTestResult {
        credential_id: result.credential_id,
        ok: result.ok,
        checked_at: result.checked_at,
        channel_id: result.channel_id,
        issue: result.issue,
        message: result.message,
    }
}

fn map_test_error(error: YoutubeCredentialTestError) -> YoutubePlatformTestError {
    match error {
        YoutubeCredentialTestError::InvalidCredentialId => {
            YoutubePlatformTestError::InvalidCredentialId
        }
        YoutubeCredentialTestError::CredentialUnavailable => {
            YoutubePlatformTestError::CredentialUnavailable
        }
        YoutubeCredentialTestError::RateLimited => YoutubePlatformTestError::RateLimited,
        YoutubeCredentialTestError::Transient => YoutubePlatformTestError::Transient,
        YoutubeCredentialTestError::NetworkUnreachable => {
            YoutubePlatformTestError::NetworkUnreachable
        }
        YoutubeCredentialTestError::AmbiguousForbidden => {
            YoutubePlatformTestError::AmbiguousForbidden
        }
        YoutubeCredentialTestError::Infrastructure => YoutubePlatformTestError::Infrastructure,
    }
}

fn project_detail(
    credential: ServiceCredential,
    credential_is_readable: bool,
) -> YoutubePlatformDetail {
    debug_assert_eq!(credential.provider, ServiceProvider::Youtube);
    let (project, metadata_is_valid) = project_from_metadata(&credential);
    let mut issues = Vec::new();
    if !metadata_is_valid {
        issues.push(YoutubeCredentialIssue::InvalidMetadata);
    }
    if !credential_is_readable {
        issues.push(YoutubeCredentialIssue::CredentialUnreadable);
    }
    if credential.disabled {
        issues.push(YoutubeCredentialIssue::Disabled);
    }
    if credential
        .cooldown_until
        .is_some_and(|until| until > unix_timestamp())
    {
        issues.push(YoutubeCredentialIssue::CoolingDown);
    }
    if credential.last_error.is_some() {
        issues.push(YoutubeCredentialIssue::RecordedFailure);
    }
    let availability = if issues.is_empty() {
        YoutubeCredentialAvailability::Available
    } else {
        YoutubeCredentialAvailability::Unavailable
    };
    YoutubePlatformDetail {
        credential_id: credential.id.to_string(),
        label: credential.label,
        project,
        availability,
        issues,
        disabled: credential.disabled,
        cooldown_until: credential.cooldown_until,
        last_checked_at: credential.last_used_at,
        created_at: credential.created_at,
        updated_at: credential.updated_at,
    }
}

fn project_from_metadata(credential: &ServiceCredential) -> (Option<String>, bool) {
    let api_is_valid = credential
        .metadata
        .get("api")
        .and_then(serde_json::Value::as_str)
        == Some("youtube_data_api_v3");
    let (project, project_is_valid) = match credential.metadata.get("project") {
        None | Some(serde_json::Value::Null) => (None, true),
        Some(serde_json::Value::String(value)) => (
            Some(value.trim().to_owned()).filter(|value| !value.is_empty()),
            true,
        ),
        Some(_) => (None, false),
    };
    (project, api_is_valid && project_is_valid)
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use onecrew_creator_runtime::AccountRefId;
    use onecrew_local_domain::{ServiceCredentialSecret, ServiceProvider};
    use onecrew_local_vault::{EmailSecretRecordKind, EmailSecretRecordSecret};
    use rusqlite::{Connection, params};
    use schemars::schema_for;
    use tokio::sync::broadcast::error::TryRecvError;

    use super::*;

    const SECRET: &str = "YOUTUBE_DETAIL_SECRET_MUST_NOT_LEAK";

    fn insert_youtube(
        platform: &LocalPlatform,
        label: &str,
        metadata: serde_json::Value,
    ) -> ServiceCredential {
        platform
            .vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                label,
                metadata,
                &ServiceCredentialSecret::new(SECRET),
            )
            .unwrap()
    }

    #[test]
    fn youtube_get_is_a_pure_safe_found_read() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let credential = insert_youtube(
            &platform,
            "youtube-primary",
            serde_json::json!({"api": "youtube_data_api_v3", "project": "Discovery"}),
        );
        let account_ref_id = AccountRefId::new(credential.id.to_string());
        let mut changes = platform.subscribe_changes();

        let outcome = platform
            .platform_youtube_get(YoutubePlatformGetInput {
                credential_id: credential.id.to_string(),
            })
            .unwrap();
        let YoutubePlatformGetOutcome::Found { detail } = outcome else {
            panic!("expected found YouTube detail");
        };
        assert_eq!(detail.project.as_deref(), Some("Discovery"));
        assert_eq!(
            detail.availability,
            YoutubeCredentialAvailability::Available
        );
        assert!(detail.issues.is_empty());
        let wire = serde_json::to_string(&detail).unwrap();
        assert!(!wire.contains(SECRET));
        assert!(!wire.contains("metadata"));
        assert!(!wire.contains("last_error"));
        assert!(platform.creators.get_account_ref(&account_ref_id).is_err());
        assert!(matches!(changes.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn youtube_get_keeps_invalid_metadata_as_found_but_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let credential = insert_youtube(
            &platform,
            "invalid-shape",
            serde_json::json!({"api": "legacy", "project": 7}),
        );
        let outcome = platform
            .platform_youtube_get(YoutubePlatformGetInput {
                credential_id: credential.id.to_string(),
            })
            .unwrap();
        let YoutubePlatformGetOutcome::Found { detail } = outcome else {
            panic!("invalid metadata must not become not_found");
        };
        assert_eq!(
            detail.availability,
            YoutubeCredentialAvailability::Unavailable
        );
        assert_eq!(detail.issues, vec![YoutubeCredentialIssue::InvalidMetadata]);
        assert_eq!(detail.project, None);
    }

    #[test]
    fn youtube_get_projects_unreadable_disabled_cooling_and_failure_states() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();

        let unreadable = insert_youtube(
            &platform,
            "unreadable",
            serde_json::json!({"api": "youtube_data_api_v3"}),
        );
        Connection::open(directory.path().join("agent-platform.db"))
            .unwrap()
            .execute(
                "UPDATE service_credentials SET encrypted_secret = ?2 WHERE id = ?1",
                params![unreadable.id.to_string(), vec![0_u8, 1_u8, 2_u8]],
            )
            .unwrap();
        let YoutubePlatformGetOutcome::Found { detail } = platform
            .platform_youtube_get(YoutubePlatformGetInput {
                credential_id: unreadable.id.to_string(),
            })
            .unwrap()
        else {
            panic!("unreadable credential must remain found");
        };
        assert!(
            detail
                .issues
                .contains(&YoutubeCredentialIssue::CredentialUnreadable)
        );

        let disabled = insert_youtube(
            &platform,
            "disabled",
            serde_json::json!({"api": "youtube_data_api_v3"}),
        );
        platform
            .vault
            .mark_service_credential_failure(disabled.id, "sanitized failure", true, None)
            .unwrap();
        let cooling = insert_youtube(
            &platform,
            "cooling",
            serde_json::json!({"api": "youtube_data_api_v3"}),
        );
        platform
            .vault
            .mark_service_credential_failure(
                cooling.id,
                "sanitized quota failure",
                false,
                Some(u64::MAX),
            )
            .unwrap();

        let detail = get_found(&platform, disabled.id);
        assert!(detail.issues.contains(&YoutubeCredentialIssue::Disabled));
        assert!(
            detail
                .issues
                .contains(&YoutubeCredentialIssue::RecordedFailure)
        );
        let detail = get_found(&platform, cooling.id);
        assert!(detail.issues.contains(&YoutubeCredentialIssue::CoolingDown));
        assert!(
            detail
                .issues
                .contains(&YoutubeCredentialIssue::RecordedFailure)
        );
        let wire = serde_json::to_string(&detail).unwrap();
        assert!(!wire.contains("sanitized quota failure"));
    }

    #[test]
    fn youtube_get_wrong_provider_invalid_id_and_missing_id_are_not_found() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let resend = platform
            .vault
            .upsert_email_secret_record(
                EmailSecretRecordKind::ResendApiKey,
                "mailbox",
                serde_json::json!({"email_address": "team@example.com"}),
                &EmailSecretRecordSecret::new("RESEND_SECRET"),
            )
            .unwrap();
        for credential_id in [
            resend.id.to_string(),
            Uuid::new_v4().to_string(),
            "not-a-uuid".into(),
        ] {
            assert_eq!(
                platform
                    .platform_youtube_get(YoutubePlatformGetInput {
                        credential_id: credential_id.clone(),
                    })
                    .unwrap(),
                YoutubePlatformGetOutcome::NotFound { credential_id }
            );
        }
    }

    #[test]
    fn youtube_get_contract_is_strict_and_credential_specific() {
        assert!(
            serde_json::from_value::<YoutubePlatformGetInput>(serde_json::json!({
                "credential_id": "youtube-1",
                "provider": "youtube"
            }))
            .is_err()
        );
        let schema = serde_json::to_value(schema_for!(YoutubePlatformDetail)).unwrap();
        let properties = schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .unwrap();
        for forbidden in [
            "provider",
            "account_ref_id",
            "metadata",
            "last_error",
            "secret",
        ] {
            assert!(!properties.contains_key(forbidden));
        }
        assert!(properties.contains_key("credential_id"));
    }

    #[test]
    fn youtube_test_contract_is_strict_secret_free_and_wire_compatible() {
        assert!(
            serde_json::from_value::<YoutubePlatformTestInput>(serde_json::json!({
                "credential_id": "youtube-1",
                "api_key": SECRET
            }))
            .is_err()
        );
        let result = project_test_result(YoutubeCredentialTest {
            credential_id: "youtube-1".into(),
            ok: true,
            checked_at: 42,
            channel_id: Some("UC-safe".into()),
            issue: None,
            message: "valid".into(),
        });
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            serde_json::json!({
                "credential_id": "youtube-1",
                "ok": true,
                "checked_at": 42,
                "channel_id": "UC-safe",
                "issue": null,
                "message": "valid"
            })
        );
        let schema = serde_json::to_value(schema_for!(YoutubePlatformTestResult)).unwrap();
        let properties = schema["properties"].as_object().unwrap();
        assert!(properties.get("api_key").is_none());
        assert!(properties.get("metadata").is_none());
        assert!(properties.get("last_error").is_none());
        assert!(
            serde_json::to_string(&schema)
                .unwrap()
                .contains("api_key_invalid")
        );
    }

    fn get_found(platform: &LocalPlatform, credential_id: Uuid) -> YoutubePlatformDetail {
        match platform
            .platform_youtube_get(YoutubePlatformGetInput {
                credential_id: credential_id.to_string(),
            })
            .unwrap()
        {
            YoutubePlatformGetOutcome::Found { detail } => detail,
            YoutubePlatformGetOutcome::NotFound { .. } => panic!("expected found detail"),
        }
    }
}
