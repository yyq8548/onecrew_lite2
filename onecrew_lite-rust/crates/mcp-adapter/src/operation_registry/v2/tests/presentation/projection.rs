use super::*;

fn projected_fields(snapshot: &onecrew_local_runtime::ResultSnapshot) -> BTreeMap<String, String> {
    snapshot
        .presentation
        .sections
        .iter()
        .filter_map(|section| match section {
            onecrew_local_runtime::PresentationSection::Fields { fields, .. } => Some(fields),
            _ => None,
        })
        .flatten()
        .map(|field| (field.label.clone(), field.value.clone()))
        .collect()
}

fn material_leaf_count(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Object(object) if object.is_empty() => 1,
        serde_json::Value::Object(object) => object.values().map(material_leaf_count).sum(),
        serde_json::Value::Array(values) if values.is_empty() => 1,
        serde_json::Value::Array(values) => values.iter().map(material_leaf_count).sum(),
        _ => 1,
    }
}

/// One nested attended request, shaped like a batch of records with their own
/// nested rows. A single item is deliberate: a list of two or more records is
/// projected as rows, and this is the per-leaf argument path.
#[derive(serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "fields define the input schema whose every leaf the confirmation card must render"
)]
struct NestedConfirmationInput {
    idempotency_key: String,
    items: Vec<NestedConfirmationItem>,
}

#[derive(serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "fields define the input schema whose every leaf the confirmation card must render"
)]
struct NestedConfirmationItem {
    display_name: String,
    notes: Option<String>,
    contact_routes: Vec<NestedConfirmationRoute>,
}

#[derive(serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "fields define the input schema whose every leaf the confirmation card must render"
)]
struct NestedConfirmationRoute {
    channel: String,
    value: String,
    source_url: Option<String>,
}

/// A confirmation card must show every value the pending call would write,
/// however deeply nested. A leaf the card drops is a value the user approved
/// without seeing.
#[tokio::test]
async fn attended_confirmation_shows_every_nested_argument_leaf() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.nested-confirmation",
        ]));
    let mut operation_policy = policy(ApprovalClass::AttendedPrompt, EffectClass::WriteLocal);
    operation_policy.approval_request =
        Some(ApprovalRequest::new("Confirm every nested value?").unwrap());
    builder
        .register_operation::<NestedConfirmationInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.nested-confirmation"),
            operation_policy,
            |_context, input: NestedConfirmationInput| async move {
                Ok::<_, TestOwnerError>(TestOutput {
                    value: input.idempotency_key,
                })
            },
            |_| unreachable!("attended owner must remain pending"),
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let call_id = "nested.preview";
    let operation_id = "onecrew.test.nested-confirmation";
    let arguments = json!({
        "idempotency_key":"nested-projection",
        "items":[{
            "display_name":"Creator One",
            "notes":"Met at the creator summit",
            "contact_routes":[{
                "channel":"email",
                "value":"creator.one@example.test",
                "source_url":"https://example.test/creator-one"
            }]
        }]
    });

    let input = ToolExecuteInput {
        calls: vec![call(
            call_id,
            operation_id,
            revision(&registry, operation_id),
            arguments.clone(),
        )],
        presentation: PresentationRequest::Never,
    };
    let result = registry
        .tool_execute_v2(&platform, &origin, SurfaceId::Full, call_id, &input)
        .await
        .unwrap();
    let snapshot = platform
        .present_result(&result.result_ref.unwrap(), &origin)
        .unwrap();
    let fields = projected_fields(&snapshot);
    assert_eq!(
        fields.len(),
        material_leaf_count(&arguments),
        "{operation_id}: {fields:?}"
    );
    assert_eq!(fields["Items[1] · Display Name"], "Creator One");
    assert_eq!(fields["Items[1] · Notes"], "Met at the creator summit");
    assert_eq!(fields["Items[1] · Contact Routes[1] · Channel"], "email");
    assert_eq!(
        fields["Items[1] · Contact Routes[1] · Value"],
        "creator.one@example.test"
    );
    assert_eq!(
        fields["Items[1] · Contact Routes[1] · Source Url"],
        "https://example.test/creator-one"
    );
}

