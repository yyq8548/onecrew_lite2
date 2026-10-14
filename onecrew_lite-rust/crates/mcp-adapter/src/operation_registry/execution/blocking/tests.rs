use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use serde_json::json;

use super::super::test_support::{TestInput, TestOutput, metadata, operation};
use crate::operation_registry::{
    descriptor::{ConnectionRequirement, EffectClass, EntryId, SurfaceId},
    execution::{
        BlockingTaskFailure, ClassifiedOwnerFailure, ExecutionContext, PublicFailureCode,
        dispatch_prepared, prepare_invocation, run_blocking,
    },
    registration::{OperationPolicy, OperationRegistry, RegistryBuilder},
};

const OWNER_HOLD: Duration = Duration::from_millis(500);
const RESPONSIVE_BUDGET: Duration = Duration::from_millis(100);
const SLOW_ID: &str = "onecrew.test.blocking-slow";
const FAST_ID: &str = "onecrew.test.blocking-fast";

fn classify(_: String) -> ClassifiedOwnerFailure {
    ClassifiedOwnerFailure::known_no_effect(
        PublicFailureCode::parse("test_rejected").unwrap(),
        false,
        None,
    )
}

/// One slow operation whose owner call holds a thread for `OWNER_HOLD`
/// (either through `run_blocking` or inline on the worker) and one fast
/// operation that only awaits.
fn registry(entered: Arc<AtomicUsize>, wrapped: bool) -> Arc<OperationRegistry> {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            SLOW_ID, FAST_ID,
        ]));
    builder
        .register_operation::<TestInput, TestOutput, String, _, _, _>(
            metadata(SLOW_ID),
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                crate::operation_registry::execution::ExecutionDeclaration::Sequential,
            ),
            move |_context, input| {
                let entered = Arc::clone(&entered);
                async move {
                    let owner_call = move || {
                        entered.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(OWNER_HOLD);
                        Ok::<_, String>(TestOutput {
                            accepted: input.name == "slow",
                        })
                    };
                    if wrapped {
                        run_blocking(owner_call).await
                    } else {
                        owner_call()
                    }
                }
            },
            classify,
        )
        .unwrap();
    builder
        .register_operation::<TestInput, TestOutput, String, _, _, _>(
            metadata(FAST_ID),
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                crate::operation_registry::execution::ExecutionDeclaration::Sequential,
            ),
            |_context, input| async move {
                tokio::task::yield_now().await;
                Ok::<_, String>(TestOutput {
                    accepted: input.name == "fast",
                })
            },
            classify,
        )
        .unwrap();
    Arc::new(builder.build().unwrap())
}

async fn dispatch(registry: &OperationRegistry, id: &str, name: &str) -> serde_json::Value {
    let id = EntryId::parse(id).unwrap();
    let entry = operation(registry, &id);
    let prepared = prepare_invocation(
        entry,
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: format!("{name}-request"),
            connection: None,
            confirmed: false,
        },
        entry.input.revision(),
        json!({ "name": name }),
    )
    .unwrap();
    dispatch_prepared(prepared).await.unwrap()
}

/// Runs two slow dispatches on the two runtime workers, waits until both
/// owner calls have started, then measures a fast dispatch that also needs
/// a worker. The measurement runs on a spawned task on purpose: the test
/// body itself executes on the `block_on` thread, which parked workers
/// cannot stall.
async fn fast_latency_behind_two_slow_owner_calls(wrapped: bool) -> Duration {
    let entered = Arc::new(AtomicUsize::new(0));
    let registry = registry(Arc::clone(&entered), wrapped);
    let slow = (0..2)
        .map(|_| {
            let registry = Arc::clone(&registry);
            tokio::spawn(async move { dispatch(&registry, SLOW_ID, "slow").await })
        })
        .collect::<Vec<_>>();
    let deadline = Instant::now() + Duration::from_secs(5);
    while entered.load(Ordering::SeqCst) < 2 {
        assert!(Instant::now() < deadline, "slow owner calls never started");
        std::thread::sleep(Duration::from_millis(1));
    }

    let started = Instant::now();
    let fast_registry = Arc::clone(&registry);
    let output = tokio::spawn(async move { dispatch(&fast_registry, FAST_ID, "fast").await })
        .await
        .unwrap();
    let elapsed = started.elapsed();
    assert_eq!(output, json!({ "accepted": true }));
    for handle in slow {
        assert_eq!(handle.await.unwrap(), json!({ "accepted": true }));
    }
    elapsed
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrapped_owner_calls_leave_runtime_workers_free_for_other_operations() {
    let elapsed = fast_latency_behind_two_slow_owner_calls(true).await;
    assert!(
        elapsed < RESPONSIVE_BUDGET,
        "fast operation waited {elapsed:?} behind blocking owner calls"
    );
}

/// Control for the measurement above: the forbidden inline shape parks
/// both workers, so the same fast operation cannot run before an owner
/// call releases its worker.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inline_owner_calls_park_runtime_workers() {
    let elapsed = fast_latency_behind_two_slow_owner_calls(false).await;
    assert!(
        elapsed >= OWNER_HOLD / 2,
        "inline owner calls did not stall the fast operation ({elapsed:?})"
    );
}

#[tokio::test]
async fn run_blocking_returns_owner_result_and_maps_join_failure_once() {
    let value = run_blocking(|| Ok::<_, String>(7)).await;
    assert_eq!(value, Ok(7));

    let rejected = run_blocking(|| Err::<u8, _>("owner rejected".to_owned())).await;
    assert_eq!(rejected, Err("owner rejected".to_owned()));

    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let panicked = run_blocking(|| -> Result<u8, String> { panic!("owner panicked") }).await;
    std::panic::set_hook(hook);
    assert_eq!(panicked, Err(String::from(BlockingTaskFailure::Panicked)));
    assert_eq!(
        String::from(BlockingTaskFailure::Panicked),
        "blocking owner call panicked"
    );
    assert_eq!(
        String::from(BlockingTaskFailure::Cancelled),
        "blocking owner call was cancelled"
    );
}
