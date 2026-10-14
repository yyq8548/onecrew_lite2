use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::resolve_exact_operator_account;
use crate::{
    LocalPlatform, OperatorAccountCredentialAvailability, OperatorAccountPlatform,
    OperatorAccountProjection,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TelegramPlatformGetInput {
    pub operator_account_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TelegramPlatformDetail {
    pub operator_account_id: String,
    pub provider_account_id: String,
    pub username: String,
    pub label: String,
    pub configured: bool,
    pub credential_availability: OperatorAccountCredentialAvailability,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum TelegramPlatformGetOutcome {
    Found { detail: TelegramPlatformDetail },
    NotFound { operator_account_id: String },
}

impl LocalPlatform {
    pub fn platform_telegram_get(
        &self,
        input: TelegramPlatformGetInput,
    ) -> Result<TelegramPlatformGetOutcome, String> {
        Ok(
            match resolve_exact_operator_account(
                self,
                &input.operator_account_id,
                OperatorAccountPlatform::Telegram,
            )? {
                Some(account) => TelegramPlatformGetOutcome::Found {
                    detail: project_detail(account),
                },
                None => TelegramPlatformGetOutcome::NotFound {
                    operator_account_id: input.operator_account_id,
                },
            },
        )
    }
}

fn project_detail(account: OperatorAccountProjection) -> TelegramPlatformDetail {
    debug_assert_eq!(account.platform, OperatorAccountPlatform::Telegram);
    TelegramPlatformDetail {
        operator_account_id: account.operator_account_id,
        provider_account_id: account.provider_account_id,
        username: account.username,
        label: account.label,
        configured: account.configured,
        credential_availability: account.credential_availability,
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
            provider_account_id: "tg-provider-1".into(),
            cookies: Vec::new(),
            x_chat_passcode: Some("UNRELATED_X_SECRET".into()),
            instagram_session_settings: None,
            telegram_api_id: Some(12345),
            telegram_api_hash: Some("TG_API_HASH_SECRET".into()),
            telegram_session: Some("TG_SESSION_SECRET".into()),
            profile_name: "Telegram primary".into(),
            browser_profile_dir: String::new(),
            fingerprint_seed: String::new(),
            proxy: None,
            proxy_egress: None,
        }
    }

    #[test]
    fn telegram_get_projects_found_without_x_or_secret_fields() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let profile = platform
            .vault
            .upsert(Platform::Telegram, "river_tg", &secret())
            .unwrap();
        let outcome = platform
            .platform_telegram_get(TelegramPlatformGetInput {
                operator_account_id: profile.id.to_string(),
            })
            .unwrap();
        let TelegramPlatformGetOutcome::Found { detail } = outcome else {
            panic!("expected found Telegram detail");
        };
        let wire = serde_json::to_string(&detail).unwrap();
        assert_eq!(detail.provider_account_id, "tg-provider-1");
        assert!(!wire.contains("x_chat"));
        assert!(!wire.contains("TG_API_HASH_SECRET"));
        assert!(!wire.contains("TG_SESSION_SECRET"));
    }

    #[test]
    fn telegram_detail_can_project_a_found_unreadable_account() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let profile = platform
            .vault
            .upsert(Platform::Telegram, "river_tg", &secret())
            .unwrap();
        Connection::open(directory.path().join("agent-platform.db"))
            .unwrap()
            .execute(
                "UPDATE profiles SET encrypted_secret = ?2 WHERE id = ?1",
                params![profile.id.to_string(), vec![0_u8, 1_u8, 2_u8]],
            )
            .unwrap();
        let TelegramPlatformGetOutcome::Found { detail } = platform
            .platform_telegram_get(TelegramPlatformGetInput {
                operator_account_id: profile.id.to_string(),
            })
            .unwrap()
        else {
            panic!("unreadable Telegram account must remain found");
        };
        assert!(!detail.configured);
        assert_eq!(
            detail.credential_availability,
            OperatorAccountCredentialAvailability::Unavailable
        );
    }

    #[test]
    fn telegram_get_missing_and_wrong_platform_are_not_found_without_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let x = platform
            .vault
            .upsert(Platform::X, "river", &secret())
            .unwrap();
        for operator_account_id in [x.id.to_string(), Uuid::new_v4().to_string()] {
            assert_eq!(
                platform
                    .platform_telegram_get(TelegramPlatformGetInput {
                        operator_account_id: operator_account_id.clone(),
                    })
                    .unwrap(),
                TelegramPlatformGetOutcome::NotFound {
                    operator_account_id
                }
            );
        }
    }

    #[test]
    fn telegram_get_contract_is_strict_and_has_no_x_fields() {
        assert!(
            serde_json::from_value::<TelegramPlatformGetInput>(serde_json::json!({
                "operator_account_id": "tg-1",
                "x_chat_passcode_status": "configured"
            }))
            .is_err()
        );
        let schema = serde_json::to_string(&schema_for!(TelegramPlatformDetail)).unwrap();
        assert!(!schema.contains("x_chat"));
        assert!(!schema.contains("connection_id"));
        assert!(!schema.contains("metadata"));
    }
}