/// The Creator batch is an owner review: preparation seals the exact items and
/// the card names every Creator it would create. Nothing in the request decides
/// whether the batch is written, so a wire `confirmed` is not a field that is
/// ignored — it is not accepted at all.
#[tokio::test]
async fn creator_batch_review_names_every_creator_and_takes_no_wire_confirmation() {
    let registry = production_registry();
    let (_directory, platform, origin) = result_owner();
    let call_id = "creator.batch.preview";
    let operation_id = "onecrew.creator.batch.create";
    let arguments = json!({
        "idempotency_key":"creator-batch-projection",
        "validation":"strict",
        "items":[
            {
                "display_name":"Creator One",
                "notes":"Met at the creator summit",
                "contact_routes":[{
                    "channel":"email",
                    "value":"creator.one@example.test",
                    "source_url":"https://example.test/creator-one"
                }]
            },
            {
                "display_name":"Creator Two",
                "notes":null,
                "contact_routes":[]
            }
        ]
    });

    let input = ToolExecuteInput {
        calls: vec![call(
            call_id,
            operation_id,
            revision(&registry, operation_id),
            arguments.clone(),
        )],
        presentation: PresentationRequest::Never,
    };
    let result = registry
        .tool_execute_v2(&platform, &origin, SurfaceId::Full, call_id, &input)
        .await
        .unwrap();
    assert!(matches!(
        &result.outcomes[0],
        ToolExecutionOutcome::InputRequired { .. }
    ));
    let snapshot = platform
        .present_result(&result.result_ref.unwrap(), &origin)
        .unwrap();
    assert_eq!(snapshot.binding.view.kind, ResultViewKind::Confirmation);
    assert!(matches!(
        snapshot.pending_action,
        Some(onecrew_local_runtime::PendingActionView::Confirm { .. })
    ));
    let rows = snapshot
        .presentation
        .sections
        .iter()
        .filter_map(|section| match section {
            onecrew_local_runtime::PresentationSection::Items { items, .. } => Some(items),
            _ => None,
        })
        .flatten()
        .map(|item| item.title.clone())
        .collect::<Vec<_>>();
    assert_eq!(rows, ["Creator One", "Creator Two"]);
    let fields = projected_fields(&snapshot);
    assert_eq!(fields["Creators"], "2");
    assert_eq!(fields["Validation"], "strict");

    // The batch is still unwritten: the card holds the only authority to write
    // it, and it has not been confirmed.
    let mut forged = arguments;
    forged["confirmed"] = json!(true);
    let forged_input = ToolExecuteInput {
        calls: vec![call(
            call_id,
            operation_id,
            revision(&registry, operation_id),
            forged,
        )],
        presentation: PresentationRequest::Never,
    };
    assert!(matches!(
        registry
            .tool_execute_v2(&platform, &origin, SurfaceId::Full, "forged", &forged_input)
            .await,
        Err(ToolExecuteOrchestrationFailure::Prevalidation { .. })
    ));
}

#[tokio::test]
async fn oversized_attended_payload_is_rejected_before_owner_dispatch() {
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&owner_calls);
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.oversized-confirmation",
        ]));
    let mut operation_policy = policy(ApprovalClass::AttendedPrompt, EffectClass::WriteLocal);
    operation_policy.approval_request = Some(ApprovalRequest::new("Confirm every value?").unwrap());
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.oversized-confirmation"),
            operation_policy,
            move |_context, input| {
                let counter = Arc::clone(&counter);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, TestOwnerError>(TestOutput { value: input.value })
                }
            },
            |_| unreachable!("oversized preview must stop before owner"),
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: vec![call(
            "call.oversized",
            "onecrew.test.oversized-confirmation",
            revision(&registry, "onecrew.test.oversized-confirmation"),
            json!({"value":"x".repeat(8 * 1024 + 1)}),
        )],
        presentation: PresentationRequest::Never,
    };
    assert_eq!(
        registry
            .tool_execute_v2(
                &platform,
                &origin,
                SurfaceId::Full,
                "oversized-confirmation",
                &input,
            )
            .await,
        Err(ToolExecuteOrchestrationFailure::PresentationUnavailable)
    );
    assert_eq!(owner_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn creator_terminal_projection_preserves_every_leaf_and_resource_reference() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.creator-search",
        ]));
    builder
        .register_operation::<TestInput, CreatorSearchOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.creator-search"),
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                crate::operation_registry::execution::ExecutionDeclaration::Sequential,
            ),
            |_context, _input| async move {
                Ok::<_, TestOwnerError>(CreatorSearchOutput {
                    discovery_id: "discovery-1".to_owned(),
                    hits: vec![CreatorHitOutput {
                        candidate_id: "candidate-1".to_owned(),
                        handle: "creator_one".to_owned(),
                        display_name: "Creator One".to_owned(),
                        profile_url: "https://example.invalid/creator_one".to_owned(),
                    }],
                    next_cursor: None,
                })
            },
            |_| unreachable!("creator test owner succeeds"),
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: vec![call(
            "call.creator-search",
            "onecrew.test.creator-search",
            revision(&registry, "onecrew.test.creator-search"),
            json!({"value":"beauty"}),
        )],
        presentation: PresentationRequest::Requested,
    };
    let result = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "creator-terminal-projection",
            &input,
        )
        .await
        .unwrap();
    assert!(result.presentation_failure.is_none());
    let output = match &result.outcomes[0] {
        ToolExecutionOutcome::Completed { result, .. } => result,
        other => panic!("unexpected outcome {other:?}"),
    };
    let snapshot = platform
        .present_result(&result.result_ref.unwrap(), &origin)
        .unwrap();
    let fields = projected_fields(&snapshot);
    assert_eq!(fields.len(), material_leaf_count(output));
    assert_eq!(fields["Discovery Id"], "discovery-1");
    assert_eq!(fields["Hits[1] · Candidate Id"], "candidate-1");
    assert_eq!(fields["Hits[1] · Handle"], "creator_one");
    assert_eq!(fields["Hits[1] · Display Name"], "Creator One");
    assert_eq!(fields["Next Cursor"], "null");
    assert!(fields.values().all(|value| !value.starts_with('{')));
    let resources = &snapshot.batch.outcomes[0].resource_refs;
    assert!(resources.iter().any(|resource| {
        resource.kind == onecrew_local_runtime::ResultResourceKind::Creator
            && resource.id == "candidate-1"
    }));
    assert!(resources.iter().any(|resource| {
        resource.kind == onecrew_local_runtime::ResultResourceKind::ProviderObject
            && resource.id == "discovery-1"
    }));
}

