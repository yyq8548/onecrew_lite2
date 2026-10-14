use super::*;

#[test]
fn candidate_save_and_manual_batch_apply_tags_without_clearing_omitted_values() {
    let (_directory, repository) = repository();
    let origin = platform_search_origin(&repository);
    let candidate = entry(origin, "tags-account", "tagged");
    let mut input = save_input(candidate.clone(), None);
    input.tags = Some(vec![" 美妆 ".into()]);
    let first = persist_entry(&repository, input).unwrap();
    assert_eq!(first.creator.creator.tags, ["美妆"]);
    let repeat = persist_entry(&repository, save_input(candidate.clone(), None)).unwrap();
    assert!(!repeat.created);
    assert_eq!(repeat.creator.creator.tags, ["美妆"]);
    let mut clear = save_input(candidate, None);
    clear.tags = Some(vec![]);
    assert!(
        persist_entry(&repository, clear)
            .unwrap()
            .creator
            .creator
            .tags
            .is_empty()
    );
    let results = repository
        .create_manual_creator_batch(vec![crate::ManualCreatorCreateInput {
            display_name: "Manual tags".into(),
            tags: Some(vec!["美妆".into(), " Beauty ".into()]),
            notes: None,
            contact_routes: Vec::new(),
        }])
        .unwrap();
    assert_eq!(results[0].tags, ["美妆", "Beauty"]);
    assert_eq!(
        repository
            .get_creator(&results[0].creator_id)
            .unwrap()
            .creator
            .tags,
        results[0].tags
    );
}

fn patch(creator_id: CreatorId, tags: Option<Vec<String>>) -> crate::UpdateCreatorInput {
    crate::UpdateCreatorInput {
        creator_id,
        tags,
        display_name: None,
        notes: None,
        score: None,
        assessment: None,
        follower_counts: None,
    }
}

#[test]
fn creator_tags_roundtrip_replace_clear_and_reject_invalid_atomically() {
    let (_directory, repository) = repository();
    let record = repository
        .create_creator(CreateCreatorInput {
            display_name: "Tagged Creator".into(),
            notes: None,
            tags: Some(vec![" 美妆 ".into(), "𠮷".repeat(12), "Beauty".into()]),
        })
        .unwrap();
    let id = record.creator.id;
    let original = vec!["美妆".to_string(), "𠮷".repeat(12), "Beauty".into()];
    assert_eq!(record.creator.tags, original);
    assert_eq!(
        repository
            .update_creator(patch(id.clone(), None))
            .unwrap()
            .creator
            .tags,
        original
    );
    for invalid in [
        vec!["".into()],
        vec!["x".repeat(13)],
        vec!["a".into(), " a ".into()],
        vec!["line\nbreak".into()],
        vec!["a".into(), "b".into(), "c".into(), "d".into()],
    ] {
        let mut input = patch(id.clone(), Some(invalid));
        input.display_name = Some("Should roll back".into());
        assert!(repository.update_creator(input).is_err());
        let current = repository.get_creator(&id).unwrap();
        assert_eq!(current.creator.tags, original);
        assert_eq!(current.creator.display_name, "Tagged Creator");
    }
    let page = repository
        .list_creators(crate::ListCreatorsInput::default())
        .unwrap();
    assert_eq!(page.items[0].tags, original);
    assert!(
        repository
            .update_creator(patch(id.clone(), Some(vec![])))
            .unwrap()
            .creator
            .tags
            .is_empty()
    );
    let replaced = repository
        .update_creator(patch(id, Some(vec!["beauty".into(), "Beauty".into()])))
        .unwrap();
    assert_eq!(replaced.creator.tags, ["beauty", "Beauty"]);
}

#[test]
fn tags_participate_in_portable_hash_and_workbook_roundtrip() {
    let (_directory, repository) = repository();
    let record = repository
        .create_creator(CreateCreatorInput {
            display_name: "Portable".into(),
            notes: None,
            tags: None,
        })
        .unwrap();
    let before = repository.export_creator_bundle().unwrap();
    repository
        .update_creator(patch(
            record.creator.id,
            Some(vec!["美妆".into(), "Beauty".into()]),
        ))
        .unwrap();
    let after = repository.export_creator_bundle().unwrap();
    assert_ne!(before.content_sha256, after.content_sha256);
    assert_ne!(
        before.creators[0].semantic_sha256,
        after.creators[0].semantic_sha256
    );
    assert_ne!(before.creators[0].record_key, after.creators[0].record_key);
    assert_eq!(
        crate::CreatorBundle::from_json_bytes(&after.to_json_bytes().unwrap()).unwrap(),
        after
    );
    assert_eq!(
        crate::CreatorBundle::from_xlsx_bytes(&after.to_xlsx_bytes().unwrap()).unwrap(),
        after
    );
}

#[test]
fn tags_input_schema_is_bounded_and_current_creator_output_requires_tags() {
    let schema = serde_json::to_value(schemars::schema_for!(crate::UpdateCreatorInput)).unwrap();
    assert_eq!(schema["properties"]["tags"]["maxItems"], 3);
    assert_eq!(schema["properties"]["tags"]["items"]["maxLength"], 12);
    assert_eq!(schema["properties"]["tags"]["items"]["minLength"], 1);
    let (_directory, repository) = repository();
    let record = repository
        .create_creator(CreateCreatorInput {
            display_name: "Required tags".into(),
            notes: None,
            tags: None,
        })
        .unwrap();
    let mut json = serde_json::to_value(record.creator).unwrap();
    json.as_object_mut().unwrap().remove("tags");
    assert!(serde_json::from_value::<crate::Creator>(json).is_err());
}
