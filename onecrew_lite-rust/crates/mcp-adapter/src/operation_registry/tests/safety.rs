use super::*;

#[test]
fn operation_safety_requires_closed_modes_and_exact_bounded_pointers() {
    let register = |id: &str, safety: OperationSafety| {
        let mut builder =
            RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
                "onecrew.test.confirmation-mode-invalid",
                "onecrew.test.confirmation-pointer-invalid",
                "onecrew.test.idempotency-mode-invalid",
                "onecrew.test.safety-duplicate",
                "onecrew.test.safety-oversized",
                "onecrew.test.safety-pointerless-recovery",
                "onecrew.test.safety-pointerless-warning",
                "onecrew.test.safety-undeclared-warning",
                "onecrew.test.safety-valid",
                "onecrew.test.warning-pointer-invalid",
            ]));
        let mut operation_policy = policy(ApprovalClass::OwnerGuarded, EffectClass::ExternalWrite);
        operation_policy.safety = safety;
        builder.register_operation::<StrictInput, StrictOutput, TestOwnerError, _, _, _>(
            metadata(id, &[], &[SurfaceId::Full]),
            operation_policy,
            |_context, _input| async { Ok::<_, TestOwnerError>(StrictOutput { accepted: true }) },
            classify_test_error,
        )
    };

    let pointer = |value| InstancePointer::parse(value).unwrap();
    let complete = OperationSafety {
        confirmation: ConfirmationSafety {
            mode: ConfirmationMode::OwnerParameters,
            parameter_pointers: vec![pointer("/name")],
        },
        idempotency: IdempotencySafety {
            mode: IdempotencyMode::OwnerInputIdentity,
            identity_pointers: vec![pointer("/name"), pointer("/action/kind")],
        },
        readback: ReadbackSafety {
            mode: ReadbackMode::InlineOwnerVerified,
            operation_ids: Vec::new(),
        },
        audit: AuditMode::OwnerRecorded,
        warnings_present: true,
        recovery_present: true,
        warning_output_pointers: vec![pointer("/accepted")],
        recovery_output_pointers: vec![pointer("/accepted")],
    };
    register("onecrew.test.safety-valid", complete.clone()).unwrap();

    for (id, mut invalid, expected_role) in [
        (
            "onecrew.test.confirmation-pointer-invalid",
            complete.clone(),
            ParameterPointerRole::Input,
        ),
        (
            "onecrew.test.warning-pointer-invalid",
            complete.clone(),
            ParameterPointerRole::Output,
        ),
    ] {
        if expected_role == ParameterPointerRole::Input {
            invalid.confirmation.parameter_pointers = vec![pointer("/missing")];
        } else {
            invalid.warning_output_pointers = vec![pointer("/missing")];
        }
        assert!(matches!(
            register(id, invalid),
            Err(RegistryBuildError::SafetyPointerMismatch { role, .. }) if role == expected_role
        ));
    }

    let mut duplicate = complete.clone();
    duplicate.idempotency.identity_pointers = vec![pointer("/name"), pointer("/name")];
    assert!(matches!(
        register("onecrew.test.safety-duplicate", duplicate),
        Err(RegistryBuildError::DuplicateSafetyPointer {
            field: "idempotency.identity_pointers"
        })
    ));

    let mut oversized = complete.clone();
    oversized.recovery_output_pointers = vec![pointer("/accepted"); 13];
    assert!(matches!(
        register("onecrew.test.safety-oversized", oversized),
        Err(RegistryBuildError::SafetyPointerOutOfBounds {
            field: "recovery_output_pointers"
        })
    ));

    let mut undeclared_warning = complete.clone();
    undeclared_warning.warnings_present = false;
    assert_eq!(
        register("onecrew.test.safety-undeclared-warning", undeclared_warning),
        Err(RegistryBuildError::InvalidSafetyPolicy)
    );

    let mut pointerless_warning = complete.clone();
    pointerless_warning.warning_output_pointers.clear();
    assert_eq!(
        register(
            "onecrew.test.safety-pointerless-warning",
            pointerless_warning
        ),
        Err(RegistryBuildError::InvalidSafetyPolicy)
    );
    let mut pointerless_recovery = complete.clone();
    pointerless_recovery.recovery_output_pointers.clear();
    assert_eq!(
        register(
            "onecrew.test.safety-pointerless-recovery",
            pointerless_recovery
        ),
        Err(RegistryBuildError::InvalidSafetyPolicy)
    );

    for (id, mut invalid) in [
        ("onecrew.test.confirmation-mode-invalid", complete.clone()),
        ("onecrew.test.idempotency-mode-invalid", complete),
    ] {
        if id.contains("confirmation") {
            invalid.confirmation.mode = ConfirmationMode::None;
        } else {
            invalid.idempotency.mode = IdempotencyMode::None;
        }
        assert_eq!(
            register(id, invalid),
            Err(RegistryBuildError::InvalidSafetyPolicy)
        );
    }
}

