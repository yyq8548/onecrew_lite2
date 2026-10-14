//! Construct one shared application graph under the data-directory lifecycle lock.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::credentials::{EmailMailboxAuthResolver, email_gateway::RuntimeEmailGateway};
use onecrew_creator_runtime::{CreatorGateways, CreatorRuntime, MailboxAuthBinding, MailboxRefId};
use onecrew_creator_sync::CreatorSyncRuntime;
use onecrew_local_vault::LocalVault;
use onecrew_sessions::PlatformSessions;

use crate::{
    LocalDataPreflight, LocalDataState, LocalDataUpgradeError, LocalDataUpgradeResult,
    LocalPlatform, browser, credentials, data_upgrade, interaction,
};

/// The late-bound Creator mailbox callback obtains an owned binding through
/// Creator read admission before vault/cache work or a provider future starts.
/// Weak ownership avoids a services -> Creator -> gateway -> services Arc cycle.
struct CreatorEmailMailboxResolver(std::sync::Weak<CreatorRuntime>);

#[async_trait::async_trait]
impl EmailMailboxAuthResolver for CreatorEmailMailboxResolver {
    async fn resolve_mailbox_auth_connection(
        &self,
        mailbox_ref_id: &MailboxRefId,
    ) -> Result<MailboxAuthBinding, String> {
        self.0
            .upgrade()
            .ok_or_else(|| "Creator mailbox owner is unavailable".to_string())?
            .resolve_mailbox_auth_binding(mailbox_ref_id)
            .await
    }
}

#[derive(Debug, Clone)]
pub struct LocalPlatformHostPaths {
    system_desktop_dir: Option<PathBuf>,
}

impl LocalPlatformHostPaths {
    pub fn with_system_desktop(system_desktop_dir: PathBuf) -> Self {
        Self {
            system_desktop_dir: Some(system_desktop_dir),
        }
    }

    pub fn without_system_desktop() -> Self {
        Self {
            system_desktop_dir: None,
        }
    }
}

impl LocalPlatform {
    pub fn preflight_data_dir(data_dir: impl AsRef<Path>) -> LocalDataPreflight {
        data_upgrade::inspect_data_dir(data_dir.as_ref())
    }

    pub fn upgrade_data_dir(
        data_dir: impl AsRef<Path>,
    ) -> Result<LocalDataUpgradeResult, LocalDataUpgradeError> {
        data_upgrade::upgrade_data_dir(data_dir.as_ref())
    }

