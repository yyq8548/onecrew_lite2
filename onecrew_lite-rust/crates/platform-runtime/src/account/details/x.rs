use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::resolve_exact_operator_account;
use crate::{
    LocalPlatform, OperatorAccountCredentialAvailability, OperatorAccountPlatform,
    OperatorAccountProjection, XChatPasscodeStatus,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct XPlatformGetInput {
    pub operator_account_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct XPlatformDetail {
    pub operator_account_id: String,
    pub provider_account_id: String,
    pub username: String,
    pub label: String,
    pub configured: bool,
    pub credential_availability: OperatorAccountCredentialAvailability,
    pub x_chat_passcode_status: XChatPasscodeStatus,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum XPlatformGetOutcome {
    Found { detail: XPlatformDetail },
    NotFound { operator_account_id: String },
}

impl LocalPlatform {
    pub fn platform_x_get(&self, input: XPlatformGetInput) -> Result<XPlatformGetOutcome, String> {
        Ok(
            match resolve_exact_operator_account(
                self,
                &input.operator_account_id,
                OperatorAccountPlatform::X,
            )? {
                Some(account) => XPlatformGetOutcome::Found {
                    detail: project_detail(account),
                },
                None => XPlatformGetOutcome::NotFound {
                    operator_account_id: input.operator_account_id,
                },
            },
        )
    }
}

fn project_detail(account: OperatorAccountProjection) -> XPlatformDetail {
    debug_assert_eq!(account.platform, OperatorAccountPlatform::X);
    XPlatformDetail {
        operator_account_id: account.operator_account_id,
        provider_account_id: account.provider_account_id,
        username: account.username,
        label: account.label,
        configured: account.configured,
        credential_availability: account.credential_availability,
        x_chat_passcode_status: account.x_chat_passcode_status,
        updated_at: account.updated_at,
    }
}

#[cfg(test)]
mod tests {
    use onecrew_local_domain::{Platform, ProfileSecret};
    use rusqlite::{Connection, params};
    use schemars::schema_for;
    use uuid::Uuid;

    use super::*;

    fn secret() -> ProfileSecret {
        ProfileSecret {
            provider_account_id: "x-provider-1".into(),
            cookies: Vec::new(),
            x_chat_passcode: Some("X_CHAT_SECRET_MUST_NOT_LEAK".into()),
            instagram_session_settings: None,
            telegram_api_id: None,
            telegram_api_hash: None,
            telegram_session: None,
            profile_name: "X primary".into(),
            browser_profile_dir: String::new(),
            fingerprint_seed: String::new(),
            proxy: None,
            proxy_egress: None,
        }
    }

    #[test]
    fn x_get_projects_found_and_never_serializes_secret_material() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let profile = platform
            .vault
            .upsert(Platform::X, "river", &secret())
            .unwrap();

        let outcome = platform
            .platform_x_get(XPlatformGetInput {
                operator_account_id: profile.id.to_string(),
            })
            .unwrap();
        let XPlatformGetOutcome::Found { detail } = outcome else {
            panic!("expected found X detail");
        };
        assert_eq!(detail.provider_account_id, "x-provider-1");
        assert_eq!(
            detail.x_chat_passcode_status,
            XChatPasscodeStatus::Configured
        );
        let wire = serde_json::to_string(&detail).unwrap();
        assert!(!wire.contains("X_CHAT_SECRET_MUST_NOT_LEAK"));
        assert!(!wire.contains("cookies"));
    }

    #[test]
    fn x_get_retains_a_found_but_unreadable_account_as_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let profile = platform
            .vault
            .upsert(Platform::X, "river", &secret())
            .unwrap();
        Connection::open(directory.path().join("agent-platform.db"))
            .unwrap()
            .execute(
                "UPDATE profiles SET encrypted_secret = ?2 WHERE id = ?1",
                params![profile.id.to_string(), vec![0_u8, 1_u8, 2_u8]],
            )
            .unwrap();
        let XPlatformGetOutcome::Found { detail } = platform
            .platform_x_get(XPlatformGetInput {
                operator_account_id: profile.id.to_string(),
            })
            .unwrap()
        else {
            panic!("unreadable X account must remain found");
        };
        assert!(!detail.configured);
        assert_eq!(
            detail.credential_availability,
            OperatorAccountCredentialAvailability::Unavailable
        );
        assert_eq!(
            detail.x_chat_passcode_status,
            XChatPasscodeStatus::Unavailable
        );
    }

    #[test]
    fn x_get_missing_and_wrong_platform_are_the_same_safe_not_found() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let instagram = platform
            .vault
            .upsert(Platform::Instagram, "river_ig", &secret())
            .unwrap();
        for operator_account_id in [instagram.id.to_string(), Uuid::new_v4().to_string()] {
            assert_eq!(
                platform
                    .platform_x_get(XPlatformGetInput {
                        operator_account_id: operator_account_id.clone(),
                    })
                    .unwrap(),
                XPlatformGetOutcome::NotFound {
                    operator_account_id
                }
            );
        }
    }

    #[test]
    fn x_get_contract_is_strict_and_x_specific() {
        assert!(
            serde_json::from_value::<XPlatformGetInput>(serde_json::json!({
                "operator_account_id": "x-1",
                "platform": "x"
            }))
            .is_err()
        );
        let schema = serde_json::to_string(&schema_for!(XPlatformDetail)).unwrap();
        assert!(schema.contains("x_chat_passcode_status"));
        assert!(!schema.contains("connection_id"));
        assert!(!schema.contains("metadata"));
    }
}
