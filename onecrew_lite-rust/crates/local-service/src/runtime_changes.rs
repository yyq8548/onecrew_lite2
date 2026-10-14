//! Merge owner invalidations into the existing webpage wire in one hosted task.
//! Positive sequences order this merge's emissions only, not durable commits.
//! Lifecycle/transport reconnect invalidations retain their zero sentinel.

use onecrew_creator_runtime::{CreatorChange, CreatorChangeScope};
use onecrew_local_runtime::{PlatformChange, PlatformChangeScope};
use serde::Serialize;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::state::ServiceEvent;

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum BridgeScope {
    All,
}

/// Owner enums retain the vocabulary; this adapter contributes only `all`.
#[derive(Serialize)]
#[serde(untagged)]
enum WireScope {
    Creator(CreatorChangeScope),
    Platform(PlatformChangeScope),
    Bridge(BridgeScope),
}

#[derive(Serialize)]
struct WireChange {
    sequence: u64,
    scopes: Vec<WireScope>,
}

fn receive_scopes<T>(
    received: Result<T, broadcast::error::RecvError>,
    open: &mut bool,
    project: impl FnOnce(T) -> Vec<WireScope>,
) -> Option<Vec<WireScope>> {
    match received {
        Ok(change) => Some(project(change)),
        Err(broadcast::error::RecvError::Lagged(_)) => {
            Some(vec![WireScope::Bridge(BridgeScope::All)])
        }
        Err(broadcast::error::RecvError::Closed) => {
            *open = false;
            None
        }
    }
}

