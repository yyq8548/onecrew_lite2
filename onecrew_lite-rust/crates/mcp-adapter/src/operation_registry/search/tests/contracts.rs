use super::*;
use onecrew_local_runtime::ToolkitId;

#[test]
fn exact_id_bypasses_lexical_ranking_but_not_hard_filters() {
    let registry = production_registry();
    let exact = search(&registry, "x.profile.get", vec![ToolkitId::X], Vec::new());
    assert_eq!(exact.operations.len(), 1);
    assert_eq!(exact.operations[0].id.as_str(), "x.profile.get");
    assert!(exact.playbooks.is_empty());

    let filtered = search(
        &registry,
        "x.profile.get",
        vec![ToolkitId::Email],
        Vec::new(),
    );
    assert!(filtered.operations.is_empty());
    assert!(filtered.playbooks.is_empty());

    for unknown_id in [
        "platform.x.profile",
        "x.profile.ge",
        "X.profile.get",
        "onecrew.legacy_creator_get",
        "x_user_search",
        "platform_read",
    ] {
        let unknown = search(&registry, unknown_id, vec![ToolkitId::X], Vec::new());
        assert!(unknown.operations.is_empty(), "{unknown_id}");
        assert!(unknown.playbooks.is_empty(), "{unknown_id}");
    }
    let natural_hyphen_phrase = search(
        &registry,
        "creator-discovery",
        vec![ToolkitId::X],
        Vec::new(),
    );
    let natural_spaced_phrase = search(
        &registry,
        "creator discovery",
        vec![ToolkitId::X],
        Vec::new(),
    );
    assert_eq!(natural_hyphen_phrase, natural_spaced_phrase);
    let sentence_with_period = search(
        &registry,
        "Find relevant X accounts.",
        vec![ToolkitId::X],
        Vec::new(),
    );
    assert!(!sentence_with_period.operations.is_empty());

    let cross_toolkit = search(
        &registry,
        "playbook.recipe.find-x-creators",
        vec![ToolkitId::X],
        Vec::new(),
    );
    assert!(cross_toolkit.operations.is_empty());
    assert!(cross_toolkit.playbooks.is_empty());
    let cross_toolkit = search(
        &registry,
        "playbook.recipe.find-x-creators",
        vec![ToolkitId::X, ToolkitId::OneCrew],
        Vec::new(),
    );
    assert_eq!(
        cross_toolkit.playbooks[0].id.as_str(),
        "playbook.recipe.find-x-creators"
    );
}

#[test]
fn standalone_operation_search_does_not_fabricate_a_playbook() {
    let registry = production_registry();
    let result = search(
        &registry,
        "List mail templates",
        vec![ToolkitId::Email],
        vec![EffectClass::ReadLocal],
    );
    assert_eq!(result.operations[0].id.as_str(), "email.template.list");
    assert!(result.playbooks.is_empty());
    assert_eq!(
        result
            .operations
            .iter()
            .filter(|operation| operation.match_role == CatalogMatchRole::Primary)
            .count(),
        1
    );
}

#[test]
fn matched_playbook_returns_linked_primary_and_related_operations() {
    let registry = production_registry();
    let result = search(
        &registry,
        "Draft and send an email, then verify its thread",
        vec![ToolkitId::Email],
        Vec::new(),
    );
    assert_eq!(
        result.playbooks[0].id.as_str(),
        "playbook.email.draft-and-send"
    );
    let roles = result
        .operations
        .iter()
        .map(|operation| (operation.id.as_str(), operation.match_role))
        .collect::<Vec<_>>();
    assert!(roles.contains(&("email.draft.save", CatalogMatchRole::Primary)));
    assert!(roles.contains(&("email.draft.send", CatalogMatchRole::Primary)));
    assert!(roles.contains(&("email.draft.get", CatalogMatchRole::Related)));
    assert!(roles.contains(&("email.thread.list", CatalogMatchRole::Related)));
    let primary_ids = result.playbooks[0]
        .primary_operation_ids
        .iter()
        .map(|id| id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(result.operations.iter().all(|operation| {
        (operation.match_role == CatalogMatchRole::Primary)
            == primary_ids.contains(operation.id.as_str())
    }));
    assert!(
        result.operations[..primary_ids.len()]
            .iter()
            .all(|operation| operation.match_role == CatalogMatchRole::Primary)
    );
}

#[test]
fn contract_rejects_non_english_and_empty_queries_without_guessing_intent_boundaries() {
    for invalid in [
        json!({"query": "查找 X 创作者"}),
        json!({"query": "the and their"}),
    ] {
        assert!(serde_json::from_value::<ToolSearchInput>(invalid).is_err());
    }
    for valid in [
        json!({"query": "Find X creators; separately send an email"}),
        json!({"query": "Find X creators\nSend an email"}),
        json!({"query": "Find X creators and also inspect profiles"}),
    ] {
        assert!(serde_json::from_value::<ToolSearchInput>(valid).is_ok());
    }
}

#[test]
fn retrieval_is_deterministic_and_response_budgets_remain_bounded() {
    let registry = production_registry();
    let query = "Find Telegram communities relevant to a topic";
    let first = search(&registry, query, vec![ToolkitId::Telegram], Vec::new());
    for _ in 0..4 {
        assert_eq!(
            search(&registry, query, vec![ToolkitId::Telegram], Vec::new()),
            first
        );
    }
    assert!(first.operations.len() <= 8);
    assert!(first.playbooks.len() <= 3);
    assert!(serde_json::to_vec(&first).unwrap().len() < 64 * 1024);
}
