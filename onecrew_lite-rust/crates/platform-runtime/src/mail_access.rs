//! Remote mail authorization uses the existing paired installation credential.
use crate::LocalPlatform;
use onecrew_access_client::mail::{MailClient, MailConnection};
use onecrew_local_vault::VaultCreatorSyncCredentialState;

pub(crate) struct Identity(VaultCreatorSyncCredentialState);
impl Identity {
    pub(crate) fn user(&self) -> &str {
        &self.0.credential().metadata.access_user_id
    }
    pub(crate) fn bearer(&self) -> &str {
        match &self.0 {
            VaultCreatorSyncCredentialState::Active { bearer, .. } => bearer.expose_secret(),
            _ => unreachable!("identity admits active credentials only"),
        }
    }
}
pub(crate) async fn identity(vault: onecrew_local_vault::LocalVault) -> Result<Identity, String> {
    tokio::task::spawn_blocking(move || {
        match vault
            .get_creator_sync_credential()
            .map_err(|_| "mail_identity_unavailable")?
        {
            Some(state @ VaultCreatorSyncCredentialState::Active { .. }) => Ok(Identity(state)),
            _ => Err("mail_access_login_required".to_owned()),
        }
    })
    .await
    .map_err(|_| "mail_identity_unavailable".to_owned())?
}
pub(crate) async fn check_identity(
    vault: onecrew_local_vault::LocalVault,
    before: &Identity,
) -> Result<(), String> {
    let after = identity(vault).await?;
    if before.0.credential() != after.0.credential() || before.bearer() != after.bearer() {
        return Err("mail_access_account_changed".into());
    }
    Ok(())
}
pub(crate) fn mailbox_id(user: &str, connection: &str) -> String {
    use sha2::{Digest, Sha256};
    let bytes = Sha256::digest(format!("onecrew-gmail\0{user}\0{connection}"));
    uuid::Uuid::from_bytes(bytes[..16].try_into().expect("digest length")).to_string()
}
impl LocalPlatform {
    async fn mail_access_identity(&self) -> Result<Identity, String> {
        identity(self.vault.clone()).await
    }
    async fn check_mail_identity(&self, before: &Identity) -> Result<(), String> {
        check_identity(self.vault.clone(), before).await
    }
    /// Returns only the short-lived browser confirmation URL, never mailbox tokens.
    pub async fn gmail_connect_url(&self) -> Result<String, String> {
        let identity = self.mail_access_identity().await?;
        let client = MailClient::new(onecrew_creator_sync::production_access_origin())
            .map_err(|e| e.code().to_owned())?;
        let url = client
            .connect_google(identity.bearer())
            .await
            .map_err(|e| e.code().to_owned())?;
        self.check_mail_identity(&identity).await?;
        Ok(url)
    }
    /// Explicit reconciliation after OAuth. No email is sent or imported.
    pub async fn refresh_gmail_mailboxes(&self) -> Result<Vec<MailConnection>, String> {
        use onecrew_creator_runtime::*;
        let identity = identity(self.vault.clone()).await?;
        let client = MailClient::new(onecrew_creator_sync::production_access_origin())
            .map_err(|e| e.code().to_owned())?;
        let connections = client
            .connections(identity.bearer())
            .await
            .map_err(|e| e.code().to_owned())?;
        check_identity(self.vault.clone(), &identity).await?;
        let mut projections = Vec::new();
        for c in connections.iter().filter(|c| c.provider == "google") {
            if !c.id.starts_with("conn_") || uuid::Uuid::parse_str(&c.id[5..]).is_err() {
                return Err("mail_connection_invalid".into());
            }
            let id = MailboxRefId::new(mailbox_id(identity.user(), &c.id));
            let available = c.status == "connected"
                && c.capabilities.can_send
                && c.capabilities.can_read_thread
                && c.scopes
                    .iter()
                    .any(|s| s == "https://www.googleapis.com/auth/gmail.send")
                && c.scopes
                    .iter()
                    .any(|s| s == "https://www.googleapis.com/auth/gmail.readonly");
            projections.push((
                MailboxRefInput {
                    id: id.clone(),
                    provider: "gmail".into(),
                    label: c.email.clone(),
                    email_address: c.email.clone(),
                    display_name: None,
                    capabilities: vec![EmailCapability::Send, EmailCapability::SentReadback, EmailCapability::Inbox],
                    available,
                    last_error: (!available).then(|| "gmail_authorization_required".into()),
                },
                MailboxIdentityMappingInput {
                    mailbox_ref_id: id,
                    auth_connection_id: EmailAuthConnectionId::new(format!(
                        "gmail:{}:{}",
                        identity.user(),
                        c.id
                    )),
                    provider_kind: EmailProviderKind::Remote,
                    secret_record_id: EmailSecretRecordId::new(format!(
                        "access:{}:{}",
                        identity.user(),
                        c.id
                    )),
                    connection_label: c.email.clone(),
                    projection_kind: MailboxProjectionKind::Mailbox,
                    capability_projection_revision: 2,
                    checkpoint_namespace: format!("gmail-{}", c.id),
                },
            ));
        }
        self.creators()?
            .reconcile_gmail_mailboxes(projections)
            .await?;
        check_identity(self.vault.clone(), &identity).await?;
        Ok(connections)
    }
    /// Fresh remote status; does not import messages or enable sending implicitly.
    pub async fn mail_access_connections(&self) -> Result<Vec<MailConnection>, String> {
        let identity = self.mail_access_identity().await?;
        let client = MailClient::new(onecrew_creator_sync::production_access_origin())
            .map_err(|e| e.code().to_owned())?;
        let connections = client
            .connections(identity.bearer())
            .await
            .map_err(|e| e.code().to_owned())?;
        self.check_mail_identity(&identity).await?;
        Ok(connections)
    }
}
