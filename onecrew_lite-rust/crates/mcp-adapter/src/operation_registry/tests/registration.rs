use super::*;

#[test]
fn construction_rejects_duplicate_ids_aliases_and_unknown_playbook_operations() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.first",
            "onecrew.test.missing",
            "onecrew.test.second",
        ]));
    add_operation(
        &mut builder,
        "onecrew.test.first",
        &["find first"],
        &[SurfaceId::Full],
    )
    .unwrap();
    let duplicate_id = add_operation(
        &mut builder,
        "onecrew.test.first",
        &["other"],
        &[SurfaceId::Full],
    );
    assert!(matches!(
        duplicate_id,
        Err(RegistryBuildError::DuplicateId(_))
    ));

    let duplicate_alias = add_operation(
        &mut builder,
        "onecrew.test.second",
        &["find-first"],
        &[SurfaceId::Full],
    );
    assert!(matches!(
        duplicate_alias,
        Err(RegistryBuildError::DuplicateAlias(_))
    ));

    builder
        .register_playbook(playbook_entry(
            "playbook.test.missing",
            &[SurfaceId::Full],
            "onecrew.test.missing",
        ))
        .unwrap();
    assert!(matches!(
        builder.build(),
        Err(RegistryBuildError::UnknownPlaybookOperation(_))
    ));
}

#[test]
fn construction_accepts_and_searches_playbook_without_steps_or_pitfalls() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.primary",
        ]));
    add_operation(
        &mut builder,
        "onecrew.test.primary",
        &[],
        &[SurfaceId::Full],
    )
    .unwrap();
    let mut playbook = playbook_entry(
        "playbook.test.optional-guidance",
        &[SurfaceId::Full],
        "onecrew.test.primary",
    );
    playbook.metadata.title = "Optional Guidance Playbook".to_owned();
    playbook.recommended_steps.clear();
    playbook.known_pitfalls.clear();
    builder.register_playbook(playbook).unwrap();
    let registry = builder.build().unwrap();

    let (_, playbooks) = registry.search_for_test(
        SurfaceId::Full,
        &SearchQuery {
            text: SearchText::parse("Optional Guidance Playbook").unwrap(),
            ..SearchQuery::default()
        },
    );
    assert_eq!(playbooks.len(), 1);
    assert!(playbooks[0].recommended_steps.is_empty());
    assert!(playbooks[0].known_pitfalls.is_empty());
}

#[test]
fn construction_rejects_playbook_step_and_pitfall_upper_bound_overflow() {
    let mut steps_builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.primary",
        ]));
    add_operation(
        &mut steps_builder,
        "onecrew.test.primary",
        &[],
        &[SurfaceId::Full],
    )
    .unwrap();
    let mut too_many_steps = playbook_entry(
        "playbook.test.too-many-steps",
        &[SurfaceId::Full],
        "onecrew.test.primary",
    );
    too_many_steps.recommended_steps = (0..7)
        .map(|index| PlaybookStep {
            instruction: format!("Step {index}."),
            operation_ids: Vec::new(),
        })
        .collect();
    steps_builder.register_playbook(too_many_steps).unwrap();
    assert!(matches!(
        steps_builder.build(),
        Err(RegistryBuildError::InvalidPlaybook(_))
    ));

    let mut pitfalls_builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.primary",
        ]));
    add_operation(
        &mut pitfalls_builder,
        "onecrew.test.primary",
        &[],
        &[SurfaceId::Full],
    )
    .unwrap();
    let mut too_many_pitfalls = playbook_entry(
        "playbook.test.too-many-pitfalls",
        &[SurfaceId::Full],
        "onecrew.test.primary",
    );
    too_many_pitfalls.known_pitfalls = (0..6).map(|index| format!("Pitfall {index}.")).collect();
    pitfalls_builder
        .register_playbook(too_many_pitfalls)
        .unwrap();
    assert!(matches!(
        pitfalls_builder.build(),
        Err(RegistryBuildError::InvalidPlaybook(_))
    ));
}