#[tokio::test]
async fn committed_outcome_survives_post_owner_projection_and_mint_failures() {
    for failure in [
        ToolPresentationFailure::ProjectionUnavailable,
        ToolPresentationFailure::Capacity,
    ] {
        let owner_calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&owner_calls);
        let mut builder =
            RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
                "onecrew.test.committed",
            ]));
        builder
            .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
                metadata("onecrew.test.committed"),
                OperationPolicy::read_only(
                    EffectClass::ReadLocal,
                    ConnectionRequirement::None,
                    crate::operation_registry::execution::ExecutionDeclaration::Sequential,
                ),
                move |_context, _input| {
                    let counter = Arc::clone(&counter);
                    async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        Ok::<_, TestOwnerError>(TestOutput {
                            value: if failure == ToolPresentationFailure::ProjectionUnavailable {
                                "x".repeat(8 * 1024 + 1)
                            } else {
                                "committed".to_owned()
                            },
                        })
                    }
                },
                |_| unreachable!("committed owner succeeds"),
            )
            .unwrap();
        let registry = builder.build().unwrap();
        let (_directory, platform, origin) = result_owner();
        let input = ToolExecuteInput {
            calls: vec![call(
                "call.committed",
                "onecrew.test.committed",
                revision(&registry, "onecrew.test.committed"),
                json!({"value":"execute"}),
            )],
            presentation: PresentationRequest::Requested,
        };
        let result = if failure == ToolPresentationFailure::Capacity {
            registry
                .tool_execute_v2_with_minter(
                    &platform,
                    &origin,
                    SurfaceId::Full,
                    "committed-capacity",
                    &input,
                    &|_| Err(ResultStoreError::Capacity("injected")),
                )
                .await
                .unwrap()
        } else {
            registry
                .tool_execute_v2(
                    &platform,
                    &origin,
                    SurfaceId::Full,
                    "committed-projection",
                    &input,
                )
                .await
                .unwrap()
        };
        assert_eq!(owner_calls.load(Ordering::SeqCst), 1);
        assert!(matches!(
            result.outcomes.as_slice(),
            [ToolExecutionOutcome::Completed { .. }]
        ));
        if failure == ToolPresentationFailure::Capacity {
            // The store could not hold a card, so there is no card to name.
            assert!(result.result_ref.is_none());
            assert_eq!(result.presentation_failure, Some(failure));
            continue;
        }
        // The owner ran and its output cannot be shown safely. That closes the
        // card as a presentation failure; it does not withhold the card. A
        // null reference told the reader nothing about what had just happened.
        assert_eq!(
            result.presentation_failure,
            Some(ToolPresentationFailure::ProjectionUnavailable)
        );
        let presented = crate::operation_registry::v2::state::result_present_v2(
            &platform,
            &origin,
            crate::operation_registry::v2::contract::ResultPresentInput {
                result_ref: result
                    .result_ref
                    .expect("an unprojectable result still mints its card"),
                locale: onecrew_local_runtime::PresentationLocale::SimplifiedChinese,
            },
        )
        .unwrap();
        assert_eq!(
            presented.model.status,
            crate::operation_registry::v2::contract::ResultPresentStatus::Failed
        );
        let failure = presented
            .model
            .failure
            .expect("a failed presentation exposes its safe cause");
        assert_eq!(failure.code, "presentation_unavailable");
        assert!(!failure.retryable);
        assert!(
            failure.summary.contains("请勿") || failure.summary.contains("不要"),
            "{}",
            failure.summary
        );
        let snapshot = presented.snapshot;
        assert!(matches!(
            snapshot.state,
            ResultState::Failed(ref state) if state.code == "presentation_unavailable"
        ));
        assert!(
            !serde_json::to_string(&snapshot)
                .unwrap()
                .contains(&"x".repeat(1024)),
            "the value that could not be represented stays off the card"
        );
    }
}
/// One provider page of posts, shaped the way the X advanced search returns
/// one: an effective query beside a page of records and its cursor.
#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PostSearchOutput {
    effective_query: String,
    page: PostPageOutput,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PostPageOutput {
    items: Vec<PostOutput>,
    next_cursor: Option<String>,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PostOutput {
    post_id: String,
    text: String,
    language: String,
    author: PostAuthorOutput,
    media: Vec<PostMediaOutput>,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PostAuthorOutput {
    candidate_id: String,
    display_name: String,
    handle: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PostMediaOutput {
    width: u32,
    height: u32,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WideOutput {
    values: Vec<String>,
}

fn post_page(records: usize) -> PostSearchOutput {
    PostSearchOutput {
        effective_query: "beauty min_faves:100".to_owned(),
        page: PostPageOutput {
            items: (1..=records)
                .map(|index| PostOutput {
                    post_id: format!("post-{index}"),
                    text: format!("Post {index} about a launch"),
                    language: "en".to_owned(),
                    author: PostAuthorOutput {
                        candidate_id: format!("candidate-{index}"),
                        display_name: format!("Creator {index}"),
                        handle: format!("creator_{index}"),
                    },
                    media: vec![
                        PostMediaOutput {
                            width: 947,
                            height: 1183,
                        },
                        PostMediaOutput {
                            width: 320,
                            height: 400,
                        },
                    ],
                })
                .collect(),
            next_cursor: Some("cursor-2".to_owned()),
        },
    }
}

/// A page of provider records reads row by row.
///
/// Every scalar leaf used to become its own field, so one ordinary search page
/// projected hundreds of paths like `Page · Items · [5] · Media · [1] · Width`
/// — past the capacity of any one card section, which then took the whole
/// result reference down with it. The page is a list, and a list is rows.
#[tokio::test]
async fn a_provider_page_projects_one_row_per_record_rather_than_one_field_per_leaf() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.post-search",
        ]));
    builder
        .register_operation::<TestInput, PostSearchOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.post-search"),
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                crate::operation_registry::execution::ExecutionDeclaration::Sequential,
            ),
            |_context, _input| async move { Ok::<_, TestOwnerError>(post_page(22)) },
            |_| unreachable!("post search test owner succeeds"),
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: vec![call(
            "call.post-search",
            "onecrew.test.post-search",
            revision(&registry, "onecrew.test.post-search"),
            json!({"value":"beauty"}),
        )],
        presentation: PresentationRequest::Requested,
    };
    let result = registry
        .tool_execute_v2(&platform, &origin, SurfaceId::Full, "post-search", &input)
        .await
        .unwrap();
    assert_eq!(result.presentation_failure, None);
    let output = match &result.outcomes[0] {
        ToolExecutionOutcome::Completed { result, .. } => result,
        other => panic!("unexpected outcome {other:?}"),
    };
    // Without this, the case proves nothing: a page that fits one section
    // never exercised the capacity that used to destroy the card.
    assert!(
        material_leaf_count(output) > 128,
        "the fixture must exceed one section's capacity: {}",
        material_leaf_count(output)
    );
    let snapshot = platform
        .present_result(
            &result.result_ref.expect("a page of records mints its card"),
            &origin,
        )
        .unwrap();
    use onecrew_local_runtime::PresentationSection;
    let [
        PresentationSection::Items {
            items: operations, ..
        },
        PresentationSection::Fields { fields: lead, .. },
        PresentationSection::Items { items: rows, .. },
    ] = snapshot.presentation.sections.as_slice()
    else {
        panic!(
            "a page projects its operations, its remaining leaves and its rows: {:?}",
            snapshot.presentation.sections
        )
    };
    assert_eq!(operations.len(), 1);
    // The page shortened to rows, not to a truncated wall of paths.
    assert_eq!(rows.len(), 22);
    assert_eq!(rows[4].title, "Post 5 about a launch");
    let fifth = rows[4].description.clone().expect("a row details itself");
    assert!(
        fifth.contains("Author · Display Name: Creator 5")
            && fifth.contains("Post Id: post-5")
            && fifth.contains("Media[1] · Width: 947"),
        "{fifth}"
    );
    // Whatever the list sat beside is how the reader continues it.
    let lead = lead
        .iter()
        .map(|field| (field.label.clone(), field.value.clone()))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(lead["Effective Query"], "beauty min_faves:100");
    assert_eq!(lead["Page · Next Cursor"], "cursor-2");
    // References are evidence, and evidence never depended on the display.
    let resources = &snapshot.batch.outcomes[0].resource_refs;
    assert_eq!(resources.len(), 44);
    assert!(resources.iter().any(|resource| {
        resource.kind == onecrew_local_runtime::ResultResourceKind::Creator
            && resource.id == "candidate-22"
    }));
    assert!(resources.iter().any(|resource| {
        resource.kind == onecrew_local_runtime::ResultResourceKind::ProviderObject
            && resource.id == "post-22"
    }));
}

