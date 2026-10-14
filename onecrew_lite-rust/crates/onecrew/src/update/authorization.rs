//! Existing installation credential reads only; never creates or upgrades a vault.
use onecrew_local_vault::{LocalVault, VaultCreatorSyncCredentialState};
use std::path::{Path, PathBuf};

pub(super) struct Authorization(VaultCreatorSyncCredentialState);

impl Authorization {
    pub(super) fn bearer(&self) -> &str {
        match &self.0 {
            VaultCreatorSyncCredentialState::Active { bearer, .. } => bearer.expose_secret(),
            _ => unreachable!("validated active credential"),
        }
    }

    pub(super) async fn verify_current(&self, data_dir: PathBuf) -> Result<(), String> {
        let current = load(data_dir).await?;
        if self.0.credential() != current.0.credential() || self.bearer() != current.bearer() {
            return Err("update_owner_changed".into());
        }
        Ok(())
    }
}

pub(super) async fn load(data_dir: PathBuf) -> Result<Authorization, String> {
    tokio::task::spawn_blocking(move || load_existing(&data_dir))
        .await
        .map_err(|_| "update_credential_worker_failed")?
}

fn load_existing(data_dir: &Path) -> Result<Authorization, String> {
    let vault = LocalVault::open_existing_read_only(data_dir.join("agent-platform.db"))
        .map_err(|_| "update_login_unavailable")?;
    match vault
        .get_creator_sync_credential()
        .map_err(|_| "update_login_unavailable")?
    {
        Some(state @ VaultCreatorSyncCredentialState::Active { .. }) => Ok(Authorization(state)),
        _ => Err("update_login_required".into()),
    }
}