#[test]
fn alias_namespace_rejects_id_collisions_and_partial_registration() {
    let mut reverse =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.accepted",
            "onecrew.test.existing",
            "onecrew.test.owner",
            "onecrew.test.rejected",
            "onecrew.test.second",
        ]));
    add_operation(
        &mut reverse,
        "onecrew.test.owner",
        &["onecrew.test.second"],
        &[SurfaceId::Full],
    )
    .unwrap();
    assert!(matches!(
        add_operation(&mut reverse, "onecrew.test.second", &[], &[SurfaceId::Full]),
        Err(RegistryBuildError::DuplicateAlias(_))
    ));

    let mut atomic =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.accepted",
            "onecrew.test.existing",
            "onecrew.test.owner",
            "onecrew.test.rejected",
            "onecrew.test.second",
        ]));
    add_operation(
        &mut atomic,
        "onecrew.test.existing",
        &["collision"],
        &[SurfaceId::Full],
    )
    .unwrap();
    assert!(
        add_operation(
            &mut atomic,
            "onecrew.test.rejected",
            &["new-alias", "collision"],
            &[SurfaceId::Full],
        )
        .is_err()
    );
    add_operation(
        &mut atomic,
        "onecrew.test.accepted",
        &["new-alias"],
        &[SurfaceId::Full],
    )
    .unwrap();
}

#[test]
fn construction_rejects_empty_surfaces_and_incomplete_operations() {
    assert!(SurfaceSet::new([]).is_err());

    for (missing_input, missing_output, missing_binding, expected) in [
        (true, false, false, RegistryBuildError::MissingInputSchema),
        (false, true, false, RegistryBuildError::MissingOutputSchema),
        (false, false, true, RegistryBuildError::MissingBinding),
    ] {
        let mut builder =
            RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
                "onecrew.test.incomplete",
            ]));
        let result = builder.register_incomplete_for_test(
            metadata("onecrew.test.incomplete", &[], &[SurfaceId::Full]),
            missing_input,
            missing_output,
            missing_binding,
        );
        assert_eq!(result, Err(expected));
    }
}

#[test]
fn construction_rejects_invalid_approval_projector_combinations() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.prompt-missing",
            "onecrew.test.prompt-unexpected",
        ]));
    let missing = builder.register_operation::<StrictInput, StrictOutput, TestOwnerError, _, _, _>(
        metadata("onecrew.test.prompt-missing", &[], &[SurfaceId::Full]),
        policy(ApprovalClass::AttendedPrompt, EffectClass::DeleteLocal),
        |_context, _input| async { Ok::<_, TestOwnerError>(StrictOutput { accepted: true }) },
        classify_test_error,
    );
    assert_eq!(missing, Err(RegistryBuildError::InvalidApprovalPolicy));

    let mut unexpected_policy = policy(ApprovalClass::None, EffectClass::ReadLocal);
    unexpected_policy.approval_request = Some(ApprovalRequest::new("safe prompt").unwrap());
    let unexpected = builder.register_operation(
        metadata("onecrew.test.prompt-unexpected", &[], &[SurfaceId::Full]),
        unexpected_policy,
        |_context, _input: StrictInput| async {
            Ok::<_, TestOwnerError>(StrictOutput { accepted: true })
        },
        classify_test_error,
    );
    assert_eq!(unexpected, Err(RegistryBuildError::InvalidApprovalPolicy));
}

fn register_search_with_effect(
    builder: &mut RegistryBuilder,
    id: &str,
    effect: EffectClass,
) -> Result<(), RegistryBuildError> {
    builder.register_search_operation::<StrictInput, StrictOutput, TestOwnerError, _, _, _, _>(
        metadata(id, &[], &[SurfaceId::Full]),
        policy(ApprovalClass::OwnerGuarded, effect),
        |_context, _input| async { Ok::<_, TestOwnerError>(StrictOutput { accepted: true }) },
        classify_test_error,
        |_, _| Ok(()),
        |_, _| Ok(serde_json::Map::new()),
    )
}