/// The existing Application owns and joins this task. Receivers, not owner
/// handles, are retained here, so either owner can close independently.
pub(crate) async fn run_change_merge(
    mut creators: broadcast::Receiver<CreatorChange>,
    mut platform: broadcast::Receiver<PlatformChange>,
    events: broadcast::Sender<ServiceEvent>,
    cancel: CancellationToken,
) {
    let (mut creators_open, mut platform_open) = (true, true);
    let mut sequence = 0_u64;
    while creators_open || platform_open {
        let scopes = tokio::select! {
            // Cancellation wins even under continuous owner traffic. The
            // nested unbiased select still treats both owners fairly.
            biased;
            _ = cancel.cancelled() => break,
            scopes = async {
                tokio::select! {
                    received = creators.recv(), if creators_open => {
                        receive_scopes(received, &mut creators_open, |change| {
                            change.scopes.into_iter().map(WireScope::Creator).collect()
                        })
                    }
                    received = platform.recv(), if platform_open => {
                        receive_scopes(received, &mut platform_open, |change| {
                            change.scopes.into_iter().map(WireScope::Platform).collect()
                        })
                    }
                }
            } => scopes,
        };
        let Some(scopes) = scopes else { continue };
        let Some(next) = sequence.checked_add(1) else {
            log::error!("runtime change merge sequence exhausted");
            break;
        };
        sequence = next;
        match serde_json::to_value(WireChange { sequence, scopes }) {
            Ok(payload) => {
                if events
                    .send(ServiceEvent {
                        event: "runtime://changed",
                        payload,
                    })
                    .is_err()
                {
                    log::debug!("runtime change published without a webpage subscriber");
                }
            }
            Err(_) => log::error!("runtime change wire serialization failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::{collections::BTreeSet, time::Duration};
    use tokio::task::JoinHandle;

    struct Harness {
        creators: broadcast::Sender<CreatorChange>,
        platform: broadcast::Sender<PlatformChange>,
        events: broadcast::Receiver<ServiceEvent>,
        cancel: CancellationToken,
        task: JoinHandle<()>,
    }

    impl Harness {
        fn new(capacity: usize) -> Self {
            let (creators, creator_rx) = broadcast::channel(capacity);
            let (platform, platform_rx) = broadcast::channel(capacity);
            let (events, event_rx) = broadcast::channel(32);
            let cancel = CancellationToken::new();
            let task = tokio::spawn(run_change_merge(
                creator_rx,
                platform_rx,
                events,
                cancel.clone(),
            ));
            Self {
                creators,
                platform,
                events: event_rx,
                cancel,
                task,
            }
        }

        async fn next(&mut self) -> Value {
            let event = tokio::time::timeout(Duration::from_secs(2), self.events.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(event.event, "runtime://changed");
            event.payload
        }

        async fn stop(self) {
            self.cancel.cancel();
            joined(self.task).await;
        }
    }

    async fn joined(task: JoinHandle<()>) {
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
    }

    fn creator() -> CreatorChange {
        CreatorChange {
            scopes: vec![CreatorChangeScope::Creators],
        }
    }

    fn account() -> PlatformChange {
        PlatformChange {
            scopes: vec![PlatformChangeScope::OperatorAccounts],
        }
    }

    #[tokio::test]
    async fn mixed_owner_facts_share_one_sequence_and_the_existing_wire() {
        let mut h = Harness::new(4);
        h.creators.send(creator()).unwrap();
        assert_eq!(h.next().await, json!({"sequence":1,"scopes":["creators"]}));
        h.platform.send(account()).unwrap();
        assert_eq!(
            h.next().await,
            json!({"sequence":2,"scopes":["operator_accounts"]})
        );
        h.creators
            .send(CreatorChange {
                scopes: vec![CreatorChangeScope::EmailFolders],
            })
            .unwrap();
        assert_eq!(
            h.next().await,
            json!({"sequence":3,"scopes":["email_folders"]})
        );
        h.stop().await;
    }

    #[tokio::test]
    async fn either_owner_lag_invalidates_all_then_both_owners_continue() {
        for creator_lags in [true, false] {
            let mut h = Harness::new(1);
            // No await before filling: the single-thread test runtime has not
            // polled the merge yet, making overflow deterministic.
            for _ in 0..3 {
                if creator_lags {
                    h.creators.send(creator()).unwrap();
                } else {
                    h.platform.send(account()).unwrap();
                }
            }
            assert_eq!(h.next().await, json!({"sequence":1,"scopes":["all"]}));
            let scope = if creator_lags {
                "creators"
            } else {
                "operator_accounts"
            };
            assert_eq!(h.next().await, json!({"sequence":2,"scopes":[scope]}));
            if creator_lags {
                h.platform.send(account()).unwrap();
            } else {
                h.creators.send(creator()).unwrap();
            }
            let scope = if creator_lags {
                "operator_accounts"
            } else {
                "creators"
            };
            assert_eq!(h.next().await, json!({"sequence":3,"scopes":[scope]}));
            h.stop().await;
        }
    }

    #[tokio::test]
    async fn either_owner_can_close_without_closing_the_other_and_both_close_the_task() {
        for creator_closes in [true, false] {
            let Harness {
                creators,
                platform,
                mut events,
                task,
                ..
            } = Harness::new(4);
            if creator_closes {
                drop(creators);
                platform.send(account()).unwrap();
                let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    event.payload,
                    json!({"sequence":1,"scopes":["operator_accounts"]})
                );
                assert!(!task.is_finished());
                drop(platform);
            } else {
                drop(platform);
                creators.send(creator()).unwrap();
                let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(event.payload, json!({"sequence":1,"scopes":["creators"]}));
                assert!(!task.is_finished());
                drop(creators);
            }
            joined(task).await;
            assert!(matches!(
                events.recv().await,
                Err(broadcast::error::RecvError::Closed)
            ));
        }
    }

    #[tokio::test]
    async fn cancellation_joins_an_idle_merge_with_owners_still_open() {
        Harness::new(4).stop().await;
    }

    #[tokio::test]
    async fn cancellation_wins_over_ready_owner_traffic() {
        let mut h = Harness::new(4);
        h.creators.send(creator()).unwrap();
        h.platform.send(account()).unwrap();
        h.cancel.cancel();
        joined(h.task).await;
        assert!(matches!(
            h.events.recv().await,
            Err(broadcast::error::RecvError::Closed)
        ));
    }

    #[tokio::test]
    async fn cancellation_joins_a_merge_while_a_publisher_keeps_sending() {
        let mut h = Harness::new(4);
        let publisher = h.platform.clone();
        let stop = h.cancel.clone();
        let producer = tokio::spawn(async move {
            while !stop.is_cancelled() {
                if publisher.send(account()).is_err() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        });
        assert_eq!(h.next().await["sequence"], 1);
        h.stop().await;
        joined(producer).await;
    }

    #[test]
    fn actual_wire_is_secret_free_and_matches_the_complete_web_union() {
        let scopes = CreatorChangeScope::ALL
            .into_iter()
            .map(WireScope::Creator)
            .chain(
                PlatformChangeScope::ALL
                    .into_iter()
                    .map(WireScope::Platform),
            )
            .chain(std::iter::once(WireScope::Bridge(BridgeScope::All)))
            .collect();
        let payload = serde_json::to_value(WireChange {
            sequence: 7,
            scopes,
        })
        .unwrap();
        let object = payload.as_object().unwrap();
        assert_eq!(
            object.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            BTreeSet::from(["scopes", "sequence"])
        );
        assert_eq!(object["sequence"], 7);
        let scopes = object["scopes"].as_array().unwrap();
        let actual = scopes
            .iter()
            .map(|scope| scope.as_str().unwrap())
            .collect::<BTreeSet<_>>();
        assert_eq!(actual.len(), scopes.len());
        assert_eq!(actual.len(), 13);
        let source = include_str!("../../../apps/web/src/lib/runtime-change.ts");
        let union = source
            .split_once("export type RuntimeChangeScope =")
            .unwrap()
            .1
            .split_once(';')
            .unwrap()
            .0;
        let web = union.split('"').skip(1).step_by(2).collect::<BTreeSet<_>>();
        assert_eq!(actual, web);
    }
}
