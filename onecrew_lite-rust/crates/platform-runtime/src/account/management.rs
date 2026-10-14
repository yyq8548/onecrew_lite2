use crate::PlatformChangeScope;
use onecrew_local_domain::{LocalProfile, Platform, ProfileSecret};
use onecrew_local_vault::LocalVault;
use uuid::Uuid;

use crate::LocalPlatform;

use super::{
    ClearXChatPasscodeInput, OperatorAccountCredentialAvailability, OperatorAccountPlatform,
    OperatorAccountProjection, SetXChatPasscodeInput, project_x_chat_passcode_status,
    require_confirmation,
};

impl LocalPlatform {
    pub fn operator_account_list(&self) -> Result<Vec<OperatorAccountProjection>, String> {
        self.vault
            .list(None)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|profile| {
                let readable_secret = self.vault.get(profile.id).ok().map(|(_, secret)| secret);
                Ok(project_operator_account(profile, readable_secret.as_ref()))
            })
            .collect()
    }

    pub fn operator_account_x_chat_passcode_set(
        &self,
        input: SetXChatPasscodeInput,
    ) -> Result<OperatorAccountProjection, String> {
        if input.x_chat_passcode.trim().is_empty() {
            return Err("x_chat_passcode must be non-empty".into());
        }
        self.replace_operator_account_x_chat_passcode(
            &input.operator_account_id,
            Some(input.x_chat_passcode),
        )
    }

    pub fn operator_account_x_chat_passcode_clear(
        &self,
        input: ClearXChatPasscodeInput,
    ) -> Result<OperatorAccountProjection, String> {
        require_confirmation(input.confirmed)?;
        self.replace_operator_account_x_chat_passcode(&input.operator_account_id, None)
    }

    fn replace_operator_account_x_chat_passcode(
        &self,
        operator_account_id: &str,
        x_chat_passcode: Option<String>,
    ) -> Result<OperatorAccountProjection, String> {
        let profile_id =
            Uuid::parse_str(operator_account_id).map_err(|_| "invalid operator_account_id")?;
        let (profile, mut secret) = self
            .vault
            .get(profile_id)
            .map_err(|error| error.to_string())?;
        if profile.platform != Platform::X {
            return Err("X Chat passcode can only be changed for an X operator account".into());
        }
        if profile.provider_account_id.trim().is_empty()
            || secret.provider_account_id.trim().is_empty()
            || profile.provider_account_id != secret.provider_account_id
        {
            return Err("operator account profile identity does not match its vault secret".into());
        }

        secret.x_chat_passcode = x_chat_passcode;
        let replaced = self
            .vault
            .replace_profile(profile.id, profile.platform, profile.username, &secret)
            .map_err(|error| error.to_string())?;
        if replaced.id != profile_id || replaced.provider_account_id != secret.provider_account_id {
            self.publish_changes([PlatformChangeScope::OperatorAccounts]);
            return Err("X Chat passcode update replaced the wrong operator account".into());
        }
        self.publish_changes([PlatformChangeScope::OperatorAccounts]);
        Ok(project_operator_account(replaced, Some(&secret)))
    }

    pub(super) fn operator_account_by_id(
        &self,
        id: &str,
    ) -> Result<OperatorAccountProjection, String> {
        self.operator_account_list()?
            .into_iter()
            .find(|account| account.operator_account_id == id)
            .ok_or_else(|| "operator account not found".to_string())
    }
}

fn safe_public_account_label(
    platform: OperatorAccountPlatform,
    username: &str,
    profile_name: Option<&str>,
    account_id: &str,
) -> String {
    if platform == OperatorAccountPlatform::Telegram {
        let public_username = username.trim().trim_start_matches('@');
        if !public_username.is_empty() && !looks_like_phone_number(public_username) {
            return format!("@{public_username}");
        }
        let suffix = account_id.get(..8).unwrap_or(account_id);
        return format!("Telegram account {suffix}");
    }
    profile_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(username)
        .to_string()
}

/// One operator account's secret-free projection read straight from the
/// vault, or `None` when no profile has this id. Same projection as
/// [`LocalPlatform::operator_account_list`] for one row; it decrypts one
/// secret, so callers run it on the blocking pool.
pub(crate) fn operator_account_projection(
    vault: &LocalVault,
    profile_id: Uuid,
) -> Result<Option<OperatorAccountProjection>, String> {
    let Some(profile) = vault
        .list(None)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|profile| profile.id == profile_id)
    else {
        return Ok(None);
    };
    let readable_secret = vault.get(profile.id).ok().map(|(_, secret)| secret);
    Ok(Some(project_operator_account(
        profile,
        readable_secret.as_ref(),
    )))
}

pub(super) fn project_operator_account(
    profile: LocalProfile,
    secret: Option<&ProfileSecret>,
) -> OperatorAccountProjection {
    let x_chat_passcode_status = project_x_chat_passcode_status(profile.platform, secret);
    let platform = match profile.platform {
        Platform::X => OperatorAccountPlatform::X,
        Platform::Instagram => OperatorAccountPlatform::Instagram,
        Platform::Telegram => OperatorAccountPlatform::Telegram,
    };
    let credential_availability = if secret.is_some() {
        OperatorAccountCredentialAvailability::Available
    } else {
        OperatorAccountCredentialAvailability::Unavailable
    };
    let public_username = if platform == OperatorAccountPlatform::Telegram
        && looks_like_phone_number(&profile.username)
    {
        String::new()
    } else {
        profile.username.clone()
    };
    OperatorAccountProjection {
        operator_account_id: profile.id.to_string(),
        platform,
        provider_account_id: profile.provider_account_id.clone(),
        username: public_username,
        label: safe_public_account_label(
            platform,
            &profile.username,
            secret.map(|secret| secret.profile_name.as_str()),
            &profile.id.to_string(),
        ),
        configured: credential_availability == OperatorAccountCredentialAvailability::Available,
        credential_availability,
        x_chat_passcode_status,
        updated_at: profile.updated_at,
    }
}

fn looks_like_phone_number(value: &str) -> bool {
    let digits = value.chars().filter(char::is_ascii_digit).count();
    digits >= 7
        && value.chars().all(|character| {
            character.is_ascii_digit() || matches!(character, '+' | '-' | ' ' | '(' | ')')
        })
}
