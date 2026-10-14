use super::*;

pub(super) fn attended_policy() -> OperationPolicy {
    let mut operation_policy = policy(ApprovalClass::AttendedPrompt, EffectClass::ExternalWrite);
    operation_policy.approval_request =
        Some(ApprovalRequest::new("Confirm the staged operation?").unwrap());
    operation_policy
}

pub(super) fn success_attended_registry(owner_calls: Arc<AtomicUsize>) -> OperationRegistry {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.confirm",
        ]));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.confirm"),
            attended_policy(),
            move |_context, input| {
                let owner_calls = Arc::clone(&owner_calls);
                async move {
                    owner_calls.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, TestOwnerError>(TestOutput {
                        value: format!("completed:{}", input.value),
                    })
                }
            },
            |_| unreachable!("test owner succeeds"),
        )
        .unwrap();
    builder.build().unwrap()
}

pub(super) async fn execute_and_present_confirmation(
    registry: &OperationRegistry,
    platform: &onecrew_local_runtime::LocalPlatform,
    origin: &ResultOrigin,
    operation_id: &str,
    batch_request_id: &str,
) -> onecrew_local_runtime::ResultSnapshot {
    let input = ToolExecuteInput {
        calls: vec![call(
            "call.confirm",
            operation_id,
            revision(registry, operation_id),
            json!({"value":"approved"}),
        )],
        presentation: PresentationRequest::Never,
    };
    let result_ref = registry
        .tool_execute_v2(platform, origin, SurfaceId::Full, batch_request_id, &input)
        .await
        .unwrap()
        .result_ref
        .unwrap();
    result_present_v2(
        platform,
        origin,
        ResultPresentInput {
            result_ref,
            locale: Default::default(),
        },
    )
    .unwrap()
    .snapshot
}
