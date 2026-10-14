//! Frozen plan-scope arguments compiled from each search's wire input. The
//! approval token is compiled from these arguments, so a plan's first page and
//! every executed page compile the same arguments, and an omitted page size
//! compiles exactly like its explicit default.
use super::*;
use crate::operation_registry::execution::SearchScopePurpose;

/// Compiles one search's scope arguments and approval token for `purpose`.
fn compile(
    registry: &crate::operation_registry::OperationRegistry,
    operation: &str,
    connection: &ConnectionRef,
    arguments: Value,
    purpose: SearchScopePurpose,
) -> (Value, String) {
    let entry = registry_operation(registry, operation);
    let prepared = prepare_invocation(
        entry,
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: format!("scope-{operation}"),
            connection: Some(connection),
            confirmed: false,
        },
        entry.input.revision(),
        arguments,
    )
    .unwrap_or_else(|failure| panic!("{operation}: {failure:?}"));
    let arguments = prepared
        .search_scope_arguments(purpose)
        .unwrap_or_else(|failure| panic!("{operation}: {failure:?}"))
        .expect("a plan-governed search");
    let token = prepared
        .search_scope_token(purpose)
        .unwrap_or_else(|failure| panic!("{operation}: {failure:?}"))
        .expect("a plan-governed search");
    (Value::Object(arguments), token)
}

/// The execute envelope of a confirmed page; searches with a native
/// continuation also send the provider's cursor.
fn executed(operation: &str, mut arguments: Value) -> Value {
    arguments["search_plan_policy"] = json!("require_confirmation");
    arguments["confirmed_plan_ref"] = json!("frozen-plan-reference");
    if matches!(
        operation,
        "x.posts.search.advanced" | "youtube.channels.search" | "youtube.videos.search"
    ) {
        arguments["cursor"] = json!("provider+/page-2==");
    }
    arguments
}

/// Every submitted form compiles to `compiled` for both purposes, and all of
/// them bind one approval token.
fn assert_one_scope(
    registry: &crate::operation_registry::OperationRegistry,
    operation: &str,
    inputs: &[(&str, Value)],
    compiled: &Value,
) {
    let (_, connection, _) = search_calls()
        .into_iter()
        .find(|(id, _, _)| *id == operation)
        .expect("a plan-governed search call");
    let mut tokens = BTreeSet::new();
    for (label, input) in inputs {
        for (purpose_label, purpose, arguments) in [
            (
                "initial_plan",
                SearchScopePurpose::InitialPlan,
                input.clone(),
            ),
            (
                "execute",
                SearchScopePurpose::Execute,
                executed(operation, input.clone()),
            ),
        ] {
            let (arguments, token) = compile(registry, operation, &connection, arguments, purpose);
            assert_eq!(&arguments, compiled, "{operation}/{label}/{purpose_label}");
            tokens.insert(token);
        }
    }
    assert_eq!(tokens.len(), 1, "{operation}: one approval token");
}

#[test]
fn omitted_and_explicit_default_page_sizes_compile_one_frozen_scope_for_both_purposes() {
    let (_directory, _platform, registry) = production_registry();
    let x_defaults = json!({
        "all_words": ["rust"],
        "exact_phrase": null,
        "any_words": [],
        "exclude_words": [],
        "hashtags": [],
        "from_accounts": [],
        "to_accounts": [],
        "mentioning_accounts": [],
        "language": null,
        "since": null,
        "until": null,
        "min_replies": null,
        "min_likes": null,
        "min_reposts": null,
        "replies": "any",
        "links": "any",
        "result_mode": "top",
        "count": 20
    });
    let youtube_defaults = json!({
        "query": "rust",
        "count": 20,
        "order": "relevance",
        "published_after": null,
        "region_code": null,
        "language": null
    });
    let query_defaults = json!({"query": "rust", "count": 20});
    let operations = plan_governed_operations(&registry)
        .into_iter()
        .map(|(id, _)| id.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    let cases = [
        (
            "x.posts.search.advanced",
            json!({"all_words": ["rust"]}),
            json!({
                "all_words": ["rust"],
                "replies": "any",
                "links": "any",
                "result_mode": "top",
                "count": 20
            }),
            x_defaults,
        ),
        (
            "instagram.users.search",
            json!({"query": "rust"}),
            json!({"query": "rust", "count": 20}),
            query_defaults.clone(),
        ),
        (
            "youtube.channels.search",
            json!({"query": "rust"}),
            json!({"query": "rust", "count": 20}),
            youtube_defaults.clone(),
        ),
        (
            "youtube.videos.search",
            json!({"query": "rust"}),
            json!({"query": "rust", "count": 20}),
            youtube_defaults,
        ),
        (
            "telegram.communities.search",
            json!({"query": "rust"}),
            json!({"query": "rust", "count": 20}),
            query_defaults,
        ),
    ];
    assert_eq!(
        cases
            .iter()
            .map(|(operation, ..)| (*operation).to_owned())
            .collect::<BTreeSet<_>>(),
        operations,
        "every plan-governed search has a frozen scope"
    );
    for (operation, omitted, explicit_default, compiled) in cases {
        assert_one_scope(
            &registry,
            operation,
            &[("omitted", omitted), ("explicit_default", explicit_default)],
            &compiled,
        );
    }
}

#[test]
fn populated_search_inputs_compile_to_frozen_scope_arguments_for_both_purposes() {
    let (_directory, _platform, registry) = production_registry();
    let x_populated = json!({
        "all_words": ["rust", "async"],
        "exact_phrase": "memory safety",
        "any_words": ["tokio", "axum"],
        "exclude_words": ["giveaway"],
        "hashtags": ["#rustlang"],
        "from_accounts": ["@rustlang"],
        "to_accounts": ["ferris"],
        "mentioning_accounts": ["@tokio_rs"],
        "language": "en",
        "since": "2026-08-01",
        "until": "2026-09-01",
        "min_replies": 2,
        "min_likes": 10,
        "min_reposts": 3,
        "replies": "exclude",
        "links": "include",
        "result_mode": "latest",
        "count": 50
    });
    let youtube_populated = json!({
        "query": "rust tutorials",
        "count": 50,
        "order": "date",
        "published_after": "2026-06-18T00:00:00Z",
        "region_code": "TR",
        "language": "tr"
    });
    for (operation, input) in [
        ("x.posts.search.advanced", x_populated),
        (
            "instagram.users.search",
            json!({"query": "rust creators", "count": 7}),
        ),
        ("youtube.channels.search", youtube_populated.clone()),
        ("youtube.videos.search", youtube_populated),
        (
            "telegram.communities.search",
            json!({"query": "rust community", "count": 100}),
        ),
    ] {
        assert_one_scope(
            &registry,
            operation,
            &[("populated", input.clone())],
            &input,
        );
    }
}
