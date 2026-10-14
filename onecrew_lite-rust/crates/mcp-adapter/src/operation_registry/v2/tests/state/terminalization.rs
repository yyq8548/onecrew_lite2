use super::*;

#[tokio::test]
async fn submitted_guard_drop_before_dispatch_terminalizes_known_no_effect() {
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let registry = success_attended_registry(Arc::clone(&owner_calls));
    let (_directory, platform, origin) = result_owner();
    let available = execute_and_present_confirmation(
        &registry,
        &platform,
        &origin,
        "onecrew.test.confirm",
        "batch-guard-before",
    )
    .await;
    let submitted = platform
        .submit_result_confirmation(&available.binding)
        .unwrap();
    drop(SubmittedTerminalGuard::new(
        &platform,
        submitted.binding().clone(),
        available.clone(),
    ));
    let current = platform.read_current_result(&available.binding).unwrap();
    assert!(matches!(
        current.state,
        ResultState::Failed(ref failure)
            if failure.code == "confirmation_abandoned_before_dispatch"
    ));
    assert_eq!(owner_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn owner_panic_terminalizes_submitted_as_unknown() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.panic-confirm",
        ]));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.panic-confirm"),
            attended_policy(),
            |_context, _input| async move {
                panic!("injected owner panic");
                #[expect(
                    unreachable_code,
                    reason = "the typed tail fixes the owner future output after the intentional test panic"
                )]
                Ok::<_, TestOwnerError>(TestOutput {
                    value: "unreachable".to_owned(),
                })
            },
            |_| unreachable!("panicking owner has no classified return"),
        )
        .unwrap();
    let registry = Arc::new(builder.build().unwrap());
    let (_directory, platform, origin) = result_owner();
    let available = execute_and_present_confirmation(
        &registry,
        &platform,
        &origin,
        "onecrew.test.panic-confirm",
        "batch-guard-panic",
    )
    .await;
    let binding = available.binding.clone();
    let task = tokio::spawn({
        let registry = Arc::clone(&registry);
        let platform = Arc::clone(&platform);
        async move {
            registry
                .result_state_v2(
                    &platform,
                    SurfaceId::Full,
                    ResultStateInput::Confirm {
                        binding: available.binding,
                    },
                )
                .await
        }
    });
    assert!(task.await.unwrap_err().is_panic());
    let current = platform.read_current_result(&binding).unwrap();
    assert!(matches!(
        current.state,
        ResultState::Unknown(ref unknown)
            if unknown.code == "confirmation_abandoned_after_dispatch"
    ));
}

#[tokio::test]
async fn aborted_blocking_owner_terminalizes_submitted_as_unknown() {
    let started = Arc::new(tokio::sync::Notify::new());
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.blocking-confirm",
        ]));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.blocking-confirm"),
            attended_policy(),
            {
                let started = Arc::clone(&started);
                move |_context, _input| {
                    let started = Arc::clone(&started);
                    async move {
                        started.notify_one();
                        std::future::pending::<Result<TestOutput, TestOwnerError>>().await
                    }
                }
            },
            |_| unreachable!("blocking owner never returns"),
        )
        .unwrap();
    let registry = Arc::new(builder.build().unwrap());
    let (_directory, platform, origin) = result_owner();
    let available = execute_and_present_confirmation(
        &registry,
        &platform,
        &origin,
        "onecrew.test.blocking-confirm",
        "batch-guard-abort",
    )
    .await;
    let binding = available.binding.clone();
    let task = tokio::spawn({
        let registry = Arc::clone(&registry);
        let platform = Arc::clone(&platform);
        async move {
            registry
                .result_state_v2(
                    &platform,
                    SurfaceId::Full,
                    ResultStateInput::Confirm {
                        binding: available.binding,
                    },
                )
                .await
        }
    });
    started.notified().await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    let current = platform.read_current_result(&binding).unwrap();
    assert!(matches!(
        current.state,
        ResultState::Unknown(ref unknown)
            if unknown.code == "confirmation_abandoned_after_dispatch"
    ));
}

#[tokio::test]
async fn owner_unknown_closes_terminal_unknown() {
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&owner_calls);
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.unknown-confirm",
        ]));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.unknown-confirm"),
            attended_policy(),
            move |_context, _input| {
                let counter = Arc::clone(&counter);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Err::<TestOutput, _>(TestOwnerError)
                }
            },
            |_| {
                ClassifiedOwnerFailure::unknown(
                    PublicFailureCode::parse("effect_unknown").unwrap(),
                    None,
                )
            },
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let available = execute_and_present_confirmation(
        &registry,
        &platform,
        &origin,
        "onecrew.test.unknown-confirm",
        "batch-unknown-confirm",
    )
    .await;
    let output = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::Confirm {
                binding: available.binding,
            },
        )
        .await
        .unwrap();
    let ResultStateOutput::Snapshot { snapshot, .. } = output else {
        panic!("unknown owner outcome must return snapshot")
    };
    assert!(matches!(snapshot.state, ResultState::Unknown(_)));
    assert_eq!(owner_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn confirmed_owner_with_unrepresentable_output_closes_presentation_failed() {
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&owner_calls);
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.unrepresentable-confirm",
        ]));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.unrepresentable-confirm"),
            attended_policy(),
            move |_context, _input| {
                let counter = Arc::clone(&counter);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, TestOwnerError>(TestOutput {
                        value: "x".repeat(8 * 1024 + 1),
                    })
                }
            },
            |_| unreachable!("owner succeeds"),
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let available = execute_and_present_confirmation(
        &registry,
        &platform,
        &origin,
        "onecrew.test.unrepresentable-confirm",
        "batch-unrepresentable-confirm",
    )
    .await;
    let output = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::Confirm {
                binding: available.binding,
            },
        )
        .await
        .unwrap();
    let ResultStateOutput::Snapshot { snapshot, .. } = output else {
        panic!("terminal snapshot")
    };
    assert!(matches!(
        snapshot.state,
        ResultState::Failed(ref failure)
            if failure.code == "presentation_unavailable"
    ));
    assert_eq!(owner_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn store_failures_map_to_closed_codes_without_raw_causes() {
    let failure = map_store_failure(ResultStoreError::InvalidInput("RAW_PROVIDER_SECRET"));
    assert_eq!(failure, ResultStateFailure::InvalidInput);
    let encoded = serde_json::to_string(&failure).unwrap();
    assert_eq!(encoded, r#"{"code":"invalid_input"}"#);
    assert!(!encoded.contains("RAW_PROVIDER_SECRET"));
}
