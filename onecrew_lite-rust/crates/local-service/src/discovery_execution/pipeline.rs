//! Ordered projection work. Keep one active job and only the latest pending job.
use futures::{FutureExt, future::BoxFuture};
use std::future::Future;
use tokio::task::JoinHandle;

pub(super) const SCORE_CONCURRENCY: usize = onecrew_local_runtime::DISCOVERY_SCORE_CONCURRENCY;
pub(super) const READ_AHEAD: usize = 3;

pub(super) fn score_capacity(remaining: u32) -> usize {
    (remaining as usize).min(SCORE_CONCURRENCY + READ_AHEAD)
}

#[derive(Clone)]
pub(super) struct ScoreSlots(std::sync::Arc<tokio::sync::Semaphore>);
impl Default for ScoreSlots {
    fn default() -> Self {
        Self(std::sync::Arc::new(tokio::sync::Semaphore::new(
            SCORE_CONCURRENCY,
        )))
    }
}
impl ScoreSlots {
    pub(super) fn active(&self) -> usize {
        SCORE_CONCURRENCY - self.0.available_permits()
    }
    pub(super) async fn acquire(
        &self,
        cancel: &tokio_util::sync::CancellationToken,
        shutdown: &tokio_util::sync::CancellationToken,
    ) -> Option<tokio::sync::OwnedSemaphorePermit> {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => None,
            _ = shutdown.cancelled() => None,
            permit = self.0.clone().acquire_owned() => {
                if cancel.is_cancelled() || shutdown.is_cancelled() { None } else { permit.ok() }
            }
        }
    }
}

#[derive(Default)]
pub(super) struct LatestWork {
    active: Option<JoinHandle<()>>,
    pending: Option<BoxFuture<'static, ()>>,
}
impl LatestWork {
    pub(super) fn request(&mut self, work: impl Future<Output = ()> + Send + 'static) {
        self.pending = Some(work.boxed());
        self.pump();
    }
    fn pump(&mut self) {
        if self.active.is_none()
            && let Some(work) = self.pending.take()
        {
            self.active = Some(tokio::spawn(work));
        }
    }
    pub(super) async fn reap(&mut self) {
        if self.active.as_ref().is_some_and(|task| task.is_finished()) {
            let _ = self.active.take().unwrap().await;
            self.pump();
        }
    }
    pub(super) async fn drain(&mut self) {
        while let Some(task) = self.active.take() {
            let _ = task.await;
            self.pump();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn two_scores_overlap_third_waits_and_pause_discards_queued_work() {
        let slots = ScoreSlots::default();
        let cancel = tokio_util::sync::CancellationToken::new();
        let shutdown = tokio_util::sync::CancellationToken::new();
        let first = slots.acquire(&cancel, &shutdown).await.unwrap();
        let second = slots.acquire(&cancel, &shutdown).await.unwrap();
        let third = slots.acquire(&cancel, &shutdown);
        tokio::pin!(third);
        assert!(futures::poll!(&mut third).is_pending());
        drop(first);
        let third = third.await.unwrap();
        let queued = slots.acquire(&cancel, &shutdown);
        tokio::pin!(queued);
        assert!(futures::poll!(&mut queued).is_pending());
        cancel.cancel();
        assert!(queued.await.is_none());
        // Pause never revokes admitted permits; their receipts can finish.
        assert_eq!(slots.0.available_permits(), 0);
        drop(second);
        drop(third);
        assert_eq!(slots.0.available_permits(), SCORE_CONCURRENCY);
        assert!(slots.acquire(&cancel, &shutdown).await.is_none());
    }

    #[tokio::test]
    async fn shutdown_wins_even_with_available_score_capacity() {
        let slots = ScoreSlots::default();
        let shutdown = tokio_util::sync::CancellationToken::new();
        shutdown.cancel();
        assert!(
            slots
                .acquire(&tokio_util::sync::CancellationToken::new(), &shutdown)
                .await
                .is_none()
        );
    }

    #[test]
    fn recommendation_slots_bound_active_and_buffered_authors() {
        assert_eq!(score_capacity(30), 5); // Two active, at most three prepared.
        assert_eq!(score_capacity(3), 3);
        assert_eq!(score_capacity(1), 1);
        assert_eq!(score_capacity(0), 0);
    }

    #[tokio::test]
    async fn sync_overlaps_reads_and_keeps_only_latest_pending_snapshot() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let (started, started_rx) = oneshot::channel();
        let (release, release_rx) = oneshot::channel();
        let mut lane = LatestWork::default();
        let first = order.clone();
        lane.request(async move {
            started.send(()).unwrap();
            release_rx.await.unwrap();
            first.lock().unwrap().push(1);
        });
        started_rx.await.unwrap();
        // Provider work can finish while the first HTTP sync remains blocked.
        let work = Arc::new(crate::work::WorkOwner::default());
        assert_eq!(work.run(async { 42 }).await.unwrap(), 42);
        for value in [2, 3] {
            let order = order.clone();
            lane.request(async move {
                order.lock().unwrap().push(value);
            });
        }
        assert!(order.lock().unwrap().is_empty());
        release.send(()).unwrap();
        lane.drain().await;
        assert_eq!(*order.lock().unwrap(), vec![1, 3]);
        assert!(lane.active.is_none() && lane.pending.is_none());
    }

    #[tokio::test]
    async fn paused_reader_does_not_cancel_already_admitted_score() {
        let work = Arc::new(crate::work::WorkOwner::default());
        let cancel = tokio_util::sync::CancellationToken::new();
        let (started, started_rx) = oneshot::channel();
        let (release, release_rx) = oneshot::channel();
        let owner = work.clone();
        let score = tokio::spawn(async move {
            owner
                .run(async move {
                    started.send(()).unwrap();
                    release_rx.await.unwrap();
                    7
                })
                .await
        });
        started_rx.await.unwrap();
        assert_eq!(
            super::super::owned_read(&work, cancel.clone(), async { Ok(3) })
                .await
                .unwrap()
                .unwrap(),
            3
        );
        cancel.cancel();
        assert!(
            super::super::owned_read::<()>(&work, cancel, async { panic!("must not dispatch") })
                .await
                .unwrap()
                .is_err()
        );
        release.send(()).unwrap();
        assert_eq!(score.await.unwrap().unwrap(), 7);
    }
}
