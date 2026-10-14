use super::*;

/// A confirmed search plan authorizes reads only: every production operation
/// that accepts a plan scope is an open-world read.
#[test]
fn every_plan_governed_production_operation_is_an_open_world_read() {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();
    let mut plan_governed = BTreeSet::new();
    for (id, operation) in plan_governed_operations(&registry) {
        assert_eq!(operation.effect, EffectClass::ReadOpenWorld, "{id}");
        plan_governed.insert(id.as_str().to_owned());
    }
    assert_eq!(
        plan_governed,
        BTreeSet::from(
            [
                "instagram.users.search",
                "telegram.communities.search",
                "x.posts.search.advanced",
                "youtube.channels.search",
                "youtube.videos.search",
            ]
            .map(str::to_owned)
        )
    );
}

/// A confirmed plan sets criteria and parameters only: no search page carries
/// a plan field, and the dependent thread read takes no plan at all.
#[test]
fn no_search_output_carries_a_plan_field_and_the_thread_read_takes_no_plan() {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();
    let plan_governed = plan_governed_operations(&registry);
    assert!(!plan_governed.is_empty());
    for (id, operation) in plan_governed {
        let schema = operation.output.document();
        let expected_type = if matches!(
            id.as_str(),
            "instagram.users.search" | "telegram.communities.search"
        ) {
            "array"
        } else {
            "object"
        };
        assert_eq!(schema["type"], expected_type, "{id}");
        let wire = serde_json::to_string(schema).unwrap();
        for retired in [
            "plan_budget",
            "plan_novelty",
            "discovery_id",
            "candidate_id",
            "already_saved_creator_id",
        ] {
            assert!(
                !wire.contains(retired),
                "{id} output declares retired {retired}"
            );
        }
        let input = operation.input.document();
        if expected_type == "array" {
            assert!(
                input["properties"].get("cursor").is_none(),
                "{id} has no native cursor"
            );
        } else {
            assert!(
                input["properties"].get("cursor").is_some(),
                "{id} accepts its native cursor"
            );
        }
    }
    let thread = registry_operation(&registry, "x.post_thread.get");
    assert!(!thread.binding.supports_search_plan());
}