#[test]
fn operation_safety_readback_references_are_unique_operations_on_every_surface() {
    fn add_write_with_readback(
        builder: &mut RegistryBuilder,
        id: &str,
        surfaces: &[SurfaceId],
        readback_ids: &[&str],
    ) {
        let mut operation_policy = policy(ApprovalClass::OwnerGuarded, EffectClass::ExternalWrite);
        operation_policy.safety.readback = ReadbackSafety {
            mode: ReadbackMode::FollowUpOperations,
            operation_ids: readback_ids
                .iter()
                .map(|id| EntryId::parse(*id).unwrap())
                .collect(),
        };
        builder
            .register_operation::<StrictInput, StrictOutput, TestOwnerError, _, _, _>(
                metadata(id, &[], surfaces),
                operation_policy,
                |_context, _input| async {
                    Ok::<_, TestOwnerError>(StrictOutput { accepted: true })
                },
                classify_test_error,
            )
            .unwrap();
    }

    let mut valid =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.readback",
            "onecrew.test.write",
        ]));
    add_write_with_readback(
        &mut valid,
        "onecrew.test.write",
        &[SurfaceId::Full],
        &["onecrew.test.readback"],
    );
    add_operation(&mut valid, "onecrew.test.readback", &[], &[SurfaceId::Full]).unwrap();
    valid.build().unwrap();

    let mut unknown =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.duplicate-readback",
            "onecrew.test.missing",
            "onecrew.test.readback",
            "onecrew.test.write",
        ]));
    add_write_with_readback(
        &mut unknown,
        "onecrew.test.write",
        &[SurfaceId::Full],
        &["onecrew.test.missing"],
    );
    assert!(matches!(
        unknown.build(),
        Err(RegistryBuildError::UnknownReadbackOperation { .. })
    ));

    let mut wrong_kind =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.duplicate-readback",
            "onecrew.test.missing",
            "onecrew.test.readback",
            "onecrew.test.write",
        ]));
    add_write_with_readback(
        &mut wrong_kind,
        "onecrew.test.write",
        &[SurfaceId::Full],
        &["playbook.test.write"],
    );
    wrong_kind
        .register_playbook(playbook_entry(
            "playbook.test.write",
            &[SurfaceId::Full],
            "onecrew.test.write",
        ))
        .unwrap();
    assert!(matches!(
        wrong_kind.build(),
        Err(RegistryBuildError::ReadbackReferenceKindMismatch { .. })
    ));

    let mut hidden =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.duplicate-readback",
            "onecrew.test.missing",
            "onecrew.test.readback",
            "onecrew.test.write",
        ]));
    add_write_with_readback(
        &mut hidden,
        "onecrew.test.write",
        &[SurfaceId::Full, SurfaceId::Accounts],
        &["onecrew.test.readback"],
    );
    add_operation(
        &mut hidden,
        "onecrew.test.readback",
        &[],
        &[SurfaceId::Full],
    )
    .unwrap();
    assert!(matches!(
        hidden.build(),
        Err(RegistryBuildError::ReadbackSurfaceMismatch { .. })
    ));

    let mut duplicate = policy(ApprovalClass::OwnerGuarded, EffectClass::ExternalWrite);
    duplicate.safety.readback = ReadbackSafety {
        mode: ReadbackMode::FollowUpOperations,
        operation_ids: vec![
            EntryId::parse("onecrew.test.readback").unwrap(),
            EntryId::parse("onecrew.test.readback").unwrap(),
        ],
    };
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.duplicate-readback",
            "onecrew.test.missing",
            "onecrew.test.readback",
            "onecrew.test.write",
        ]));
    assert_eq!(
        builder.register_operation::<StrictInput, StrictOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.duplicate-readback", &[], &[SurfaceId::Full]),
            duplicate,
            |_context, _input| async { Ok::<_, TestOwnerError>(StrictOutput { accepted: true }) },
            classify_test_error,
        ),
        Err(RegistryBuildError::InvalidSafetyPolicy)
    );
}

