use onecrew_local_runtime::ToolkitId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::filters::deserialize_unique_filter;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OperatorToolkitId {
    X,
    Instagram,
    Telegram,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ServiceToolkitId {
    Youtube,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MailboxToolkitId {
    Email,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ConnectionRequirement {
    None,
    OperatorAccount { toolkit: OperatorToolkitId },
    ServiceCredential { toolkit: ServiceToolkitId },
    Mailbox { toolkit: MailboxToolkitId },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ConnectionRef {
    OperatorAccount {
        toolkit: OperatorToolkitId,
        operator_account_id: String,
    },
    ServiceCredential {
        toolkit: ServiceToolkitId,
        credential_id: String,
    },
    Mailbox {
        toolkit: MailboxToolkitId,
        mailbox_ref_id: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConnectionListInput {
    #[serde(default, deserialize_with = "deserialize_toolkit_filters")]
    #[schemars(length(max = 8))]
    pub(crate) toolkits: Vec<ToolkitId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConnectionListResult {
    pub(crate) x: Vec<OperatorConnectionListItem>,
    pub(crate) instagram: Vec<OperatorConnectionListItem>,
    pub(crate) telegram: Vec<OperatorConnectionListItem>,
    pub(crate) youtube: Vec<ServiceCredentialConnectionListItem>,
    pub(crate) email: Vec<MailboxConnectionListItem>,
    pub(crate) truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OperatorConnectionListItem {
    pub(crate) connection: ConnectionRef,
    pub(crate) username: String,
    pub(crate) label: String,
    pub(crate) availability: onecrew_local_runtime::LocalConnectionAvailability,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServiceCredentialConnectionListItem {
    pub(crate) connection: ConnectionRef,
    pub(crate) label: String,
    pub(crate) availability: onecrew_local_runtime::LocalConnectionAvailability,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MailboxConnectionListItem {
    pub(crate) connection: ConnectionRef,
    pub(crate) email_address: String,
    pub(crate) display_label: String,
    pub(crate) availability: onecrew_local_runtime::LocalConnectionAvailability,
}

pub(crate) const MAX_CONNECTIONS_PER_TOOLKIT: usize = 20;

impl ConnectionListResult {
    pub(crate) fn empty() -> Self {
        Self {
            x: Vec::new(),
            instagram: Vec::new(),
            telegram: Vec::new(),
            youtube: Vec::new(),
            email: Vec::new(),
            truncated: false,
        }
    }

    pub(crate) fn project(
        inventory: onecrew_local_runtime::PlatformConnectionInventory,
        input: &ConnectionListInput,
    ) -> Self {
        let includes = |toolkit| input.toolkits.is_empty() || input.toolkits.contains(&toolkit);
        let mut truncated = false;
        let x = project_operator_connections(
            inventory.x,
            OperatorToolkitId::X,
            includes(ToolkitId::X),
            &mut truncated,
        );
        let instagram = project_operator_connections(
            inventory.instagram,
            OperatorToolkitId::Instagram,
            includes(ToolkitId::Instagram),
            &mut truncated,
        );
        let telegram = project_operator_connections(
            inventory.telegram,
            OperatorToolkitId::Telegram,
            includes(ToolkitId::Telegram),
            &mut truncated,
        );
        let youtube = if includes(ToolkitId::Youtube) {
            truncated |= inventory.youtube.len() > MAX_CONNECTIONS_PER_TOOLKIT;
            inventory
                .youtube
                .into_iter()
                .take(MAX_CONNECTIONS_PER_TOOLKIT)
                .map(|item| ServiceCredentialConnectionListItem {
                    connection: ConnectionRef::ServiceCredential {
                        toolkit: ServiceToolkitId::Youtube,
                        credential_id: item.credential_id,
                    },
                    label: item.label,
                    availability: item.availability,
                })
                .collect()
        } else {
            Vec::new()
        };
        let email = if includes(ToolkitId::Email) {
            truncated |= inventory.mailboxes.len() > MAX_CONNECTIONS_PER_TOOLKIT;
            inventory
                .mailboxes
                .into_iter()
                .take(MAX_CONNECTIONS_PER_TOOLKIT)
                .map(|item| MailboxConnectionListItem {
                    connection: ConnectionRef::Mailbox {
                        toolkit: MailboxToolkitId::Email,
                        mailbox_ref_id: item.mailbox_ref_id,
                    },
                    email_address: item.email_address,
                    display_label: item.display_label,
                    availability: item.availability,
                })
                .collect()
        } else {
            Vec::new()
        };
        Self {
            x,
            instagram,
            telegram,
            youtube,
            email,
            truncated,
        }
    }
}

fn project_operator_connections<T>(
    items: Vec<T>,
    toolkit: OperatorToolkitId,
    included: bool,
    truncated: &mut bool,
) -> Vec<OperatorConnectionListItem>
where
    T: IntoOperatorConnectionListItem,
{
    if !included {
        return Vec::new();
    }
    *truncated |= items.len() > MAX_CONNECTIONS_PER_TOOLKIT;
    items
        .into_iter()
        .take(MAX_CONNECTIONS_PER_TOOLKIT)
        .map(|item| item.into_item(toolkit))
        .collect()
}

trait IntoOperatorConnectionListItem {
    fn into_item(self, toolkit: OperatorToolkitId) -> OperatorConnectionListItem;
}

macro_rules! impl_operator_connection_item {
    ($type:ty) => {
        impl IntoOperatorConnectionListItem for $type {
            fn into_item(self, toolkit: OperatorToolkitId) -> OperatorConnectionListItem {
                OperatorConnectionListItem {
                    connection: ConnectionRef::OperatorAccount {
                        toolkit,
                        operator_account_id: self.operator_account_id,
                    },
                    username: self.username,
                    label: self.label,
                    availability: self.availability,
                }
            }
        }
    };
}

impl_operator_connection_item!(onecrew_local_runtime::XConnectionSummary);
impl_operator_connection_item!(onecrew_local_runtime::InstagramConnectionSummary);
impl_operator_connection_item!(onecrew_local_runtime::TelegramConnectionSummary);

impl ConnectionRequirement {
    pub(crate) fn accepts(&self, connection: Option<&ConnectionRef>) -> bool {
        match (self, connection) {
            (Self::None, None) => true,
            (
                Self::OperatorAccount { toolkit },
                Some(ConnectionRef::OperatorAccount {
                    toolkit: actual,
                    operator_account_id,
                }),
            ) => toolkit == actual && valid_reference_id(operator_account_id),
            (
                Self::ServiceCredential { toolkit },
                Some(ConnectionRef::ServiceCredential {
                    toolkit: actual,
                    credential_id,
                }),
            ) => toolkit == actual && valid_reference_id(credential_id),
            (
                Self::Mailbox { toolkit },
                Some(ConnectionRef::Mailbox {
                    toolkit: actual,
                    mailbox_ref_id,
                }),
            ) => toolkit == actual && valid_reference_id(mailbox_ref_id),
            _ => false,
        }
    }
}

fn valid_reference_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256
}

pub(in crate::operation_registry) fn deserialize_toolkit_filters<'de, D>(
    deserializer: D,
) -> Result<Vec<ToolkitId>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_unique_filter(deserializer, 8, "toolkit")
}
