use super::*;
use onecrew_local_runtime::ToolkitId;

#[test]
fn read_only_paraphrases_do_not_cross_into_effectful_operations_or_playbooks() {
    let registry = production_registry();
    for (query, toolkit, expected) in [
        (
            "Read X browser login status",
            ToolkitId::X,
            "x.connection.browser.status",
        ),
        (
            "Read Instagram browser login status",
            ToolkitId::Instagram,
            "instagram.connection.browser.status",
        ),
        (
            "Read Telegram login status",
            ToolkitId::Telegram,
            "telegram.connection.login.status",
        ),
        (
            "Inspect a YouTube credential connection",
            ToolkitId::Youtube,
            "youtube.connection.get",
        ),
        (
            "Get a saved creator",
            ToolkitId::OneCrew,
            "onecrew.creator.get",
        ),
        (
            "Read a reusable mail template",
            ToolkitId::Email,
            "email.template.get",
        ),
    ] {
        let result = search(
            &registry,
            query,
            vec![toolkit],
            vec![EffectClass::ReadLocal],
        );
        assert!(result.playbooks.is_empty(), "{query}");
        assert!(!result.operations.is_empty(), "{query}");
        assert!(result.operations.iter().all(|operation| {
            operation.toolkit == toolkit && operation.effects == vec![EffectClass::ReadLocal]
        }));
        assert!(
            result
                .operations
                .iter()
                .any(|operation| operation.id.as_str() == expected),
            "{query}: {expected} not in Recall@6"
        );
    }
}

#[test]
fn destructive_intents_do_not_cross_into_reads_or_setup_playbooks() {
    let registry = production_registry();
    for (query, toolkit, expected) in [
        (
            "Delete a creator group",
            ToolkitId::OneCrew,
            "onecrew.creator.group.delete",
        ),
        (
            "Delete a saved mail template",
            ToolkitId::Email,
            "email.template.delete",
        ),
        (
            "Disconnect an Instagram operator account",
            ToolkitId::Instagram,
            "instagram.connection.disconnect",
        ),
        (
            "Disconnect a Telegram operator account",
            ToolkitId::Telegram,
            "telegram.connection.disconnect",
        ),
        (
            "Disconnect an X operator account",
            ToolkitId::X,
            "x.connection.disconnect",
        ),
        (
            "Delete a YouTube credential",
            ToolkitId::Youtube,
            "youtube.credential.delete",
        ),
    ] {
        let result = search(
            &registry,
            query,
            vec![toolkit],
            vec![EffectClass::DeleteLocal],
        );
        assert!(result.playbooks.is_empty(), "{query}");
        assert!(!result.operations.is_empty(), "{query}");
        assert!(result.operations.iter().all(|operation| {
            operation.toolkit == toolkit && operation.effects == vec![EffectClass::DeleteLocal]
        }));
        assert!(
            result
                .operations
                .iter()
                .any(|operation| operation.id.as_str() == expected),
            "{query}: {expected} not in Recall@6"
        );
    }
}

#[test]
fn standalone_read_and_disconnect_actions_do_not_expand_into_setup_playbooks() {
    let registry = production_registry();
    for (query, toolkit, expected) in [
        (
            "Read X operator connection",
            ToolkitId::X,
            "x.connection.get",
        ),
        (
            "Disconnect X operator account",
            ToolkitId::X,
            "x.connection.disconnect",
        ),
        ("Get creator", ToolkitId::OneCrew, "onecrew.creator.get"),
    ] {
        let result = search(&registry, query, vec![toolkit], Vec::new());
        assert!(result.playbooks.is_empty(), "{query}");
        let primary = result
            .operations
            .iter()
            .filter(|operation| operation.match_role == CatalogMatchRole::Primary)
            .collect::<Vec<_>>();
        assert_eq!(primary.len(), 1, "{query}");
        assert_eq!(primary[0].id.as_str(), expected, "{query}");
    }

    let delete_wording = search(
        &registry,
        "Delete X operator connection",
        vec![ToolkitId::X],
        Vec::new(),
    );
    assert!(delete_wording.playbooks.is_empty());
}

#[test]
fn unrelated_or_underspecified_queries_return_no_catalog_guess() {
    let registry = production_registry();
    for query in [
        "Calculate orbital mechanics trajectory",
        "Forecast regional weather tomorrow",
        "Manage unrelated warehouse inventory",
    ] {
        let result = search(&registry, query, Vec::new(), Vec::new());
        assert!(result.operations.is_empty(), "{query}");
        assert!(result.playbooks.is_empty(), "{query}");
    }
}

#[test]
fn hard_effect_and_toolkit_filters_prevent_write_or_cross_toolkit_leakage() {
    let registry = production_registry();
    let result = search(
        &registry,
        "Read X posts and inspect a conversation thread",
        vec![ToolkitId::X],
        vec![EffectClass::ReadOpenWorld],
    );
    assert!(!result.operations.is_empty());
    assert!(result.operations.iter().all(|operation| {
        operation.toolkit == ToolkitId::X && operation.effects == vec![EffectClass::ReadOpenWorld]
    }));
    assert!(result.operations.iter().all(|operation| {
        !operation.id.as_str().contains("send")
            && !operation.id.as_str().contains("save")
            && !operation.id.as_str().contains("delete")
    }));
}
