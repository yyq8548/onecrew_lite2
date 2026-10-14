use super::*;

#[test]
fn search_is_bounded_stable_and_surface_authorized() {
    let mut first =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.hidden",
            "onecrew.test.operation-0",
            "onecrew.test.operation-1",
            "onecrew.test.operation-2",
            "onecrew.test.operation-3",
            "onecrew.test.operation-4",
            "onecrew.test.operation-5",
            "onecrew.test.operation-6",
            "onecrew.test.operation-7",
        ]));
    let mut second =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.hidden",
            "onecrew.test.operation-0",
            "onecrew.test.operation-1",
            "onecrew.test.operation-2",
            "onecrew.test.operation-3",
            "onecrew.test.operation-4",
            "onecrew.test.operation-5",
            "onecrew.test.operation-6",
            "onecrew.test.operation-7",
        ]));
    for index in 0..8 {
        let id = format!("onecrew.test.operation-{index}");
        add_operation(&mut first, &id, &[], &[SurfaceId::Full]).unwrap();
    }
    for index in (0..8).rev() {
        let id = format!("onecrew.test.operation-{index}");
        add_operation(&mut second, &id, &[], &[SurfaceId::Full]).unwrap();
    }
    add_operation(
        &mut first,
        "onecrew.test.hidden",
        &[],
        &[SurfaceId::Accounts],
    )
    .unwrap();
    add_operation(
        &mut second,
        "onecrew.test.hidden",
        &[],
        &[SurfaceId::Accounts],
    )
    .unwrap();
    let query = SearchQuery {
        text: SearchText::parse("test").unwrap(),
        ..SearchQuery::default()
    };
    let first_hits = first
        .build()
        .unwrap()
        .search_for_test(SurfaceId::Full, &query);
    let second_hits = second
        .build()
        .unwrap()
        .search_for_test(SurfaceId::Full, &query);
    assert_eq!(first_hits, second_hits);
    assert_eq!(first_hits.0.len(), 8);
    assert!(
        first_hits
            .0
            .iter()
            .all(|hit| hit.id.as_str() != "onecrew.test.hidden")
    );
}

#[test]
fn playbook_entries_are_returned_complete_by_search() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.read",
        ]));
    add_operation(&mut builder, "onecrew.test.read", &[], &[SurfaceId::Full]).unwrap();
    let operation_id = EntryId::parse("onecrew.test.read").unwrap();
    builder
        .register_playbook(playbook_entry(
            "playbook.test.read",
            &[SurfaceId::Full],
            operation_id.as_str(),
        ))
        .unwrap();
    let registry = builder.build().unwrap();
    let (_, playbooks) = registry.search_for_test(
        SurfaceId::Full,
        &SearchQuery {
            text: SearchText::parse("Test playbook").unwrap(),
            ..SearchQuery::default()
        },
    );
    assert_eq!(playbooks.len(), 1);
    assert_eq!(
        playbooks[0].primary_operation_ids,
        vec![operation_id.clone()]
    );
    assert_eq!(playbooks[0].recommended_steps.len(), 2);
    assert_eq!(playbooks[0].known_pitfalls.len(), 1);
}
