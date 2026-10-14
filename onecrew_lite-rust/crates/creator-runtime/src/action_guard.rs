//! Process-local same-key ownership shared by Email batches and typed outreach.
//! The durable ledger remains the authority for remote effects and recovery.

use crate::CreatorRuntime;

/// Holds one idempotency key as in-flight for this process; dropping the
/// guard releases it. A second same-key call while the guard is held would
/// otherwise repeat a provider write whose first attempt is still pending.
pub(crate) struct InFlightActionGuard {
    key: String,
    actions: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
}

impl InFlightActionGuard {
    pub(crate) fn acquire(runtime: &CreatorRuntime, key: &str) -> Result<Self, String> {
        let actions = runtime.in_flight_actions();
        {
            let mut held = actions
                .lock()
                .map_err(|_| "in-flight action registry poisoned")?;
            if !held.insert(key.to_owned()) {
                return Err(
                    "an action with this idempotency key is still in progress in this process; wait for it to finish before retrying"
                        .into(),
                );
            }
        }
        Ok(Self {
            key: key.to_owned(),
            actions,
        })
    }
}

impl Drop for InFlightActionGuard {
    fn drop(&mut self) {
        if let Ok(mut held) = self.actions.lock() {
            held.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn in_flight_guard_blocks_a_concurrent_same_key_action_and_releases_on_drop() {
        let directory = tempfile::tempdir().unwrap();
        let runtime =
            crate::CreatorRuntime::open(directory.path(), crate::CreatorGateways::default())
                .unwrap();
        let guard = crate::action_guard::InFlightActionGuard::acquire(&runtime, "shared-key")
            .expect("first acquire");
        let error = crate::action_guard::InFlightActionGuard::acquire(&runtime, "shared-key")
            .map(|_| ())
            .unwrap_err();
        assert!(
            error.contains("still in progress in this process"),
            "{error}"
        );
        drop(guard);
        crate::action_guard::InFlightActionGuard::acquire(&runtime, "shared-key")
            .expect("reacquire after release");
    }
}
