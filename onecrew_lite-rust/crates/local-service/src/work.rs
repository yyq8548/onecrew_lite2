//! Own accepted foreground work independently of an HTTP or MCP caller. A
//! cancelled client never releases admission while a provider future runs.
use futures::FutureExt;
use std::{
    future::Future,
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{Notify, oneshot};

#[derive(Default)]
struct State {
    active: usize,
    paused: bool,
}

#[derive(Default)]
pub(crate) struct WorkOwner {
    state: Mutex<State>,
    changed: Notify,
}

pub(crate) struct WorkPause {
    owner: Arc<WorkOwner>,
    resume: bool,
}

impl WorkPause {
    pub(crate) fn keep_closed(mut self) {
        self.resume = false;
    }
}

impl Drop for WorkPause {
    fn drop(&mut self) {
        if self.resume {
            self.owner
                .state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .paused = false;
        }
    }
}

struct Completion(Arc<WorkOwner>);
impl Drop for Completion {
    fn drop(&mut self) {
        self.0
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .active -= 1;
        self.0.changed.notify_one();
    }
}

impl WorkOwner {
    pub(crate) async fn run<T: Send + 'static>(
        self: &Arc<Self>,
        work: impl Future<Output = T> + Send + 'static,
    ) -> Result<T, String> {
        let completion = {
            let mut state = self.state.lock().map_err(|_| "host_work_unavailable")?;
            if state.paused {
                return Err("host_busy".into());
            }
            state.active += 1;
            Completion(self.clone())
        };
        let (send, receive) = oneshot::channel();
        tokio::spawn(async move {
            let result = AssertUnwindSafe(work)
                .catch_unwind()
                .await
                .map_err(|_| "host_work_failed".to_owned());
            drop(completion);
            if send.send(result).is_err() {
                log::debug!("host work completed after its caller stopped waiting");
            }
        });
        receive.await.map_err(|_| "host_work_failed".to_owned())?
    }

    /// Caller must retain the returned guard until the identity/storage change
    /// or shutdown completes. Timeout restores intake and never kills work.
    pub(crate) async fn pause(self: &Arc<Self>, duration: Duration) -> Result<WorkPause, String> {
        {
            let mut state = self.state.lock().map_err(|_| "host_work_unavailable")?;
            if state.paused {
                return Err("host_busy".into());
            }
            state.paused = true;
        }
        let pause = WorkPause {
            owner: self.clone(),
            resume: true,
        };
        tokio::time::timeout(duration, async {
            loop {
                let notified = self.changed.notified();
                if self.state.lock().unwrap_or_else(|p| p.into_inner()).active == 0 {
                    break;
                }
                notified.await;
            }
        })
        .await
        .map_err(|_| "host_busy: existing work is still running".to_owned())?;
        Ok(pause)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn disconnected_outreach_work_remains_owned_through_record_and_terminal() {
        use onecrew_local_runtime::{
            CallDisposition, ExecutedBatch, ExecutedCallOutcome, ExecutedResultInput,
            InitialResultState, LocalPlatform, OwnerActionPayload, PendingUserAction,
            PresentationProjection, PresentationSection, ResultOrigin, ResultPresentationKind,
            ResultResolution, ResultState, ResultTerminalUpdate, ResultViewKind, TerminalResult,
        };
        use std::{
            future::poll_fn,
            sync::atomic::{AtomicUsize, Ordering},
            task::Poll,
        };

        let data = tempfile::tempdir().unwrap();
        let platform = Arc::new(LocalPlatform::open(data.path()).unwrap());
        let origin = ResultOrigin::new("web:host-outreach-fixture").unwrap();
        let frame = ExecutedBatch {
            outcomes: vec![ExecutedCallOutcome {
                call_id: "outreach-call".into(),
                operation_id: "fixture.outreach_lifecycle".into(),
                disposition: CallDisposition::NotRun {
                    reason: "confirmation_required".into(),
                },
                summary: "Awaiting review".into(),
                resource_refs: vec![],
            }],
        };
        let presentation = PresentationProjection {
            kind: ResultPresentationKind::new("outreach_draft").unwrap(),
            title: "Host lifecycle fixture".into(),
            summary: None,
            sections: vec![PresentationSection::Notice {
                heading: None,
                body: "Prepared fixture".into(),
                tone: onecrew_local_runtime::NoticeTone::Neutral,
            }],
        };
        let reference = platform
            .mint_execution_result(ExecutedResultInput {
                origin: origin.clone(),
                view_kind: ResultViewKind::Confirmation,
                batch: frame.clone(),
                presentation: presentation.clone(),
                pending_action: Some(PendingUserAction::Confirm {
                    prompt: "Continue fixture?".into(),
                    label: "Confirm".into(),
                    payload: OwnerActionPayload::new(b"host-owned fixture".to_vec()).unwrap(),
                }),
                initial_state: InitialResultState::Available,
            })
            .unwrap();
        let initial = platform.present_result(&reference, &origin).unwrap();
        let binding = initial.binding.clone();
        let owner = Arc::new(WorkOwner::default());
        let phases = Arc::new(AtomicUsize::new(0));
        let (provider_entered, provider_waiting) = oneshot::channel();
        let (provider_release, provider_continue) = oneshot::channel();
        let (record_entered, record_waiting) = oneshot::channel();
        let (record_release, record_continue) = std::sync::mpsc::channel();
        let (terminal_entered, terminal_waiting) = oneshot::channel();
        let (terminal_release, terminal_continue) = oneshot::channel();
        let host = owner.clone();
        let runtime = platform.clone();
        let recorded = phases.clone();
        // Dependency waits are controlled, not live SDK effects. Ownership,
        // blocking dispatch and the result-store terminal transition are real.
        // Runtime's outreach tests independently cover actual claim/record policy.
        let caller = tokio::spawn(async move {
            host.run(async move {
                let submitted = runtime.submit_result_confirmation(&binding).unwrap();
                provider_entered.send(()).unwrap();
                provider_continue.await.unwrap();
                recorded.store(1, Ordering::SeqCst);
                let record_phase = recorded.clone();
                crate::blocking::run_blocking("outreach_record_fixture", move || {
                    record_entered.send(()).unwrap();
                    record_continue
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                    record_phase.store(2, Ordering::SeqCst);
                })
                .await
                .unwrap();
                terminal_entered.send(()).unwrap();
                terminal_continue.await.unwrap();
                let mut completed = frame;
                completed.outcomes[0].disposition = CallDisposition::Succeeded;
                completed.outcomes[0].summary = "Fixture recorded".into();
                runtime
                    .transition_result_terminal(
                        submitted.binding(),
                        ResultTerminalUpdate {
                            terminal: TerminalResult::Resolved(ResultResolution {
                                code: "fixture_recorded".into(),
                                summary: "Fixture recorded".into(),
                            }),
                            batch: completed,
                            presentation,
                            continuation: None,
                        },
                    )
                    .unwrap();
                recorded.store(3, Ordering::SeqCst);
            })
            .await
        });
        provider_waiting.await.unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        let mut stopping = Box::pin(owner.pause(Duration::from_secs(5)));
        poll_fn(|cx| {
            assert!(stopping.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        assert!(
            owner.run(async {}).await.is_err(),
            "shutdown closes new intake"
        );
        provider_release.send(()).unwrap();
        record_waiting.await.unwrap();
        assert_eq!(phases.load(Ordering::SeqCst), 1);
        poll_fn(|cx| {
            assert!(stopping.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        record_release.send(()).unwrap();
        terminal_waiting.await.unwrap();
        assert_eq!(phases.load(Ordering::SeqCst), 2);
        assert!(matches!(
            platform
                .read_current_result(&initial.binding)
                .unwrap()
                .state,
            ResultState::Submitted { .. }
        ));
        poll_fn(|cx| {
            assert!(stopping.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        terminal_release.send(()).unwrap();
        let pause = stopping.await.unwrap();
        assert_eq!(phases.load(Ordering::SeqCst), 3);
        assert!(
            matches!(platform.read_current_result(&initial.binding).unwrap().state,
            ResultState::Resolved(result) if result.code == "fixture_recorded")
        );
        pause.keep_closed();
        assert!(owner.run(async {}).await.is_err());
    }

    #[tokio::test]
    async fn disconnect_keeps_work_owned_and_pause_timeout_restores_intake() {
        let owner = Arc::new(WorkOwner::default());
        let (entered, waiting) = oneshot::channel();
        let (finish, done) = oneshot::channel();
        let worker = owner.clone();
        let caller = tokio::spawn(async move {
            worker
                .run(async move {
                    entered.send(()).expect("test observer is waiting");
                    done.await.unwrap();
                    42
                })
                .await
        });
        waiting.await.unwrap();
        caller.abort();
        assert!(owner.pause(Duration::from_millis(10)).await.is_err());
        assert_eq!(owner.run(async { 1 }).await.unwrap(), 1);
        finish.send(()).unwrap();
        let pause = owner.pause(Duration::from_secs(1)).await.unwrap();
        assert!(owner.run(async { 2 }).await.is_err());
        drop(pause);
        assert_eq!(owner.run(async { 3 }).await.unwrap(), 3);
    }
}
