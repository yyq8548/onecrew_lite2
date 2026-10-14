//! Process-local Creator invalidation shared by runtime clones.
use tokio::sync::broadcast;

use crate::{CreatorChange, CreatorChangeScope, CreatorRuntime};

#[derive(Clone)]
pub(super) struct CreatorChangeBus {
    sender: broadcast::Sender<CreatorChange>,
}

impl CreatorChangeBus {
    pub(super) fn new() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self { sender }
    }

    pub(super) fn subscribe(&self) -> broadcast::Receiver<CreatorChange> {
        self.sender.subscribe()
    }

    pub(super) fn publish<I>(&self, scopes: I)
    where
        I: IntoIterator<Item = CreatorChangeScope>,
    {
        let scopes = scopes.into_iter().collect::<Vec<_>>();
        if scopes.is_empty() {
            return;
        }
        if self.sender.send(CreatorChange { scopes }).is_err() {
            log::trace!("Creator invalidation had no subscriber");
        }
    }
}

impl CreatorRuntime {
    /// Subscribe to best-effort process-local Creator invalidations.
    /// Consumers handle lag by refreshing current owner state.
    pub fn subscribe_changes(&self) -> broadcast::Receiver<CreatorChange> {
        self.changes.subscribe()
    }

    /// Only Creator use cases publish their own committed or process-local
    /// state. Receiver loss never changes the result of the triggering work.
    pub(crate) fn publish_changes<I>(&self, scopes: I)
    where
        I: IntoIterator<Item = CreatorChangeScope>,
    {
        self.changes.publish(scopes);
    }
}

#[cfg(test)]
mod change_bus_tests {
    use super::{CreatorChangeBus, CreatorChangeScope};

    #[test]
    fn clones_share_delivery_and_ignore_empty_or_unobserved_changes() {
        let changes = CreatorChangeBus::new();
        changes.publish([CreatorChangeScope::Creators]);
        let cloned = changes.clone();
        let mut receiver = changes.subscribe();
        cloned.publish([]);
        assert!(matches!(
            receiver.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
        cloned.publish([CreatorChangeScope::Creators]);
        changes.publish([CreatorChangeScope::CreatorGroups]);
        assert_eq!(
            receiver.try_recv().unwrap().scopes,
            vec![CreatorChangeScope::Creators]
        );
        assert_eq!(
            receiver.try_recv().unwrap().scopes,
            vec![CreatorChangeScope::CreatorGroups]
        );
    }
}