/// Eight independent native reads may each fit their own card section while
/// their combined projection exceeds the shared result budget. The completed
/// calls must remain a readable, bounded card rather than becoming a failed
/// result with no useful presentation.
#[tokio::test]
async fn eight_fifty_native_reads_keep_completed_results_bounded_and_readable() {
    let operation_ids = [
        "onecrew.test.post-search-a",
        "onecrew.test.post-search-b",
        "onecrew.test.post-search-c",
        "onecrew.test.post-search-d",
        "onecrew.test.post-search-e",
        "onecrew.test.post-search-f",
        "onecrew.test.post-search-g",
        "onecrew.test.post-search-h",
    ];
    let mut builder = RegistryBuilder::new(
        crate::operation_registry::catalog_test_support::catalog(&operation_ids),
    );
    for operation_id in operation_ids {
        builder
            .register_operation::<TestInput, PostSearchOutput, TestOwnerError, _, _, _>(
                metadata(operation_id),
                OperationPolicy::read_only(
                    EffectClass::ReadLocal,
                    ConnectionRequirement::None,
                    crate::operation_registry::execution::ExecutionDeclaration::Sequential,
                ),
                |_context, _input| async move { Ok::<_, TestOwnerError>(post_page(50)) },
                |_| unreachable!("native read fixture succeeds"),
            )
            .unwrap();
    }
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: operation_ids
            .iter()
            .enumerate()
            .map(|(index, operation_id)| {
                call(
                    &format!("call.post-search-{index}"),
                    operation_id,
                    revision(&registry, operation_id),
                    json!({"value": format!("query-{index}")}),
                )
            })
            .collect(),
        presentation: PresentationRequest::Requested,
    };
    let result = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "eight-fifty-native-reads",
            &input,
        )
        .await
        .unwrap();

    assert!(
        result
            .outcomes
            .iter()
            .all(|outcome| { matches!(outcome, ToolExecutionOutcome::Completed { .. }) })
    );
    assert_eq!(result.presentation_failure, None);
    let result_ref = result
        .result_ref
        .expect("completed native reads retain a result reference");
    let snapshot = platform
        .present_result(&result_ref, &origin)
        .expect("completed native reads retain a readable snapshot");
    assert!(matches!(snapshot.state, ResultState::Resolved(_)));

    let output_bytes = serde_json::to_vec(&snapshot.presentation).unwrap();
    assert!(output_bytes.len() <= 64 * 1024, "{} B", output_bytes.len());
    let sections = &snapshot.presentation.sections;
    let Some(one_per_call) = sections.first() else {
        panic!("the card retains one readable summary item per call");
    };
    let onecrew_local_runtime::PresentationSection::Items { items, .. } = one_per_call else {
        panic!("the card starts with the per-call operation summaries");
    };
    assert_eq!(items.len(), operation_ids.len());
    assert!(items.iter().all(|item| {
        !item.title.is_empty()
            && item
                .description
                .as_deref()
                .is_some_and(|description| !description.is_empty())
    }));

    let row_sections = sections
        .iter()
        .skip(1)
        .filter_map(|section| match section {
            onecrew_local_runtime::PresentationSection::Items { items, .. } if items.len() < 50 => {
                Some(items)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(row_sections.len(), operation_ids.len());
    assert!(row_sections.iter().all(|rows| {
        !rows.is_empty()
            && rows.iter().all(|row| {
                !row.title.is_empty()
                    && !row.title.contains(" · ")
                    && row
                        .description
                        .as_deref()
                        .is_some_and(|description| !description.is_empty())
            })
    }));

    let notices = sections
        .iter()
        .filter_map(|section| match section {
            onecrew_local_runtime::PresentationSection::Notice { body, .. } => Some(body),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(notices.len(), operation_ids.len());
    assert!(notices.iter().all(|notice| notice.contains("50")));
}

/// An output past a section's capacity is shortened, not thrown away.
#[tokio::test]
async fn an_output_past_one_section_shows_what_fits_and_says_what_it_left_out() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.wide",
        ]));
    builder
        .register_operation::<TestInput, WideOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.wide"),
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                crate::operation_registry::execution::ExecutionDeclaration::Sequential,
            ),
            |_context, _input| async move {
                Ok::<_, TestOwnerError>(WideOutput {
                    values: (1..=200).map(|index| format!("value-{index}")).collect(),
                })
            },
            |_| unreachable!("wide test owner succeeds"),
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: vec![call(
            "call.wide",
            "onecrew.test.wide",
            revision(&registry, "onecrew.test.wide"),
            json!({"value":"wide"}),
        )],
        presentation: PresentationRequest::Requested,
    };
    let result = registry
        .tool_execute_v2(&platform, &origin, SurfaceId::Full, "wide", &input)
        .await
        .unwrap();
    assert_eq!(result.presentation_failure, None);
    let snapshot = platform
        .present_result(
            &result
                .result_ref
                .expect("a wide result still mints its card"),
            &origin,
        )
        .unwrap();
    // A read that completed reads as completed. Being too wide for one card
    // section is a fact about the card, not about the call.
    assert!(matches!(snapshot.state, ResultState::Resolved(_)));
    assert_eq!(projected_fields(&snapshot).len(), 128);
    use onecrew_local_runtime::PresentationSection;
    let notice = snapshot
        .presentation
        .sections
        .iter()
        .find_map(|section| match section {
            PresentationSection::Notice { body, .. } => Some(body.clone()),
            _ => None,
        })
        .expect("a shortened card says so");
    assert!(
        notice.contains("128") && notice.contains("200"),
        "the card names both what it shows and what there was: {notice}"
    );
}

