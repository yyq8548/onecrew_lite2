//! Minimal, secret-free navigation index for platform connections.
//!
//! The aggregate only selects the next typed detail route. Each branch retains
//! its real public identifier and owns its mapping invariants; detail and
//! actions remain in their dedicated runtime operations.

mod instagram;
mod mailboxes;
mod resolution;
mod shared;
mod telegram;
mod x;
mod youtube;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::LocalPlatform;

pub use instagram::InstagramConnectionSummary;
pub use mailboxes::{MailboxAccountSummary, MailboxProviderKind};
pub use resolution::{PlatformConnectionResolutionError, PlatformConnectionSelector};
pub use telegram::TelegramConnectionSummary;
pub use x::XConnectionSummary;
pub use youtube::YoutubeCredentialSummary;

/// Local readiness for selecting an entry from the navigation index.
///
/// `available` describes locally recorded/readable state only. It does not
/// claim that a remote provider check ran during this list operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocalConnectionAvailability {
    Available,
    Unavailable,
}

/// Bounded navigation index whose groups keep independent platform DTOs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlatformConnectionInventory {
    pub x: Vec<XConnectionSummary>,
    pub instagram: Vec<InstagramConnectionSummary>,
    pub telegram: Vec<TelegramConnectionSummary>,
    pub youtube: Vec<YoutubeCredentialSummary>,
    pub mailboxes: Vec<MailboxAccountSummary>,
}

impl LocalPlatform {
    /// Build the local navigation index without provider I/O.
    pub fn platform_connection_list(&self) -> Result<PlatformConnectionInventory, String> {
        let operator_accounts = self.operator_account_list()?;
        let youtube_credentials = self.list_youtube_api_keys()?;
        let mailbox_accounts = self.creators.list_mailboxes()?;

        Ok(PlatformConnectionInventory {
            x: x::summaries(&operator_accounts),
            instagram: instagram::summaries(&operator_accounts),
            telegram: telegram::summaries(&operator_accounts),
            youtube: youtube::summaries(&self.vault, youtube_credentials),
            mailboxes: mailboxes::summaries(mailbox_accounts),
        })
    }

    /// Admit one platform connection for a provider read: the vault
    /// projection runs on the blocking pool, then the Creator `account_refs`
    /// gate is read under bounded Creator admission. Fails closed: a gate
    /// that cannot be read admits nothing.
    pub async fn resolve_platform_connection(
        &self,
        selector: PlatformConnectionSelector,
    ) -> Result<onecrew_creator_runtime::AccountRefId, PlatformConnectionResolutionError> {
        resolution::resolve(self, selector).await
    }
}

#[cfg(test)]
mod tests;
