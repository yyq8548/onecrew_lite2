use super::*;

#[test]
fn attended_prompt_dispatches_only_a_call_confirmed_through_result_submission() {
    let binding_calls = Arc::new(AtomicUsize::new(0));
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.delete",
        ]));
    let counter = Arc::clone(&binding_calls);
    let mut delete_policy = policy(ApprovalClass::AttendedPrompt, EffectClass::DeleteLocal);
    delete_policy.approval_request =
        Some(ApprovalRequest::new("Delete the selected test record?").unwrap());
    builder
        .register_operation(
            metadata("onecrew.test.delete", &[], &[SurfaceId::Full]),
            delete_policy,
            move |_context, _input: StrictInput| {
                let counter = Arc::clone(&counter);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, TestOwnerError>(StrictOutput { accepted: true })
                }
            },
            classify_test_error,
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let operation = registry_operation(&registry, "onecrew.test.delete");
    let arguments = json!({"name":"sentinel-secret","action":{"kind":"read"}});

    // A direct execute carries no confirmation: it fails closed before the
    // owner runs, under the same public code as before.
    let unconfirmed = execute_current_operation(
        operation,
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: "request-1".to_owned(),
            connection: None,
            confirmed: false,
        },
        arguments.clone(),
    );
    assert!(matches!(
        unconfirmed,
        Err(crate::operation_registry::execution::ExecuteFailure {
            kind: ExecuteFailureKind::Approval(
                crate::operation_registry::approval::ApprovalFailure::ChannelUnavailable
            ),
            ..
        })
    ));
    assert_eq!(binding_calls.load(Ordering::SeqCst), 0);

    // A confirmed result submission admits exactly the call it confirmed.
    for request_id in ["request-2", "request-3"] {
        execute_current_operation(
            operation,
            ExecutionContext {
                surface: SurfaceId::Full,
                request_id: request_id.to_owned(),
                connection: None,
                confirmed: true,
            },
            arguments.clone(),
        )
        .unwrap();
    }
    assert_eq!(binding_calls.load(Ordering::SeqCst), 2);
}

#[derive(JsonSchema)]
struct FailingOutput;

impl Serialize for FailingOutput {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        Err(serde::ser::Error::custom("sentinel-output-cause"))
    }
}

#[test]
fn post_invocation_serialization_failure_is_never_a_safe_retry() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.external-write",
        ]));
    builder
        .register_operation::<StrictInput, FailingOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.external-write", &[], &[SurfaceId::Full]),
            policy(ApprovalClass::OwnerGuarded, EffectClass::ExternalWrite),
            move |_context, _input| {
                let counter = Arc::clone(&counter);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, TestOwnerError>(FailingOutput)
                }
            },
            classify_test_error,
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let failure = execute_current_operation(
        registry_operation(&registry, "onecrew.test.external-write"),
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: "request".to_owned(),
            connection: None,
            confirmed: false,
        },
        json!({"name":"a","action":{"kind":"read"}}),
    )
    .unwrap_err();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(failure.phase, ExecuteFailurePhase::AfterInvocation);
    assert_eq!(failure.kind, ExecuteFailureKind::OutputSerializationFailed);
    assert!(!failure.retry_safe);
    assert!(failure.effect_may_have_occurred);
    assert!(!format!("{failure:?}").contains("sentinel-output-cause"));
}

#[derive(Serialize, JsonSchema)]
struct ContractMismatchOutput {
    #[schemars(with = "String")]
    value: Value,
}

#[test]
fn post_invocation_output_contract_failure_is_never_a_safe_retry() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.output-contract",
        ]));
    builder
        .register_operation::<StrictInput, ContractMismatchOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.output-contract", &[], &[SurfaceId::Full]),
            policy(ApprovalClass::OwnerGuarded, EffectClass::ExternalWrite),
            move |_context, _input| {
                let counter = Arc::clone(&counter);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, TestOwnerError>(ContractMismatchOutput {
                        value: json!({"unexpected": true}),
                    })
                }
            },
            classify_test_error,
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let failure = execute_current_operation(
        registry_operation(&registry, "onecrew.test.output-contract"),
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: "request".to_owned(),
            connection: None,
            confirmed: false,
        },
        json!({"name":"a","action":{"kind":"read"}}),
    )
    .unwrap_err();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(failure.phase, ExecuteFailurePhase::AfterInvocation);
    assert_eq!(failure.kind, ExecuteFailureKind::OutputContractFailed);
    assert!(!failure.retry_safe);
    assert!(failure.effect_may_have_occurred);
}

#[test]
fn operation_local_classifier_preserves_typed_owner_effect_semantics() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.owner-failure",
        ]));
    builder
        .register_operation::<StrictInput, StrictOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.owner-failure", &[], &[SurfaceId::Full]),
            policy(ApprovalClass::OwnerGuarded, EffectClass::ExternalWrite),
            |_context, _input| async { Err::<StrictOutput, _>(TestOwnerError) },
            |_error| {
                ClassifiedOwnerFailure::unknown(
                    PublicFailureCode::parse("effect_unknown").unwrap(),
                    Some(
                        crate::operation_registry::execution::RecoveryDirective::ReadStatus {
                            reference:
                                crate::operation_registry::execution::BoundedPublicValue::parse(
                                    "audit-123",
                                )
                                .unwrap(),
                        },
                    ),
                )
            },
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let failure = execute_current_operation(
        registry_operation(&registry, "onecrew.test.owner-failure"),
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: "request".to_owned(),
            connection: None,
            confirmed: false,
        },
        json!({"name":"a","action":{"kind":"read"}}),
    )
    .unwrap_err();
    assert!(matches!(
        failure.kind,
        ExecuteFailureKind::OwnerRejected {
            effect: OwnerEffectState::Unknown,
            ..
        }
    ));
    assert!(failure.effect_may_have_occurred);
    assert!(!failure.retry_safe);
}