fn confirm_projection_call() -> crate::operation_registry::v2::projection::ProjectionCall {
    use crate::operation_registry::{
        descriptor::EntryId,
        v2::projection::{AdapterPresentationKind, ProjectionCall},
    };
    ProjectionCall {
        call_id: EntryId::parse("call.confirm").unwrap(),
        operation_id: EntryId::parse("onecrew.test.confirm").unwrap(),
        title: "Send direct message".to_owned(),
        summary: "Sends one direct message".to_owned(),
        arguments: json!({"body":"Hello"}).as_object().unwrap().clone(),
        presentable: true,
        // Sending a message changes something outside this read.
        may_have_effect: true,
        kind: AdapterPresentationKind::SimpleOperation,
    }
}

#[test]
fn confirmation_chrome_follows_the_card_locale_and_carries_only_the_submitted_values() {
    use crate::operation_registry::v2::projection::{
        confirm_action_label, confirmation_projection,
    };
    use onecrew_local_runtime::{PresentationLocale, PresentationProjection, PresentationSection};

    let call = confirm_projection_call();
    let calls = [call.clone()];
    fn only_fields_heading(projection: &PresentationProjection) -> String {
        // Exactly one section: the approval prompt belongs to the pending
        // action, so the projection no longer repeats it as a second notice.
        match projection.sections.as_slice() {
            [PresentationSection::Fields { heading, fields }] => {
                assert_eq!(fields.len(), 1, "the submitted values stay on the card");
                heading.clone().expect("the fields section is headed")
            }
            _ => panic!("a confirmation projects one fields section and nothing else"),
        }
    }

    let english = confirmation_projection(&call, &calls, PresentationLocale::English).unwrap();
    let chinese =
        confirmation_projection(&call, &calls, PresentationLocale::SimplifiedChinese).unwrap();
    assert_eq!(
        only_fields_heading(&english),
        "Review every submitted value"
    );
    assert_eq!(only_fields_heading(&chinese), "逐项核对提交的内容");
    assert_eq!(confirm_action_label(PresentationLocale::English), "Confirm");
    assert_eq!(
        confirm_action_label(PresentationLocale::SimplifiedChinese),
        "确认"
    );
    // Only the chrome is translated; the reviewed values never are.
    for projection in [&english, &chinese] {
        let PresentationSection::Fields { fields, .. } = &projection.sections[0] else {
            panic!("fields")
        };
        assert_eq!(fields[0].label, "Body");
        assert_eq!(fields[0].value, "Hello");
        assert_eq!(projection.title, "Send direct message");
    }
}