    /// Opens a headless runtime without native Desktop-path capabilities.
    ///
    /// # Errors
    ///
    /// Returns an error when the data-directory lock cannot be acquired, local
    /// data fails preflight validation, or the vault or Creator application graph
    /// cannot be constructed.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, String> {
        Self::open_with_host_paths(data_dir, LocalPlatformHostPaths::without_system_desktop())
    }

    /// Open the runtime with native paths resolved and injected by the
    /// Desktop shell. Path unavailability affects only the corresponding
    /// capability, never construction of the shared application graph.
    ///
    /// # Errors
    ///
    /// Returns an error when the data-directory lock cannot be acquired, local
    /// data fails preflight validation, or the vault or Creator application graph
    /// cannot be constructed. Missing optional host paths are represented as
    /// unavailable capabilities rather than an initialization error.
    pub fn open_with_host_paths(
        data_dir: impl AsRef<Path>,
        host_paths: LocalPlatformHostPaths,
    ) -> Result<Self, String> {
        Self::open_with_host_paths_and_browser_host(
            data_dir,
            host_paths,
            Arc::new(browser::RealBrowserHost),
            None,
        )
    }

    pub fn open_with_host_paths_and_creator_sync(
        data_dir: impl AsRef<Path>,
        host_paths: LocalPlatformHostPaths,
        creator_sync: Arc<CreatorSyncRuntime>,
    ) -> Result<Self, String> {
        Self::open_with_host_paths_and_browser_host(
            data_dir,
            host_paths,
            Arc::new(browser::RealBrowserHost),
            Some(creator_sync),
        )
    }

    /// `open` with the host facts supplied, so a test decides what this machine
    /// "is" instead of inheriting it from the machine running the test.
    #[cfg(test)]
    pub(crate) fn open_with_browser_host(
        data_dir: impl AsRef<Path>,
        browser_host: Arc<dyn browser::BrowserHostProbe>,
    ) -> Result<Self, String> {
        Self::open_with_host_paths_and_browser_host(
            data_dir,
            LocalPlatformHostPaths::without_system_desktop(),
            browser_host,
            None,
        )
    }

    fn open_with_host_paths_and_browser_host(
        data_dir: impl AsRef<Path>,
        host_paths: LocalPlatformHostPaths,
        browser_host: Arc<dyn browser::BrowserHostProbe>,
        creator_sync: Option<Arc<CreatorSyncRuntime>>,
    ) -> Result<Self, String> {
        let data_dir = data_dir.as_ref().to_path_buf();
        data_upgrade::with_data_dir_lock(&data_dir, || {
            Self::open_unlocked(
                &data_dir,
                host_paths.clone(),
                browser_host.clone(),
                creator_sync.clone(),
            )
        })
    }

    fn open_unlocked(
        data_dir: &Path,
        host_paths: LocalPlatformHostPaths,
        browser_host: Arc<dyn browser::BrowserHostProbe>,
        creator_sync: Option<Arc<CreatorSyncRuntime>>,
    ) -> Result<Self, String> {
        let data_dir = data_dir.to_path_buf();
        let preflight = Self::preflight_data_dir(&data_dir);
        if !matches!(
            preflight.state,
            LocalDataState::Fresh | LocalDataState::Ready
        ) {
            return Err(preflight
                .message
                .unwrap_or_else(|| "local data cannot be safely opened".into()));
        }
        let vault = LocalVault::open(data_dir.join("agent-platform.db"))
            .map_err(|error| error.to_string())?;
        let creator_sync = creator_sync.map_or_else(
            || {
                CreatorSyncRuntime::with_vault(vault.clone())
                    .map(Arc::new)
                    .map_err(|_| "Creator Sync runtime is unavailable".to_string())
            },
            Ok,
        )?;
        let result_store = interaction::ResultMemoryStore::new();
        let email_transport = Arc::new(
            onecrew_email::ResendAdmission::new()
                .expect("static Resend transport configuration is always constructible"),
        );
        let credential_services = Arc::new(credentials::services::CredentialServices::new(
            vault.clone(),
            email_transport,
        ));
        let sessions = Arc::new(PlatformSessions::new(vault.clone()));
        let direct_messages = Arc::new(
            crate::direct_message_access::SharedDirectMessageAccess::new(Arc::clone(&sessions)),
        );
        let email_gateway = Arc::new(RuntimeEmailGateway::new(credential_services.clone()));
        let creators = Arc::new(CreatorRuntime::open(
            &data_dir,
            CreatorGateways::default()
                .with_email(email_gateway)
                .with_direct_messages(direct_messages.clone()),
        )?);
        credential_services.bind_email_mailbox_resolver(Arc::new(CreatorEmailMailboxResolver(
            Arc::downgrade(&creators),
        )));
        let platform = Self {
            vault,
            sessions,
            direct_messages,
            creator_sync,
            data_dir,
            result_store,
            search_authority: crate::search_plan::SearchAuthority::default(),
            discovery_draft_gate: tokio::sync::Semaphore::new(1),
            discovery_score_gate: crate::discovery_match::ScoreGate::default(),
            discovery_draft_progress: std::sync::Mutex::new(None),
            opencli_auth_observations: std::sync::Mutex::new(Vec::new()),
            presentation_preferences: Arc::new(crate::PresentationPreferences::default()),
            system_desktop_dir: host_paths.system_desktop_dir,
            pending_tg: Mutex::new(HashMap::new()),
            pending_tg_qr: Mutex::new(HashMap::new()),
            pending_browser: Mutex::new(HashMap::new()),
            pending_account_connections: Mutex::new(HashMap::new()),
            tg_creds: Mutex::new(None),
            creators,
            changes: crate::changes::PlatformChangeBus::new(),
            credential_services,
            browser_host,
        };
        platform.reconcile_vault_projections()?;
        platform.recover_pending_operator_account_disconnects()?;
        platform.recover_pending_service_credential_deletes()?;
        Ok(platform)
    }

    /// Reconciles the complete Vault identity snapshot into Creator projections.
    ///
    /// This is a lifecycle operation, not an accessor concern. Startup uses it
    /// before recovery, while profile and credential mutation paths use their
    /// narrower explicit reconcilers so reads never trigger a write-back.
    pub(super) fn reconcile_vault_projections(&self) -> Result<(), String> {
        self.reconcile_vault_profile_refs()?;
        self.reconcile_vault_youtube_refs()?;
        self.reconcile_vault_mailbox_refs()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
