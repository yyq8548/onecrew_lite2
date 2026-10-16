use super::CredentialServices;
use crate::mail_access::{check_identity, identity, mailbox_id};
use onecrew_access_client::mail::{MailAccessToken, MailClient};
use onecrew_creator_runtime::{EmailProviderKind, MailboxProvider, MailboxRefId};
use onecrew_email_contracts::EmailCapability;

pub(in crate::credentials) struct GmailSession {
    pub(in crate::credentials) platform: onecrew_email::GmailPlatform,
    pub(in crate::credentials) token: MailAccessToken,
    pub(in crate::credentials) email: String,
}
impl CredentialServices {
    pub(crate) async fn gmail_connection_probe(&self, id: &MailboxRefId) -> Result<Option<String>, String> {
        let Some(mailbox) = self.gmail_mailbox(id, EmailCapability::Inbox).await? else {
            return Ok(None);
        };
        mailbox.platform.inbox(mailbox.token.expose_secret(), None, 1).await?;
        Ok(Some(mailbox.email))
    }
    pub(in crate::credentials) async fn gmail_mailbox(
        &self,
        id: &MailboxRefId,
        capability: EmailCapability,
    ) -> Result<Option<GmailSession>, String> {
        let binding = self
            .mailbox_resolver()?
            .resolve_mailbox_auth_connection(id)
            .await?;
        if binding.provider != MailboxProvider::Gmail {
            return Ok(None);
        }
        let required = match capability {
            EmailCapability::Send => onecrew_creator_runtime::EmailCapability::Send,
            EmailCapability::SentReadback => onecrew_creator_runtime::EmailCapability::SentReadback,
            EmailCapability::Inbox => onecrew_creator_runtime::EmailCapability::Inbox,
            _ => return Err("gmail_capability_unavailable".into()),
        };
        if !binding.available
            || !binding.auth_connection.available
            || !binding.capabilities.contains(&required)
            || binding.auth_connection.provider_kind != EmailProviderKind::Remote
        {
            return Err("gmail_mailbox_unavailable".into());
        }
        let current = identity(self.vault.clone()).await?;
        let prefix = format!("gmail:{}:", current.user());
        let connection_id = binding
            .auth_connection
            .auth_connection_id
            .as_str()
            .strip_prefix(&prefix)
            .ok_or("gmail_account_mismatch")?;
        if mailbox_id(current.user(), connection_id) != id.as_str()
            || binding.auth_connection.secret_record_id.as_str()
                != format!("access:{}:{connection_id}", current.user())
        {
            return Err("gmail_account_mismatch".into());
        }
        let client = MailClient::new(onecrew_creator_sync::production_access_origin())
            .map_err(|e| e.code().to_owned())?;
        let connections = client
            .connections(current.bearer())
            .await
            .map_err(|e| e.code().to_owned())?;
        let connection = connections
            .iter()
            .find(|c| c.id == connection_id)
            .ok_or("gmail_connection_missing")?;
        if connection.provider != "google"
            || connection.status != "connected"
            || !connection
                .email
                .eq_ignore_ascii_case(&binding.email_address)
            || !connection.capabilities.can_send
            || !connection.capabilities.can_read_thread
        {
            return Err("gmail_authorization_required".into());
        }
        let token = client
            .access_token(current.bearer(), connection_id)
            .await
            .map_err(|e| e.code().to_owned())?;
        validate_token(
            &token,
            &binding.email_address,
            chrono::Utc::now().timestamp(),
        )?;
        check_identity(self.vault.clone(), &current).await?;
        Ok(Some(GmailSession {
            platform: onecrew_email::GmailPlatform::new()?,
            token,
            email: binding.email_address,
        }))
    }
}

fn validate_token(token: &MailAccessToken, email: &str, now: i64) -> Result<(), String> {
    if !token.email.eq_ignore_ascii_case(email)
        || token.expose_secret().is_empty()
        || !token
            .scopes
            .iter()
            .any(|s| s == "https://www.googleapis.com/auth/gmail.send")
        || !token
            .scopes
            .iter()
            .any(|s| s == "https://www.googleapis.com/auth/gmail.readonly")
        || chrono::DateTime::parse_from_rfc3339(&token.expires_at)
            .map_err(|_| "gmail_token_invalid")?
            .timestamp()
            <= now + 30
    {
        return Err("gmail_authorization_required".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_admission_rejects_wrong_account_expiry_and_partial_consent() {
        let mut token: MailAccessToken = serde_json::from_value(serde_json::json!({
            "access_token":"fixture-only", "email":"sender@example.com", "expires_at":"2030-01-01T00:00:00Z",
            "scopes":["https://www.googleapis.com/auth/gmail.send", "https://www.googleapis.com/auth/gmail.readonly"]
        })).unwrap();
        assert!(validate_token(&token, "sender@example.com", 1791786972).is_ok());
        assert!(validate_token(&token, "other@example.com", 1791786972).is_err());
        assert!(validate_token(&token, "sender@example.com", 2000000000).is_err());
        token.scopes.pop();
        assert!(validate_token(&token, "sender@example.com", 1791786972).is_err());
        assert_ne!(
            mailbox_id("owner-a", "conn_x"),
            mailbox_id("owner-b", "conn_x")
        );
    }
}
