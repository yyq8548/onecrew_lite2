use chat_xdk_core::SigningKeyEntry;
use serde_json::Value;

use crate::XChatError;

#[derive(Debug, Clone)]
pub struct ChatKeyMaterial {
    pub signing_key_version: String,
    pub juicebox_config: Value,
    pub signing_keys: Vec<SigningKeyEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XChatDmPermissions {
    pub can_dm: bool,
    pub dm_blocking: bool,
    pub passes_premium_check: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XChatDmCapability {
    ExistingConversation,
    NewMessageRequestAllowed,
    TargetDmBlocking,
    PlatformDmNotAllowed,
    PremiumRequired,
}

impl XChatDmPermissions {
    pub fn from_public_keys_response(
        response: &Value,
        target_user_id: &str,
    ) -> Result<Self, XChatError> {
        let rows = response
            .pointer("/data/user_results_by_rest_ids")
            .and_then(Value::as_array)
            .ok_or_else(|| XChatError::DmPermission("missing user permission rows".into()))?;
        let row = rows
            .iter()
            .find(|row| {
                text(row.get("rest_id")) == target_user_id
                    || text(row.pointer("/result/rest_id")) == target_user_id
            })
            .ok_or_else(|| {
                XChatError::DmPermission(format!(
                    "no permission result for target user {target_user_id}"
                ))
            })?;
        let permissions = row
            .pointer("/result/chat_permissions")
            .ok_or_else(|| XChatError::DmPermission("missing chat_permissions".into()))?;

        Ok(Self {
            can_dm: required_bool(permissions, "can_dm")?,
            dm_blocking: required_bool(permissions, "dm_blocking")?,
            passes_premium_check: required_bool(permissions, "passes_premium_check")?,
        })
    }

    pub fn ensure_message_request_allowed(self) -> Result<(), XChatError> {
        match self.message_request_capability() {
            XChatDmCapability::NewMessageRequestAllowed => Ok(()),
            XChatDmCapability::TargetDmBlocking => Err(XChatError::DmNotAllowed(
                "the target account is blocking direct messages".into(),
            )),
            XChatDmCapability::PlatformDmNotAllowed => Err(XChatError::DmNotAllowed(
                "X does not allow this account to receive a direct message".into(),
            )),
            XChatDmCapability::PremiumRequired => Err(XChatError::PremiumRequired),
            XChatDmCapability::ExistingConversation => Ok(()),
        }
    }

    pub fn message_request_capability(self) -> XChatDmCapability {
        if self.dm_blocking {
            XChatDmCapability::TargetDmBlocking
        } else if !self.can_dm {
            XChatDmCapability::PlatformDmNotAllowed
        } else if !self.passes_premium_check {
            XChatDmCapability::PremiumRequired
        } else {
            XChatDmCapability::NewMessageRequestAllowed
        }
    }
}

impl ChatKeyMaterial {
    pub fn from_public_keys_response(
        response: &Value,
        self_user_id: &str,
    ) -> Result<Self, XChatError> {
        let rows = response
            .pointer("/data/user_results_by_rest_ids")
            .and_then(Value::as_array)
            .ok_or_else(|| XChatError::KeyMaterial("missing user key rows".into()))?;
        let mut signing_keys = Vec::new();
        let mut signing_key_version = None;
        let mut juicebox_config = None;

        for row in rows {
            let user_id =
                text(row.get("rest_id")).or_else_text(|| text(row.pointer("/result/rest_id")));
            let Some(items) = row
                .pointer("/result/get_public_keys/public_keys_with_token_map")
                .and_then(Value::as_array)
            else {
                continue;
            };
            for item in items {
                let metadata = item.get("public_key_with_metadata").unwrap_or(&Value::Null);
                let public_key = metadata.get("public_key").unwrap_or(&Value::Null);
                let version = text(metadata.get("version"));
                let entry = SigningKeyEntry {
                    user_id: user_id.clone(),
                    public_key_version: version.clone(),
                    public_key: text(public_key.get("signing_public_key")),
                    identity_public_key: text(public_key.get("public_key")),
                    identity_public_key_signature: text(
                        public_key.get("identity_public_key_signature"),
                    ),
                };
                if complete(&entry) {
                    signing_keys.push(entry);
                }
                if user_id == self_user_id && !version.is_empty() {
                    signing_key_version = Some(version);
                    juicebox_config = item.get("token_map").cloned();
                }
            }
        }

        Ok(Self {
            signing_key_version: signing_key_version.ok_or_else(|| {
                XChatError::KeyMaterial("current user has no signing-key version".into())
            })?,
            juicebox_config: juicebox_config.ok_or_else(|| {
                XChatError::KeyMaterial("current user has no Juicebox token map".into())
            })?,
            signing_keys,
        })
    }
}

fn complete(entry: &SigningKeyEntry) -> bool {
    !entry.user_id.is_empty()
        && !entry.public_key_version.is_empty()
        && !entry.public_key.is_empty()
        && !entry.identity_public_key.is_empty()
        && !entry.identity_public_key_signature.is_empty()
}

fn text(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn required_bool(value: &Value, field: &str) -> Result<bool, XChatError> {
    value
        .get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| XChatError::DmPermission(format!("chat_permissions.{field} is missing")))
}

trait StringFallback {
    fn or_else_text(self, fallback: impl FnOnce() -> String) -> String;
}

impl StringFallback for String {
    fn or_else_text(self, fallback: impl FnOnce() -> String) -> String {
        if self.is_empty() { fallback() } else { self }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn extracts_current_identity_and_participant_signing_keys() {
        let response = json!({
            "data": {"user_results_by_rest_ids": [{
                "rest_id": "42",
                "result": {"get_public_keys": {"public_keys_with_token_map": [{
                    "public_key_with_metadata": {
                        "version": "7",
                        "public_key": {
                            "signing_public_key": "sign",
                            "public_key": "identity",
                            "identity_public_key_signature": "signature"
                        }
                    },
                    "token_map": {"key_store_token_map_json": "{}", "token_map": []}
                }]}}
            }]}
        });
        let material = ChatKeyMaterial::from_public_keys_response(&response, "42").unwrap();
        assert_eq!(material.signing_key_version, "7");
        assert_eq!(material.signing_keys.len(), 1);
    }

    #[test]
    fn extracts_message_request_permissions_for_target() {
        let response = json!({
            "data": {"user_results_by_rest_ids": [{
                "rest_id": "365968174",
                "result": {"chat_permissions": {
                    "can_dm": true,
                    "can_dm_on_xchat": false,
                    "dm_blocking": false,
                    "passes_premium_check": true
                }}
            }]}
        });
        let permissions =
            XChatDmPermissions::from_public_keys_response(&response, "365968174").unwrap();
        assert_eq!(
            permissions,
            XChatDmPermissions {
                can_dm: true,
                dm_blocking: false,
                passes_premium_check: true,
            }
        );
        permissions.ensure_message_request_allowed().unwrap();
    }

    #[test]
    fn rejects_target_that_does_not_pass_premium_check() {
        let response = json!({
            "data": {"user_results_by_rest_ids": [{
                "rest_id": "7",
                "result": {"chat_permissions": {
                    "can_dm": true,
                    "dm_blocking": false,
                    "passes_premium_check": false
                }}
            }]}
        });
        let error = XChatDmPermissions::from_public_keys_response(&response, "7")
            .unwrap()
            .ensure_message_request_allowed()
            .unwrap_err();
        assert!(matches!(error, XChatError::PremiumRequired));
    }

    #[test]
    fn permission_signals_map_to_stable_message_request_capabilities() {
        assert_eq!(
            XChatDmPermissions {
                can_dm: true,
                dm_blocking: true,
                passes_premium_check: true,
            }
            .message_request_capability(),
            XChatDmCapability::TargetDmBlocking
        );
        assert_eq!(
            XChatDmPermissions {
                can_dm: false,
                dm_blocking: false,
                passes_premium_check: true,
            }
            .message_request_capability(),
            XChatDmCapability::PlatformDmNotAllowed
        );
    }
}