#[test]
fn search_operation_registration_refuses_every_effect_that_is_not_an_open_world_read() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.search-delete-local",
            "onecrew.test.search-external-step",
            "onecrew.test.search-external-write",
            "onecrew.test.search-read-local",
            "onecrew.test.search-read-open-world",
            "onecrew.test.search-sync-open-world",
            "onecrew.test.search-write-local",
        ]));
    assert_eq!(
        register_search_with_effect(
            &mut builder,
            "onecrew.test.search-external-write",
            EffectClass::ExternalWrite,
        ),
        Err(RegistryBuildError::SearchOperationNotReadOnly(
            EntryId::parse("onecrew.test.search-external-write").unwrap()
        ))
    );
    for (id, effect) in [
        ("onecrew.test.search-read-local", EffectClass::ReadLocal),
        ("onecrew.test.search-write-local", EffectClass::WriteLocal),
        ("onecrew.test.search-delete-local", EffectClass::DeleteLocal),
        (
            "onecrew.test.search-external-step",
            EffectClass::ExternalStep,
        ),
    ] {
        assert_eq!(
            register_search_with_effect(&mut builder, id, effect),
            Err(RegistryBuildError::SearchOperationNotReadOnly(
                EntryId::parse(id).unwrap()
            )),
            "{effect:?}"
        );
    }
    // Rejected registrations do not satisfy catalog completeness. A separate
    // explicit catalog declares only the accepted read bindings below.
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.search-read-open-world",
            "onecrew.test.search-sync-open-world",
        ]));
    for (id, effect) in [
        (
            "onecrew.test.search-read-open-world",
            EffectClass::ReadOpenWorld,
        ),
        (
            "onecrew.test.search-sync-open-world",
            EffectClass::SyncOpenWorld,
        ),
    ] {
        register_search_with_effect(&mut builder, id, effect).unwrap();
    }
    builder.build().unwrap();
}

/// A search registration hands the owner the call's authorization and returns
/// the owner's page as built: nothing is recorded under the plan and nothing
/// is written into the output.
#[test]
fn search_registration_returns_the_owner_page_unchanged() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.search-page",
        ]));
    let connection = ConnectionRef::OperatorAccount {
        toolkit: OperatorToolkitId::X,
        operator_account_id: "00000000-0000-0000-0000-000000000001".into(),
    };
    let mut search_policy = policy(ApprovalClass::OwnerGuarded, EffectClass::ReadOpenWorld);
    search_policy.connection = ConnectionRequirement::OperatorAccount {
        toolkit: OperatorToolkitId::X,
    };
    builder
        .register_search_operation::<StrictInput, StrictOutput, TestOwnerError, _, _, _, _>(
            metadata("onecrew.test.search-page", &[], &[SurfaceId::Full]),
            search_policy,
            |context, _input| async move {
                Ok::<_, TestOwnerError>(StrictOutput {
                    accepted: context.search_authorization.is_some(),
                })
            },
            classify_test_error,
            |_, _| Ok(()),
            |_, _| Ok(serde_json::Map::new()),
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let platform = onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap();
    let execute = |authorized: bool| {
        let operation = registry_operation(&registry, "onecrew.test.search-page");
        let mut prepared = prepare_invocation(
            operation,
            ExecutionContext {
                surface: SurfaceId::Full,
                request_id: format!("request-{authorized}"),
                connection: Some(&connection),
                confirmed: false,
            },
            operation.input.revision(),
            json!({"name": "page", "action": {"kind": "read"}}),
        )
        .unwrap();
        if authorized {
            let scope = prepared
                .search_scope_token(
                    crate::operation_registry::execution::SearchScopePurpose::Execute,
                )
                .unwrap()
                .unwrap();
            let draft = serde_json::from_value(json!({
                "description":"Read the requested fixture page",
                "keywords":["page"],
                "scopes":[{
                    "operation_id":"onecrew.test.search-page",
                    "schema_revision":operation.input.revision(),
                    "connection":{"kind":"operator_account","toolkit":"x","operator_account_id":"00000000-0000-0000-0000-000000000001"},
                    "arguments":{"name":"page","action":{"kind":"read"}}
                }]
            })).unwrap();
            let confirmed = platform
                .confirm_search_plan_editor(draft, vec![scope.clone()])
                .unwrap();
            prepared.authorize_search(
                platform
                    .authorize_search_scope(Some(&confirmed.confirmed_plan_ref), &scope)
                    .unwrap(),
            );
        }
        run(dispatch_prepared(prepared))
    };
    for authorized in [true, false] {
        assert_eq!(
            execute(authorized).unwrap(),
            json!({"accepted": authorized}),
            "the page is the owner's, with no plan field added"
        );
    }
}

