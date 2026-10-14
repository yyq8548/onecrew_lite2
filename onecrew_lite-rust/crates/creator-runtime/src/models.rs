use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }
    };
}

id_type!(CreatorId);
id_type!(CreatorGroupId);
id_type!(CreatorAccountLinkId);
id_type!(PlatformAccountId);
id_type!(PlatformAccountObservationId);
id_type!(ObservationSourceId);
id_type!(ContactLocatorId);
id_type!(CreatorContactRouteId);
id_type!(CreatorContactRouteDeleteOperationId);
id_type!(LocatorObservationId);
id_type!(AccountRefId);
id_type!(ResolvedTargetId);
id_type!(LocatorResolutionId);
id_type!(IdentityConflictId);
id_type!(AccountRelationshipId);
id_type!(ConversationId);
id_type!(MessageId);
id_type!(EmailId);
id_type!(EmailThreadId);
id_type!(EmailTemplateId);
id_type!(EmailDraftId);
id_type!(MailboxRefId);
id_type!(TouchpointId);
id_type!(AuditEventId);
id_type!(CreatorOriginId);
id_type!(DiscoveryId);
id_type!(CreatorImportOperationId);
id_type!(ReplyTriageId);

fn default_conversation_limit() -> u16 {
    100
}

mod runtime_changes;
pub use runtime_changes::*;

mod data_upgrade;
pub use data_upgrade::*;

mod core;
pub use core::*;

mod creator_identity;
pub use creator_identity::*;

mod creator_transfer;
pub use creator_transfer::*;

mod accounts_and_targets;
pub use accounts_and_targets::*;

mod engagement_records;
pub use engagement_records::*;

mod email;
pub use email::*;

mod outreach;
pub use outreach::*;

mod creator;
pub use creator::*;

mod paging;
pub use paging::*;

mod reply_triage;
pub use reply_triage::*;
