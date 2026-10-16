use super::*;

fn draft() -> SearchPlanEditorDraft {
    serde_json::from_value(serde_json::json!({
        "description":"Find technical creators", "keywords":["rust"],
        "scopes":[{"operation_id":"youtube.channels.search", "schema_revision":"exact-revision",
            "connection":{"kind":"service_credential","toolkit":"youtube","credential_id":"selected"},
            "arguments":{"query":"rust","count":5}}]
    })).unwrap()
}

#[test]
fn export_discovery_search_plan_node_oracle() {
    let Some(path) = std::env::var_os("ONECREW_DISCOVERY_SEARCH_PLAN_ORACLE") else { return; };
    let mut scopes = vec![draft().scopes.remove(0)];
    let mut varied = scopes[0].clone();
    varied.arguments = serde_json::from_value(serde_json::json!({"2":"two","10":"ten","z":{"😀":1,"\u{e000}":2},"u64":18446744073709551615u64})).unwrap();
    scopes.push(varied);
    let cases: Vec<_> = scopes.into_iter().map(|scope| serde_json::json!({"scope":scope,"token":scope.approval_token().unwrap()})).collect();
    std::fs::write(path, serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
}

#[test]
fn operation_plan_schema_has_no_provider_union_or_retired_fields() {
    let schema = serde_json::to_value(schemars::schema_for!(SearchPlanEditorDraft)).unwrap();
    let wire = serde_json::to_string(&schema).unwrap();
    for retired in [
        "tiktok_searches",
        "advanced_x",
        "SearchPlanSearch",
        "min_followers",
        "cloud_oauth",
    ] {
        assert!(!wire.contains(retired), "{retired}");
    }
    assert_eq!(schema["properties"].as_object().unwrap().len(), 3);
    assert!(
        serde_json::from_value::<SearchPlanEditorDraft>(serde_json::json!({
            "description":"criteria", "keywords":[], "searches":[], "tiktok_searches":[]
        }))
        .is_err()
    );
}

#[test]
fn strict_scope_rejects_outer_kind_and_non_object_arguments() {
    for field in ["kind", "platform", "search"] {
        let mut wire = serde_json::to_value(draft()).unwrap();
        wire["scopes"][0][field] = serde_json::json!("standard");
        assert!(serde_json::from_value::<SearchPlanEditorDraft>(wire).is_err());
    }
    let mut wire = serde_json::to_value(draft()).unwrap();
    wire["scopes"][0]["arguments"] = serde_json::json!([]);
    assert!(serde_json::from_value::<SearchPlanEditorDraft>(wire).is_err());
}

#[test]
fn acceptance_edits_preserve_exact_operation_arguments() {
    let before = draft();
    let after = before
        .apply_patch(SearchPlanPatch {
            description: Some("Review tutorials".into()),
            keywords: Some(vec!["tutorial".into()]),
        })
        .unwrap();
    assert_eq!(before.scopes, after.scopes);
    assert_ne!(before.description, after.description);
    assert!(
        before
            .apply_patch(SearchPlanPatch {
                description: None,
                keywords: Some(vec![" duplicate ".into()])
            })
            .is_err()
    );
}

#[test]
fn approval_token_binds_operation_revision_connection_and_arguments() {
    let before = draft().scopes.remove(0);
    let token = before.approval_token().unwrap();
    for field in ["operation_id", "schema_revision", "connection", "arguments"] {
        let mut wire = serde_json::to_value(&before).unwrap();
        match field {
            "connection" => wire[field]["credential_id"] = serde_json::json!("another"),
            "arguments" => wire[field]["query"] = serde_json::json!("another"),
            _ => wire[field] = serde_json::json!("another"),
        }
        let changed: SearchPlanScope = serde_json::from_value(wire).unwrap();
        assert_ne!(token, changed.approval_token().unwrap());
    }
}

#[test]
fn scope_count_and_payload_are_bounded() {
    let mut plan = draft();
    plan.scopes.clear();
    assert_eq!(
        plan.validate_typed()
            .unwrap_err()
            .location
            .parameter_pointer(),
        "/scopes"
    );
    plan.scopes = vec![draft().scopes.remove(0); SEARCH_PLAN_MAX_SCOPES + 1];
    assert!(plan.validate().is_err());
    let mut plan = draft();
    plan.scopes[0].arguments.insert(
        "query".into(),
        serde_json::json!("x".repeat(SEARCH_PLAN_MAX_BYTES)),
    );
    assert_eq!(
        plan.validate_typed().unwrap_err().reason,
        SearchPlanEditorValidationReason::PayloadTooLarge
    );
}

#[test]
fn authority_confirm_rejects_duplicate_scope_tokens() {
    let authority = SearchAuthority::default();
    let first = draft().scopes.remove(0).approval_token().unwrap();
    let mut changed = draft().scopes.remove(0);
    changed
        .arguments
        .insert("query".into(), serde_json::json!("another"));
    let second = changed.approval_token().unwrap();
    assert!(
        authority
            .confirm(vec![first.clone(), second.clone(), first.clone()])
            .is_err(),
        "an identical compiled scope token is a duplicate"
    );
    let reference = authority
        .confirm(vec![first.clone(), second])
        .expect("distinct tokens confirm");
    assert!(authority.authorize(Some(&reference), &first).is_ok());
}

#[test]
fn authority_confirm_bounds_scope_tokens_by_the_named_limit() {
    let authority = SearchAuthority::default();
    let tokens = |count: usize| -> Vec<String> {
        (0..count).map(|index| format!("sha256:{index}")).collect()
    };
    assert!(authority.confirm(tokens(SEARCH_PLAN_MAX_SCOPES)).is_ok());
    assert!(
        authority
            .confirm(tokens(SEARCH_PLAN_MAX_SCOPES + 1))
            .is_err()
    );
    assert!(authority.confirm(Vec::new()).is_err());
}