#[test]
fn terminal_chrome_follows_the_card_locale_without_translating_owner_output() {
    use crate::operation_registry::v2::projection::terminal_projection;
    use onecrew_local_runtime::{PresentationLocale, PresentationSection};

    let call = confirm_projection_call();
    let calls = [call.clone()];
    let outcomes = [ToolExecutionOutcome::Completed {
        call_id: call.call_id.clone(),
        operation_id: call.operation_id.clone(),
        result: json!({"value":"delivered"}),
    }];
    let heading = |projection: &onecrew_local_runtime::PresentationProjection| match &projection
        .sections[0]
    {
        PresentationSection::Items { heading, .. } => heading.clone().expect("items are headed"),
        _ => panic!("a terminal card leads with its operations"),
    };

    let (english, _) = terminal_projection(&outcomes, &calls, PresentationLocale::English).unwrap();
    let (chinese, _) =
        terminal_projection(&outcomes, &calls, PresentationLocale::SimplifiedChinese).unwrap();
    assert_eq!(english.title, "Execution result");
    assert_eq!(chinese.title, "执行结果");
    assert_eq!(heading(&english), "Operations");
    assert_eq!(heading(&chinese), "操作");
    assert_eq!(
        english.summary.as_deref(),
        Some("Batch execution completed.")
    );
    assert_eq!(chinese.summary.as_deref(), Some("批量执行已完成。"));
    // This call sends a message, so what the owner handed back is its own
    // bookkeeping: the card reports that the operation ran and carries none of
    // it. A read's output is the answer someone asked for and is still shown.
    for projection in [&english, &chinese] {
        assert_eq!(projection.sections.len(), 1);
        assert!(
            !serde_json::to_string(projection)
                .unwrap()
                .contains("delivered")
        );
    }
}

