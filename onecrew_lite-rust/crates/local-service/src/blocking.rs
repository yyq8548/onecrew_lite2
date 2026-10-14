//! One blocking-pool boundary for synchronous Desktop owner calls.

use std::sync::{Arc, OnceLock};
use tokio::sync::Semaphore;

const WEB_BLOCKING_CAPACITY: usize = 8;
static WEB_BLOCKING: OnceLock<Arc<Semaphore>> = OnceLock::new();

/// Run one bounded synchronous owner segment without parking a Tokio worker.
///
/// The closure's value is returned unchanged, so callers can preserve the
/// owner's typed `Result` and map only the outer task failure here. Labels are
/// fixed command identifiers; panic payloads never cross the host boundary.
pub(crate) async fn run_blocking<T, F>(label: &'static str, call: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    match tokio::task::spawn_blocking(call).await {
        Ok(result) => Ok(result),
        Err(error) if error.is_panic() => Err(format!("{label} blocking owner call panicked")),
        Err(_) => Err(format!("{label} blocking owner call was cancelled")),
    }
}

/// Admit a Web-only synchronous vault/OS segment before occupying the pool.
/// Creator work uses its own read/write/CPU owner admission instead.
pub(crate) async fn run_web_blocking<T, F>(label: &'static str, call: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    run_blocking_admitted(
        label,
        WEB_BLOCKING
            .get_or_init(|| Arc::new(Semaphore::new(WEB_BLOCKING_CAPACITY)))
            .clone(),
        call,
    )
    .await
}

async fn run_blocking_admitted<T, F>(
    label: &'static str,
    admission: Arc<Semaphore>,
    call: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let permit = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        admission.acquire_owned(),
    )
    .await
    .map_err(|_| format!("{label} blocking capacity is temporarily full"))?
    .map_err(|_| format!("{label} blocking intake is closed"))?;
    run_blocking(label, move || {
        let _permit = permit;
        call()
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
            mpsc,
        },
        time::{Duration, Instant},
    };

    use super::run_blocking;

    #[tokio::test]
    async fn blocking_admission_rejects_overload_and_closed_before_owner_call() {
        let admission = Arc::new(tokio::sync::Semaphore::new(1));
        let held = admission.clone().acquire_owned().await.unwrap();
        let called = Arc::new(AtomicBool::new(false));
        let worker_called = called.clone();
        let result = super::run_blocking_admitted("fixture", admission.clone(), move || {
            worker_called.store(true, Ordering::SeqCst);
        })
        .await;
        assert_eq!(
            result.unwrap_err(),
            "fixture blocking capacity is temporarily full"
        );
        assert!(!called.load(Ordering::SeqCst));
        drop(held);
        admission.close();
        let worker_called = called.clone();
        let result = super::run_blocking_admitted("fixture", admission, move || {
            worker_called.store(true, Ordering::SeqCst);
        })
        .await;
        assert_eq!(result.unwrap_err(), "fixture blocking intake is closed");
        assert!(!called.load(Ordering::SeqCst));
    }

    const OWNER_HOLD: Duration = Duration::from_millis(500);
    const RESPONSIVE_BUDGET: Duration = Duration::from_millis(100);
    async fn fast_latency_behind_two_slow_calls(wrapped: bool) -> Duration {
        let entered = Arc::new(AtomicUsize::new(0));
        let slow = (0..2)
            .map(|_| {
                let entered = Arc::clone(&entered);
                tokio::spawn(async move {
                    let owner = move || {
                        entered.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(OWNER_HOLD);
                    };
                    if wrapped {
                        run_blocking("test_slow_owner", owner).await.unwrap();
                    } else {
                        owner();
                    }
                })
            })
            .collect::<Vec<_>>();
        let deadline = Instant::now() + Duration::from_secs(5);
        while entered.load(Ordering::SeqCst) < 2 {
            assert!(Instant::now() < deadline, "slow owner calls never started");
            std::thread::sleep(Duration::from_millis(1));
        }

        let started = Instant::now();
        tokio::spawn(async { tokio::task::yield_now().await })
            .await
            .unwrap();
        let elapsed = started.elapsed();
        for handle in slow {
            handle.await.unwrap();
        }
        elapsed
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn blocking_dispatch_keeps_runtime_workers_responsive() {
        let wrapped = fast_latency_behind_two_slow_calls(true).await;
        assert!(
            wrapped < RESPONSIVE_BUDGET,
            "wrapped probe waited {wrapped:?}"
        );

        let inline = fast_latency_behind_two_slow_calls(false).await;
        assert!(
            inline >= OWNER_HOLD / 2,
            "inline control did not stall workers ({inline:?})"
        );
    }

    #[tokio::test]
    async fn run_blocking_returns_owner_value_and_maps_join_failure_once() {
        let owner = run_blocking("test_owner_result", || {
            Err::<u8, _>("owner rejected".to_owned())
        })
        .await;
        assert_eq!(owner, Ok(Err("owner rejected".to_owned())));

        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let panicked = run_blocking("test_owner_panic", || -> u8 {
            panic!("secret panic payload")
        })
        .await;
        std::panic::set_hook(hook);
        assert_eq!(
            panicked,
            Err("test_owner_panic blocking owner call panicked".to_owned())
        );
    }

    #[tokio::test]
    async fn started_blocking_owner_finishes_after_caller_cancellation() {
        let completed = Arc::new(AtomicBool::new(false));
        let completed_by_owner = Arc::clone(&completed);
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let caller = tokio::spawn(async move {
            run_blocking("test_cancelled_caller", move || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                completed_by_owner.store(true, Ordering::SeqCst);
            })
            .await
        });
        entered_rx.await.unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        release_tx.send(()).unwrap();

        tokio::time::timeout(Duration::from_secs(2), async {
            while !completed.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("started owner did not finish after caller cancellation");
    }
}