#[test]
fn construction_requires_explicit_secret_path_classification() {
    let mut missing_paths = policy(ApprovalClass::OwnerGuarded, EffectClass::ExternalWrite);
    missing_paths.secret = SecretClass::Transient;
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.secret-invalid",
            "onecrew.test.secret-missing",
            "onecrew.test.secret-unexpected",
            "onecrew.test.secret-valid",
        ]));
    assert_eq!(
        builder.register_operation::<StrictInput, StrictOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.secret-missing", &[], &[SurfaceId::Full]),
            missing_paths,
            |_context, _input| async { Ok::<_, TestOwnerError>(StrictOutput { accepted: true }) },
            classify_test_error,
        ),
        Err(RegistryBuildError::InvalidSecretPolicy)
    );

    let mut unexpected_paths = policy(ApprovalClass::None, EffectClass::ReadLocal);
    unexpected_paths.sensitive_input_paths = vec!["/name".to_owned()];
    assert_eq!(
        builder.register_operation::<StrictInput, StrictOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.secret-unexpected", &[], &[SurfaceId::Full]),
            unexpected_paths,
            |_context, _input| async { Ok::<_, TestOwnerError>(StrictOutput { accepted: true }) },
            classify_test_error,
        ),
        Err(RegistryBuildError::InvalidSecretPolicy)
    );

    for paths in [
        vec!["/does-not-exist".to_owned()],
        vec!["/bad~2escape".to_owned()],
        vec!["/name".to_owned(), "/name".to_owned()],
    ] {
        let mut invalid = policy(ApprovalClass::OwnerGuarded, EffectClass::ExternalWrite);
        invalid.secret = SecretClass::Transient;
        invalid.sensitive_input_paths = paths;
        assert_eq!(
            builder.register_operation::<StrictInput, StrictOutput, TestOwnerError, _, _, _>(
                metadata("onecrew.test.secret-invalid", &[], &[SurfaceId::Full]),
                invalid,
                |_context, _input| async {
                    Ok::<_, TestOwnerError>(StrictOutput { accepted: true })
                },
                classify_test_error,
            ),
            Err(RegistryBuildError::InvalidSecretPolicy)
        );
    }

    let mut valid = policy(ApprovalClass::OwnerGuarded, EffectClass::ExternalWrite);
    valid.secret = SecretClass::Transient;
    valid.sensitive_input_paths = vec!["/name".to_owned()];
    valid.view_kind = crate::operation_registry::descriptor::OperationViewKind::ReadOnly;
    builder
        .register_operation::<StrictInput, StrictOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.secret-valid", &[], &[SurfaceId::Full]),
            valid,
            |_context, _input| async { Ok::<_, TestOwnerError>(StrictOutput { accepted: true }) },
            classify_test_error,
        )
        .unwrap();
}