/// A YouTube search page, shaped as the client returns one: hits, the channel
/// metadata they share, and the provider's own cursor for the next page.
#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct YoutubeSearchOutput {
    items: Vec<YoutubeHitOutput>,
    channels: BTreeMap<String, YoutubeChannelOutput>,
    next_page_token: Option<String>,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct YoutubeHitOutput {
    channel_id: String,
    channel_url: String,
    matched: YoutubeMatchOutput,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct YoutubeMatchOutput {
    title: String,
    video_id: String,
    published_at: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct YoutubeChannelOutput {
    title: String,
    subscriber_count: u64,
}

/// One key a card may not name is not a reason to lose the page around it.
///
/// `next_page_token` normalizes to `nextpagetoken`, which carries a forbidden
/// name, and refusing the whole projection over it meant every YouTube search
/// produced no card at all — the rows, the channels and the hits all went down
/// with the one key that was never going to be shown. The key stays withheld;
/// what changes is that the reader keeps everything else, and is told a field
/// was left out so nobody has to invent a reason for the gap.
#[tokio::test]
async fn a_search_page_keeps_its_rows_when_one_key_may_not_be_named() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.youtube-search",
        ]));
    builder
        .register_operation::<TestInput, YoutubeSearchOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.youtube-search"),
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                crate::operation_registry::execution::ExecutionDeclaration::Sequential,
            ),
            |_context, _input| async move {
                Ok::<_, TestOwnerError>(YoutubeSearchOutput {
                    items: (1..=3)
                        .map(|index| YoutubeHitOutput {
                            channel_id: format!("UCalpha{index}"),
                            channel_url: format!("https://www.youtube.com/channel/UCalpha{index}"),
                            matched: YoutubeMatchOutput {
                                title: format!("Upload {index}"),
                                video_id: format!("video-{index}"),
                                published_at: "2026-09-01T00:00:00Z".to_owned(),
                            },
                        })
                        .collect(),
                    channels: BTreeMap::from([(
                        "UCalpha1".to_owned(),
                        YoutubeChannelOutput {
                            title: "Alpha Channel".to_owned(),
                            subscriber_count: 1_000,
                        },
                    )]),
                    next_page_token: Some("provider+/cursor==".to_owned()),
                })
            },
            |_| unreachable!("youtube search test owner succeeds"),
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: vec![call(
            "call.youtube-search",
            "onecrew.test.youtube-search",
            revision(&registry, "onecrew.test.youtube-search"),
            json!({"value":"rust"}),
        )],
        presentation: PresentationRequest::Requested,
    };
    let result = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "youtube-search",
            &input,
        )
        .await
        .unwrap();
    // The card is minted at all. This is the whole defect: it used to close as
    // a presentation failure with nothing to look at.
    assert_eq!(result.presentation_failure, None);
    let snapshot = platform
        .present_result(
            &result.result_ref.expect("a search page mints its card"),
            &origin,
        )
        .unwrap();
    assert!(matches!(snapshot.state, ResultState::Resolved(_)));

    use onecrew_local_runtime::PresentationSection;
    let [
        PresentationSection::Items {
            items: operations, ..
        },
        PresentationSection::Fields { fields: lead, .. },
        PresentationSection::Items { items: rows, .. },
        PresentationSection::Notice { body: withheld, .. },
    ] = snapshot.presentation.sections.as_slice()
    else {
        panic!(
            "a search page projects its operations, its lead, its rows and what it left out: {:?}",
            snapshot.presentation.sections
        )
    };
    assert_eq!(operations.len(), 1);
    // The channel metadata the hits share stays readable beside the rows.
    let lead = lead
        .iter()
        .map(|field| (field.label.clone(), field.value.clone()))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(lead["Channels · UCalpha1 · Title"], "Alpha Channel");
    assert_eq!(lead["Channels · UCalpha1 · Subscriber Count"], "1000");
    // Every hit is a row, and each row details the record it names.
    assert_eq!(rows.len(), 3);
    // A hit keeps what it matched in a sub-object, so its name sits one level
    // in. Reading only the record's own keys left every row of a search page
    // called `[1]`…`[n]`, which is the one thing a list of hits cannot be.
    assert_eq!(rows[1].title, "Upload 2");
    let second = rows[1].description.clone().expect("a row details itself");
    assert!(
        second.contains("Channel Id: UCalpha2")
            && second.contains("Matched · Video Id: video-2")
            // The row is named by the match, so the detail does not repeat it.
            && !second.contains("Matched · Title"),
        "{second}"
    );
    // The card states that a field was left out and why, in its own language.
    // A count with no reason is what let a model narrate an invented cause.
    assert!(
        withheld.contains('1') && withheld.contains("names"),
        "the card must say one field was left out, and why: {withheld}"
    );
    // Withheld means withheld: the key's value reaches no part of the card.
    let card = serde_json::to_string(&snapshot.presentation).unwrap();
    assert!(!card.contains("provider+/cursor=="), "{card}");
    assert!(!card.contains("Next Page Token"), "{card}");
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CreatorSearchOutput {
    discovery_id: String,
    hits: Vec<CreatorHitOutput>,
    next_cursor: Option<String>,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CreatorHitOutput {
    candidate_id: String,
    handle: String,
    display_name: String,
    profile_url: String,
}

/// An external effect that already happened is not a failed call.
///
/// A sent email whose readback did not finish used to arrive as
/// `ToolExecutionOutcome::Failed`, so the card read "处理失败" over a message
/// that was already in the recipient's inbox — and the obvious response to a
/// failure is to send it again. The proven effect and the genuinely uncertain
/// one now both say "do not run this again", each for its own reason.
#[test]
fn an_effect_that_already_applied_is_never_reported_as_a_failed_call() {
    use crate::operation_registry::execution::{
        ClassifiedOwnerFailure, ExecuteEffectState, PublicFailureCode,
        project_classified_owner_failure,
    };
    use crate::operation_registry::v2::projection::terminal_projection;
    use onecrew_local_runtime::{InitialResultState, PresentationLocale};

    let call = confirm_projection_call();
    let calls = [call.clone()];
    let code = PublicFailureCode::parse("mail_operation_failed").expect("a static code is valid");
    let presented = |failure| {
        let failure = project_classified_owner_failure(failure);
        let outcomes = [ToolExecutionOutcome::Unknown {
            call_id: call.call_id.clone(),
            operation_id: call.operation_id.clone(),
            failure,
            review_summary: None,
        }];
        terminal_projection(&outcomes, &calls, PresentationLocale::SimplifiedChinese)
            .expect("a terminal card projects")
            .1
    };

    let applied = presented(ClassifiedOwnerFailure::applied(code.clone(), None));
    let InitialResultState::Unknown(applied) = applied else {
        panic!("an applied effect must not present as a failed call");
    };
    assert!(
        applied.summary.contains("已执行") && applied.summary.contains("请勿重复执行"),
        "an applied effect states that it happened and must not be repeated: {}",
        applied.summary
    );

    // The genuinely uncertain effect keeps its own wording: nothing was proven
    // either way, so it cannot claim the call was carried out.
    let uncertain = presented(ClassifiedOwnerFailure::unknown(code, None));
    let InitialResultState::Unknown(uncertain) = uncertain else {
        panic!("an uncertain effect stays unknown");
    };
    assert!(
        !uncertain.summary.contains("已执行"),
        "an unproven effect must not claim the call ran: {}",
        uncertain.summary
    );
    assert_ne!(applied.summary, uncertain.summary);
    assert_eq!(
        project_classified_owner_failure(ClassifiedOwnerFailure::applied(
            PublicFailureCode::parse("mail_operation_failed").unwrap(),
            None
        ))
        .effect,
        ExecuteEffectState::Applied
    );
}
