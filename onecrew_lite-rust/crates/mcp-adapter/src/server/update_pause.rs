//! Owns actual MCP tool futures through cancellation and native update pauses.
use futures::FutureExt;
use std::{
    future::Future,
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::{Notify, oneshot},
    task::JoinHandle,
};

#[derive(Default)]
struct State {
    paused: bool,
    active: usize,
    workers: Vec<JoinHandle<()>>,
}
#[derive(Default)]
struct Inner {
    state: Mutex<State>,
    changed: Notify,
}

/// Process-local control for pausing tool intake without destroying the server
/// or interrupting existing provider work. All MCP surfaces share this owner.
#[derive(Clone, Default)]
pub struct McpAdapterUpdateControl(Arc<Inner>);

/// Resumes the original listener's tool intake on drop, including failed
/// native installation or a cancelled pause attempt.
pub struct McpAdapterUpdatePause {
    control: McpAdapterUpdateControl,
}

impl Drop for McpAdapterUpdatePause {
    fn drop(&mut self) {
        self.control
            .0
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .paused = false;
    }
}

impl McpAdapterUpdateControl {
    /// Close tool intake and drain owned calls. Timeout restores intake and
    /// leaves every running call alive; it never aborts a provider operation.
    pub async fn pause(&self, timeout: Duration) -> Result<McpAdapterUpdatePause, &'static str> {
        {
            let mut state = self.0.state.lock().map_err(|_| "mcp_pause_unavailable")?;
            if state.paused {
                return Err("mcp_already_paused");
            }
            state.paused = true;
        }
        let pause = McpAdapterUpdatePause {
            control: self.clone(),
        };
        let drained = tokio::time::timeout(timeout, async {
            loop {
                if self
                    .0
                    .state
                    .lock()
                    .map_err(|_| "mcp_pause_unavailable")?
                    .active
                    == 0
                {
                    break;
                }
                self.0.changed.notified().await;
            }
            let workers = std::mem::take(
                &mut self
                    .0
                    .state
                    .lock()
                    .map_err(|_| "mcp_pause_unavailable")?
                    .workers,
            );
            for worker in workers {
                worker.await.map_err(|_| "mcp_request_join_failed")?;
            }
            Ok::<_, &'static str>(())
        })
        .await;
        match drained {
            Ok(Ok(())) => Ok(pause),
            Ok(Err(error)) => Err(error),
            Err(_) => Err("application_busy"),
        }
    }

    pub(crate) async fn run_owned<T, F>(&self, work: F) -> Result<T, rmcp::ErrorData>
    where
        T: Send + 'static,
        F: Future<Output = Result<T, rmcp::ErrorData>> + Send + 'static,
    {
        // Reap completed requests, including those whose inbound RPC receiver
        // was cancelled. Running tasks remain registered with this owner.
        let completed = {
            let mut state = self.0.state.lock().map_err(|_| unavailable())?;
            let (completed, running) = state.workers.drain(..).partition(|task| task.is_finished());
            state.workers = running;
            completed
        };
        for worker in completed {
            let _: () = worker.await.map_err(|_| unavailable())?;
        }
        let (send, receive) = oneshot::channel();
        {
            let mut state = self.0.state.lock().map_err(|_| unavailable())?;
            if state.paused {
                return Err(rmcp::ErrorData::internal_error(
                    "MCP is temporarily paused for an application update",
                    Some(serde_json::json!({"code":"application_updating"})),
                ));
            }
            state.active += 1;
            let owner = self.clone();
            state.workers.push(tokio::spawn(async move {
                let result = AssertUnwindSafe(work)
                    .catch_unwind()
                    .await
                    .unwrap_or_else(|_| Err(unavailable()));
                {
                    let mut state = owner
                        .0
                        .state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    state.active -= 1;
                }
                owner.0.changed.notify_one();
                // A cancelled receiver does not cancel the owned operation.
                if send.send(result).is_err() {
                    log::debug!("owned update-pause operation finished after its caller left");
                }
            }));
        }
        receive.await.map_err(|_| unavailable())?
    }
}

fn unavailable() -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error("MCP request could not be completed", None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn idle_running_server_pauses_and_restores_intake() {
        let owner = McpAdapterUpdateControl::default();
        assert_eq!(owner.run_owned(async { Ok(1) }).await.unwrap(), 1);
        let pause = owner.pause(Duration::from_secs(1)).await.unwrap();
        assert!(owner.run_owned(async { Ok(2) }).await.is_err());
        drop(pause);
        assert_eq!(owner.run_owned(async { Ok(3) }).await.unwrap(), 3);
    }

    #[tokio::test]
    async fn cancelled_rpc_keeps_real_work_owned_and_pause_timeout_restores_intake() {
        let owner = McpAdapterUpdateControl::default();
        let (entered, began) = oneshot::channel();
        let (release, finish) = oneshot::channel();
        let captured = owner.clone();
        let caller = tokio::spawn(async move {
            captured
                .run_owned(async move {
                    entered.send(()).expect("test observer is waiting");
                    finish.await.expect("test releases the owned operation");
                    Ok(())
                })
                .await
        });
        began.await.unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        assert!(owner.pause(Duration::from_millis(10)).await.is_err());
        assert_eq!(owner.run_owned(async { Ok(4) }).await.unwrap(), 4);
        release.send(()).unwrap();
        let pause = owner.pause(Duration::from_secs(1)).await.unwrap();
        assert_eq!(owner.0.state.lock().unwrap().active, 0);
        assert!(owner.0.state.lock().unwrap().workers.is_empty());
        drop(pause);
    }

    #[tokio::test]
    async fn panic_finishes_the_owned_request_and_does_not_strand_admission() {
        let owner = McpAdapterUpdateControl::default();
        assert!(
            owner
                .run_owned::<(), _>(async { panic!("fixture panic") })
                .await
                .is_err()
        );
        assert!(owner.pause(Duration::from_secs(1)).await.is_ok());
    }

    #[tokio::test]
    async fn stopping_a_listener_does_not_make_its_owned_provider_work_disappear() {
        let directory = tempfile::tempdir().unwrap();
        let platform =
            Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
        let registry = crate::OperationRegistryHandle::build(
            platform.clone(),
            Arc::new(onecrew_local_runtime::CapabilityCatalog::build().unwrap()),
        )
        .unwrap();
        let handle = crate::McpAdapter::start(
            platform,
            &registry,
            crate::McpAdapterOptions {
                port: 0,
                access_token: "retired-owned-work-fixture".into(),
            },
        )
        .await
        .unwrap();
        let owner = handle.update_control();
        let captured = owner.clone();
        let (entered, began) = oneshot::channel();
        let (release, finish) = oneshot::channel();
        let operation = tokio::spawn(async move {
            captured
                .run_owned(async move {
                    entered.send(()).unwrap();
                    finish.await.unwrap();
                    Ok(())
                })
                .await
        });
        began.await.unwrap();
        handle.stop().await.unwrap();
        assert!(owner.pause(Duration::from_millis(10)).await.is_err());
        release.send(()).unwrap();
        operation.await.unwrap().unwrap();
        assert!(owner.pause(Duration::from_secs(1)).await.is_ok());
    }
}
