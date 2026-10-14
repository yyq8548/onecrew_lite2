use super::*;
use crate::SearchPlanPatch;

fn plan_input() -> ExecutedResultInput {
    let draft: crate::SearchPlanEditorDraft = serde_json::from_value(serde_json::json!({
        "description":"Find creators who publish technical tutorials",
        "keywords":["rust"],
        "scopes":[{"operation_id":"youtube.channels.search","schema_revision":"test-revision",
            "connection":{"kind":"service_credential","toolkit":"youtube","credential_id":"test-credential"},
            "arguments":{"query":"rust","count":5}}]
    })).unwrap();
    let payload = OwnerActionPayload::new(serde_json::to_vec(&draft).unwrap()).unwrap();
    ExecutedResultInput {
        pending_action: Some(PendingUserAction::SearchPlan {
            draft: draft.clone(),
            payload,
        }),
        presentation: search_plan_projection(&draft).unwrap(),
        ..confirmation_input(b"unused")
    }
}

fn patch(keyword: &str) -> SearchPlanPatch {
    SearchPlanPatch {
        description: None,
        keywords: Some(vec![keyword.to_owned()]),
    }
}

#[test]
fn projection_fields_carry_only_the_acceptance_criteria() {
    // Terminal cards render these fields once the pending draft is gone, so
    // they hold business data only, never an editor instruction or a count.
    assert_eq!(
        plan_input().presentation.sections,
        [PresentationSection::Fields {
            heading: Some("Search plan".to_owned()),
            fields: vec![PresentationField {
                label: "Acceptance criteria".to_owned(),
                value: "Find creators who publish technical tutorials".to_owned(),
            }],
        }]
    );
}

#[test]
fn revision_updates_current_payload_without_changing_mint_identity() {
    let store = ResultMemoryStore::new();
    let before = present(&store, store.mint_from_execute(plan_input()).unwrap());
    let after = store
        .revise_search_plan(&before.binding, patch("tauri"))
        .unwrap();
    assert_eq!(after.binding.fingerprint, before.binding.fingerprint);
    assert_eq!(after.binding.revision, before.binding.revision + 1);
    assert_ne!(after.pending_action, before.pending_action);
    assert_eq!(store.current(&before.binding).unwrap(), after);
    assert!(matches!(
        store.submit(&before.binding),
        Err(ResultStoreError::RevisionConflict { .. })
    ));
    let submitted = store.submit(&after.binding).unwrap();
    let draft: crate::SearchPlanEditorDraft =
        serde_json::from_slice(&submitted.owner_payload.into_generic_bytes().unwrap()).unwrap();
    assert_eq!(draft.keywords, ["tauri"]);
    assert!(store.submit(&submitted.binding).is_err());
    assert!(
        store
            .revise_search_plan(&submitted.binding, patch("another"))
            .is_err()
    );
}

#[test]
fn invalid_patch_and_wrong_binding_preserve_state() {
    let store = ResultMemoryStore::new();
    let before = present(&store, store.mint_from_execute(plan_input()).unwrap());
    let bytes = store.cleanup().unwrap().payload_bytes;
    assert!(
        store
            .revise_search_plan(&before.binding, patch(""))
            .is_err()
    );
    let mut wrong = before.binding.clone();
    wrong.origin = origin("another");
    assert_eq!(
        store
            .revise_search_plan(&wrong, patch("tauri"))
            .unwrap_err(),
        ResultStoreError::OriginMismatch
    );
    assert_eq!(store.current(&before.binding).unwrap(), before);
    assert_eq!(store.cleanup().unwrap().payload_bytes, bytes);
}

#[test]
fn revise_and_confirm_compete_for_one_revision() {
    let store = ResultMemoryStore::new();
    let before = present(&store, store.mint_from_execute(plan_input()).unwrap());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let other_store = store.clone();
    let binding = before.binding.clone();
    let other_barrier = barrier.clone();
    let revise = std::thread::spawn(move || {
        other_barrier.wait();
        other_store.revise_search_plan(&binding, patch("tauri"))
    });
    barrier.wait();
    let confirm = store.submit(&before.binding);
    assert_ne!(confirm.is_ok(), revise.join().unwrap().is_ok());
}

#[test]
fn edit_patch_rejects_opaque_arguments_and_scope_changes() {
    for wire in [
        serde_json::json!({"operation_id":"anything"}),
        serde_json::json!({"searches":[]}),
        serde_json::json!({"arguments":{}}),
    ] {
        assert!(serde_json::from_value::<SearchPlanPatch>(wire).is_err());
    }
}

