//! Process-local invalidation facts owned by the account and credential runtime.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Coarse platform collections to refresh; never carries credentials or commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlatformChangeScope {
    OperatorAccounts,
    ServiceCredentials,
}

impl PlatformChangeScope {
    pub const ALL: [Self; 2] = [Self::OperatorAccounts, Self::ServiceCredentials];
}

/// A best-effort hint to read the current owning runtime projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlatformChange {
    pub scopes: Vec<PlatformChangeScope>,
}

#[derive(Clone)]
pub(crate) struct PlatformChangeBus {
    sender: broadcast::Sender<PlatformChange>,
}

impl PlatformChangeBus {
    pub(crate) fn new() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self { sender }
    }

    fn publish(&self, scopes: impl IntoIterator<Item = PlatformChangeScope>) {
        let scopes = scopes.into_iter().collect::<Vec<_>>();
        // No subscriber must never fail an already committed operation.
        if !scopes.is_empty() && self.sender.send(PlatformChange { scopes }).is_err() {
            log::trace!("platform invalidation had no subscriber");
        }
    }
}

impl crate::LocalPlatform {
    /// Subscribe to process-local platform invalidations. A lagged receiver must
    /// refresh all projections; notifications are not a durable event feed.
    pub fn subscribe_changes(&self) -> broadcast::Receiver<PlatformChange> {
        self.changes.sender.subscribe()
    }

    pub(crate) fn publish_changes(&self, scopes: impl IntoIterator<Item = PlatformChangeScope>) {
        self.changes.publish(scopes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_change_bus_is_shared_empty_and_unobserved_changes_are_harmless() {
        let bus = PlatformChangeBus::new();
        bus.publish([PlatformChangeScope::OperatorAccounts]);
        let cloned = bus.clone();
        let mut receiver = bus.sender.subscribe();
        cloned.publish([]);
        assert!(matches!(
            receiver.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        cloned.publish(PlatformChangeScope::ALL);
        let event = receiver.try_recv().unwrap();
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            serde_json::json!({
                "scopes": ["operator_accounts", "service_credentials"]
            })
        );
    }
}
