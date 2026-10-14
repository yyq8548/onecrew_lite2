use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::models::{
    EmailFolder, EmailId, EmailThreadDispositionAction, EmailThreadId, MailboxRefId,
};

pub const MAIL_PAGE_LIMITS: crate::PageLimits = crate::PageLimits::new(25, 100);
pub const EMAIL_THREAD_MESSAGE_PAGE_LIMITS: crate::PageLimits = crate::PageLimits::new(50, 200);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EmailThreadSort {
    Newest,
    Oldest,
}

impl EmailThreadSort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Newest => "newest",
            Self::Oldest => "oldest",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListEmailThreadsInput {
    pub mailbox_ref_id: Option<MailboxRefId>,
    #[serde(default)]
    pub folder: EmailFolder,
    pub sort: EmailThreadSort,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListEmailFoldersInput {
    pub mailbox_ref_id: Option<MailboxRefId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmailFolderSummary {
    pub folder: EmailFolder,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateEmailThreadDispositionInput {
    pub thread_id: EmailThreadId,
    pub action: EmailThreadDispositionAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmailThreadMutationInput {
    pub thread_id: EmailThreadId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmailThreadGetInput {
    pub thread_id: EmailThreadId,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmailMessageGetInput {
    pub email_id: EmailId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MailboxGetInput {
    pub mailbox_ref_id: MailboxRefId,
}