#[test]
fn catalog_declarations_are_the_independent_binding_authority() {
    use onecrew_local_runtime::{CapabilityCatalog, CatalogSurface, OperationSummary};
    let expected =
        crate::operation_registry::catalog_test_support::catalog(&["onecrew.test.expected"]);
    assert!(matches!(
        RegistryBuilder::new(expected.clone()).build(),
        Err(RegistryBuildError::MissingCatalogBinding(_))
    ));
    let mut builder = RegistryBuilder::new(expected);
    assert!(matches!(
        add_operation(
            &mut builder,
            "onecrew.test.dangling",
            &[],
            &[SurfaceId::Full]
        ),
        Err(RegistryBuildError::UnknownCatalogOperation(_))
    ));
    add_operation(
        &mut builder,
        "onecrew.test.expected",
        &[],
        &[SurfaceId::Full],
    )
    .unwrap();
    assert!(matches!(
        add_operation(
            &mut builder,
            "onecrew.test.expected",
            &[],
            &[SurfaceId::Full]
        ),
        Err(RegistryBuildError::DuplicateId(_))
    ));
    builder.build().unwrap();

    let base = crate::operation_registry::catalog_test_support::catalog(&["onecrew.test.web"]);
    let toolkits = Box::leak(
        base.toolkits()
            .cloned()
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    let operations = Box::leak(
        vec![OperationSummary {
            surfaces: &[CatalogSurface::WebNavigation],
            ..*base.operation("onecrew.test.web").unwrap()
        }]
        .into_boxed_slice(),
    );
    let catalog =
        std::sync::Arc::new(CapabilityCatalog::from_static(toolkits, operations).unwrap());
    let mut builder = RegistryBuilder::new(catalog);
    assert!(matches!(
        add_operation(&mut builder, "onecrew.test.web", &[], &[SurfaceId::Full]),
        Err(RegistryBuildError::OperationNotMcpSupported(_))
    ));
    builder.build().unwrap();
}

#[test]
fn independent_read_declaration_requires_a_plain_read_binding() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.invalid-independent",
        ]));
    let mut invalid = policy(ApprovalClass::OwnerGuarded, EffectClass::ReadOpenWorld);
    invalid.execution = crate::operation_registry::execution::ExecutionDeclaration::IndependentRead;
    assert_eq!(
        builder.register_operation::<StrictInput, StrictOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.invalid-independent", &[], &[SurfaceId::Full]),
            invalid,
            |_context, _input| async { Ok::<_, TestOwnerError>(StrictOutput { accepted: true }) },
            classify_test_error,
        ),
        Err(RegistryBuildError::InvalidExecutionDeclaration)
    );
}

#[test]
fn independent_read_declaration_rejects_owner_review_output() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.invalid-review-independent",
        ]));
    let mut invalid = policy(ApprovalClass::None, EffectClass::ReadLocal);
    invalid.execution = crate::operation_registry::execution::ExecutionDeclaration::IndependentRead;
    assert_eq!(
        builder
            .register_owner_review_operation::<StrictInput, StrictOutput, TestOwnerError, _, _, _>(
                metadata(
                    "onecrew.test.invalid-review-independent",
                    &[],
                    &[SurfaceId::Full],
                ),
                invalid,
                |_context, _input| async { Err::<_, TestOwnerError>(TestOwnerError) },
                classify_test_error,
            ),
        Err(RegistryBuildError::InvalidExecutionDeclaration)
    );
}
