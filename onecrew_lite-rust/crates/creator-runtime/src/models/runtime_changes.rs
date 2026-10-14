use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Creator-owned collections whose projections may need refreshing.
/// These hints contain no entity data, secrets, or commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorChangeScope {
    EmailFolders,
    EmailThreads,
    EmailDrafts,
    EmailTemplates,
    /// Process-local mailbox sync started or finished; page commits use
    /// EmailFolders/EmailThreads independently.
    EmailSync,
    Creators,
    CreatorGroups,
    /// Locally persisted DM threads. Refreshing this scope must remain a local
    /// read rather than starting another provider sync.
    Conversations,
    Outreach,
    ReplyTriage,
}

impl CreatorChangeScope {
    /// The complete Creator vocabulary. Service combines this with platform
    /// scopes and its own All sentinel when projecting the existing wire.
    pub const ALL: [Self; 10] = [
        Self::EmailFolders,
        Self::EmailThreads,
        Self::EmailDrafts,
        Self::EmailTemplates,
        Self::EmailSync,
        Self::Creators,
        Self::CreatorGroups,
        Self::Conversations,
        Self::Outreach,
        Self::ReplyTriage,
    ];
}

/// An owner-local invalidation. Service owns outbound sequence assignment;
/// this message is neither a durable event nor an operation success receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatorChange {
    pub scopes: Vec<CreatorChangeScope>,
}

#[cfg(test)]
mod creator_change_tests {
    use super::CreatorChangeScope;

    #[test]
    fn creator_vocabulary_is_exhaustive_and_has_no_platform_or_bridge_scope() {
        fn position(scope: CreatorChangeScope) -> usize {
            match scope {
                CreatorChangeScope::EmailFolders => 0,
                CreatorChangeScope::EmailThreads => 1,
                CreatorChangeScope::EmailDrafts => 2,
                CreatorChangeScope::EmailTemplates => 3,
                CreatorChangeScope::EmailSync => 4,
                CreatorChangeScope::Creators => 5,
                CreatorChangeScope::CreatorGroups => 6,
                CreatorChangeScope::Conversations => 7,
                CreatorChangeScope::Outreach => 8,
                CreatorChangeScope::ReplyTriage => 9,
            }
        }
        for (index, scope) in CreatorChangeScope::ALL.into_iter().enumerate() {
            assert_eq!(position(scope), index);
        }
        assert_eq!(
            serde_json::to_value(CreatorChangeScope::ALL).unwrap(),
            serde_json::json!([
                "email_folders",
                "email_threads",
                "email_drafts",
                "email_templates",
                "email_sync",
                "creators",
                "creator_groups",
                "conversations",
                "outreach",
                "reply_triage"
            ])
        );
    }
}