#[test]
fn execute_failure_projection_is_bounded_secret_safe_and_preserves_effect_semantics() {
    let reference =
        crate::operation_registry::execution::BoundedPublicValue::parse("audit-123").unwrap();
    let failure = crate::operation_registry::execution::ExecuteFailure {
        phase: ExecuteFailurePhase::AfterInvocation,
        kind: ExecuteFailureKind::OwnerRejected {
            code: PublicFailureCode::parse("effect_unknown").unwrap(),
            effect: OwnerEffectState::Unknown,
            recovery: Some(RecoveryDirective::ReadStatus {
                reference: reference.clone(),
            }),
            provider_status: None,
            provider_evidence: None,
            message: None,
            dm_failure: None,
        },
        retry_safe: false,
        effect_may_have_occurred: true,
    };
    let projected = project_execute_failure(&failure);
    assert_eq!(projected.code.as_str(), "effect_unknown");
    assert_eq!(projected.phase, ExecuteFailurePhase::AfterInvocation);
    assert!(!projected.retry_safe);
    assert_eq!(projected.effect, ExecuteEffectState::Unknown);
    assert_eq!(
        projected.recovery,
        Some(RecoveryDirective::ReadStatus { reference })
    );

    let encoded = serde_json::to_string(&projected).unwrap();
    assert!(encoded.len() < 512);
    assert!(!encoded.contains("owner"));
    assert!(!encoded.contains("sentinel-provider-secret"));

    for (kind, expected_code) in [
        (ExecuteFailureKind::UnknownOperation, "unknown_operation"),
        (
            ExecuteFailureKind::InputSchemaRejected,
            "input_schema_rejected",
        ),
        (
            ExecuteFailureKind::Approval(
                crate::operation_registry::approval::ApprovalFailure::ChannelUnavailable,
            ),
            "approval_channel_unavailable",
        ),
    ] {
        let projected =
            project_execute_failure(&crate::operation_registry::execution::ExecuteFailure {
                phase: ExecuteFailurePhase::BeforeInvocation,
                kind,
                retry_safe: true,
                effect_may_have_occurred: false,
            });
        assert_eq!(projected.code.as_str(), expected_code);
        assert_eq!(projected.effect, ExecuteEffectState::NotInvoked);
        assert!(projected.retry_safe);
        assert!(projected.recovery.is_none());
    }
}

#[test]
fn execute_rejects_surface_and_connection_mismatches_before_binding() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::new(std::sync::Mutex::new(None));
    let counter = Arc::clone(&calls);
    let observed_connection = Arc::clone(&observed);
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.connected",
        ]));
    let mut guarded = policy(ApprovalClass::None, EffectClass::ReadOpenWorld);
    guarded.connection = ConnectionRequirement::OperatorAccount {
        toolkit: OperatorToolkitId::X,
    };
    builder
        .register_operation::<StrictInput, StrictOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.connected", &[], &[SurfaceId::Accounts]),
            guarded,
            move |context, _input| {
                let counter = Arc::clone(&counter);
                let observed_connection = Arc::clone(&observed_connection);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    *observed_connection.lock().unwrap() = context.connection;
                    Ok::<_, TestOwnerError>(StrictOutput { accepted: true })
                }
            },
            classify_test_error,
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let id = EntryId::parse("onecrew.test.connected").unwrap();
    let operation = match registry.entry(&id, SurfaceId::Accounts).unwrap() {
        crate::operation_registry::descriptor::RegistryEntry::Operation(operation) => {
            operation.as_ref()
        }
        crate::operation_registry::descriptor::RegistryEntry::Playbook(_)
        | crate::operation_registry::descriptor::RegistryEntry::ToolkitGuide(_) => unreachable!(),
    };
    let instagram = crate::operation_registry::descriptor::ConnectionRef::OperatorAccount {
        toolkit: OperatorToolkitId::Instagram,
        operator_account_id: "account".to_owned(),
    };
    let empty_x = crate::operation_registry::descriptor::ConnectionRef::OperatorAccount {
        toolkit: OperatorToolkitId::X,
        operator_account_id: "".to_owned(),
    };
    let arguments = json!({"name":"a","action":{"kind":"read"}});
    for context in [
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: "wrong-surface".to_owned(),
            connection: None,
            confirmed: false,
        },
        ExecutionContext {
            surface: SurfaceId::Accounts,
            request_id: "wrong-connection".to_owned(),
            connection: Some(&instagram),
            confirmed: false,
        },
        ExecutionContext {
            surface: SurfaceId::Accounts,
            request_id: "empty-connection".to_owned(),
            connection: Some(&empty_x),
            confirmed: false,
        },
    ] {
        assert!(execute_current_operation(operation, context, arguments.clone()).is_err());
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let selected_x = crate::operation_registry::descriptor::ConnectionRef::OperatorAccount {
        toolkit: OperatorToolkitId::X,
        operator_account_id: "account-123".to_owned(),
    };
    execute_current_operation(
        operation,
        ExecutionContext {
            surface: SurfaceId::Accounts,
            request_id: "valid".to_owned(),
            connection: Some(&selected_x),
            confirmed: false,
        },
        arguments,
    )
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(observed.lock().unwrap().as_ref(), Some(&selected_x));
}