fn sized_plan(keyword_bytes: usize) -> crate::SearchPlanEditorDraft {
    let Some(PendingUserAction::SearchPlan { draft, .. }) = plan_input().pending_action else {
        unreachable!()
    };
    draft
        .apply_patch(SearchPlanPatch {
            description: None,
            keywords: Some(
                (0..50)
                    .map(|index| format!("{index:02}{}", "k".repeat(keyword_bytes - 2)))
                    .collect(),
            ),
        })
        .unwrap()
}

fn sized_input(draft: crate::SearchPlanEditorDraft) -> ExecutedResultInput {
    ExecutedResultInput {
        pending_action: Some(PendingUserAction::SearchPlan {
            payload: OwnerActionPayload::from_search_plan(&draft).unwrap(),
            draft: draft.clone(),
        }),
        presentation: search_plan_projection(&draft).unwrap(),
        ..confirmation_input(b"unused")
    }
}

#[test]
fn large_keyword_lists_mint_and_revise_without_losing_keywords_or_scope() {
    for bytes in [160, 512] {
        let draft = sized_plan(bytes);
        assert_eq!(draft.scopes.len(), 1);
        assert!(serde_json::to_vec(&draft).unwrap().len() > bytes * 50);
        let store = ResultMemoryStore::new();
        let before = present(&store, store.mint_from_execute(sized_input(draft)).unwrap());
        let after = store
            .revise_search_plan(
                &before.binding,
                SearchPlanPatch {
                    description: Some("Updated acceptance criteria".into()),
                    keywords: None,
                },
            )
            .unwrap();
        let Some(PendingActionView::SearchPlan { draft, .. }) = after.pending_action else {
            panic!("plan")
        };
        assert_eq!(draft.keywords.len(), 50);
        assert!(draft.keywords.iter().all(|keyword| keyword.len() == bytes));
        assert_eq!(draft.scopes.len(), 1);
        assert_eq!(after.binding.fingerprint, before.binding.fingerprint);
    }
    let store = ResultMemoryStore::new();
    let before = present(&store, store.mint_from_execute(plan_input()).unwrap());
    let keywords = sized_plan(512).keywords;
    let after = store
        .revise_search_plan(
            &before.binding,
            SearchPlanPatch {
                description: None,
                keywords: Some(keywords.clone()),
            },
        )
        .unwrap();
    assert!(
        matches!(after.pending_action, Some(PendingActionView::SearchPlan { draft, .. }) if draft.keywords == keywords && draft.scopes.len() == 1)
    );
}

#[test]
fn larger_typed_payload_cannot_bypass_generic_confirmation_or_choice_budget() {
    assert!(OwnerActionPayload::new(vec![0; MAX_OWNER_ACTION_BYTES]).is_ok());
    assert!(OwnerActionPayload::new(vec![0; MAX_OWNER_ACTION_BYTES + 1]).is_err());
    let draft = sized_plan(512);
    let mut input = confirmation_input(b"small");
    input.pending_action = Some(PendingUserAction::Confirm {
        prompt: "Review".into(),
        label: "Confirm".into(),
        payload: OwnerActionPayload::from_search_plan(&draft).unwrap(),
    });
    assert!(ResultMemoryStore::new().mint_from_execute(input).is_err());
    let mut input = choice_input();
    let Some(PendingUserAction::Choice { payload, .. }) = &mut input.pending_action else {
        unreachable!()
    };
    *payload = OwnerActionPayload::from_search_plan(&draft).unwrap();
    assert!(ResultMemoryStore::new().mint_from_execute(input).is_err());
}

#[test]
fn oversized_typed_plan_is_rejected_and_failed_revision_keeps_available_snapshot() {
    let Some(PendingUserAction::SearchPlan { mut draft, .. }) = plan_input().pending_action else {
        unreachable!()
    };
    draft.scopes[0].arguments.insert(
        "query".into(),
        serde_json::json!("x".repeat(crate::SEARCH_PLAN_MAX_BYTES - 4096)),
    );
    let store = ResultMemoryStore::new();
    let before = present(
        &store,
        store.mint_from_execute(sized_input(draft.clone())).unwrap(),
    );
    let used = store.cleanup().unwrap().payload_bytes;
    let result = store.revise_search_plan(
        &before.binding,
        SearchPlanPatch {
            description: Some("d".repeat(8192)),
            keywords: None,
        },
    );
    assert_eq!(
        result.unwrap_err(),
        ResultStoreError::InvalidInput("search_plan_payload_too_large")
    );
    assert_eq!(store.current(&before.binding).unwrap(), before);
    assert_eq!(store.cleanup().unwrap().payload_bytes, used);
    draft.description = "d".repeat(8192);
    assert_eq!(
        OwnerActionPayload::from_search_plan(&draft).unwrap_err(),
        ResultStoreError::InvalidInput("search_plan_payload_too_large")
    );
}
