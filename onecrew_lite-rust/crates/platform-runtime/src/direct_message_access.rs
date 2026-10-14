//! One session-backed provider selector shared by toolkit writes and Creator sync.
use std::sync::Arc;

use onecrew_creator_runtime::{AccountRef, DirectMessageAccess, Platform};
use onecrew_dm_contracts::{DirectMessageProvider, DmReadError, DmTarget, DmThread};
use onecrew_sessions::PlatformSessions;

use crate::toolkits::DmToolkitError;
use crate::{LocalPlatform, OperatorAccountPlatform, PlatformConnectionSelector};

pub(crate) struct SharedDirectMessageAccess {
    sessions: Arc<PlatformSessions>,
}

impl SharedDirectMessageAccess {
    pub(crate) fn new(sessions: Arc<PlatformSessions>) -> Self {
        Self { sessions }
    }

    fn for_operator(
        &self,
        platform: OperatorAccountPlatform,
        operator_id: &str,
    ) -> Arc<dyn DirectMessageProvider> {
        match platform {
            OperatorAccountPlatform::X => self.sessions.x_direct_messages(operator_id),
            OperatorAccountPlatform::Instagram => {
                self.sessions.instagram_direct_messages(operator_id)
            }
            OperatorAccountPlatform::Telegram => {
                self.sessions.telegram_direct_messages(operator_id)
            }
        }
    }
}

#[async_trait::async_trait]
impl DirectMessageAccess for SharedDirectMessageAccess {
    async fn thread(
        &self,
        account: &AccountRef,
        target: &DmTarget,
        external_thread_id: Option<&str>,
        limit: u16,
    ) -> Result<Option<DmThread>, DmReadError> {
        let platform = match account.platform {
            Platform::X => OperatorAccountPlatform::X,
            Platform::Instagram => OperatorAccountPlatform::Instagram,
            Platform::Telegram => OperatorAccountPlatform::Telegram,
            Platform::YouTube => {
                return Err(DmReadError {
                    diagnostic: onecrew_dm_contracts::DmDiagnostic::local(
                        "youtube",
                        "onecrew_dm_dispatch",
                        "unsupported_platform",
                        "YouTube does not support direct messages",
                    ),
                });
            }
            Platform::TikTok => {
                return Err(DmReadError {
                    diagnostic: onecrew_dm_contracts::DmDiagnostic::local(
                        "tiktok",
                        "onecrew_dm_dispatch",
                        "unsupported_platform",
                        "TikTok does not support direct messages",
                    ),
                });
            }
        };
        // Each existing session factory verifies the stored profile against
        // its platform before using credentials; this bridge adds no cache.
        self.for_operator(platform, account.id.as_str())
            .thread(target, external_thread_id, limit)
            .await
    }
}

impl LocalPlatform {
    pub(crate) async fn admitted_direct_messages(
        &self,
        operator_id: &str,
        platform: OperatorAccountPlatform,
    ) -> Result<Arc<dyn DirectMessageProvider>, DmToolkitError> {
        let account = self
            .resolve_platform_connection(PlatformConnectionSelector::OperatorAccount {
                platform,
                operator_account_id: operator_id.to_owned(),
            })
            .await
            .map_err(DmToolkitError::Admission)?;
        Ok(self
            .direct_messages
            .for_operator(platform, account.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_sync_share_the_runtime_session_owner() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        assert!(Arc::ptr_eq(
            &platform.sessions,
            &platform.direct_messages.sessions
        ));
    }
}
