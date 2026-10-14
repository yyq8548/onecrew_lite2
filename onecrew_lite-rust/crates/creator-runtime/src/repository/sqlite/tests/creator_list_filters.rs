use super::*;
use crate::{
    CreatorDateRange, CreatorListSort, CreatorSortDirection, CreatorSortField, CreatorStage,
    ListCreatorsInput,
};
use rusqlite::params;

fn creator(
    repository: &SqliteRepository,
    name: &str,
    stage: CreatorStage,
    updated: u64,
) -> CreatorId {
    let id = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: name.into(),
            notes: None,
        })
        .unwrap()
        .creator
        .id;
    repository.set_creator_stage(&id, stage).unwrap();
    repository
        .lock()
        .unwrap()
        .execute(
            "UPDATE creators SET updated_at = ?2 WHERE id = ?1",
            params![id.as_str(), updated],
        )
        .unwrap();
    id
}

fn touch(repository: &SqliteRepository, id: &CreatorId, at: u64) {
    repository.lock().unwrap().execute(
        "INSERT INTO touchpoints (id,creator_id,kind,occurred_at,metadata_json) VALUES (?1,?2,'direct_message',?3,'{}')",
        params![format!("{}-{at}", id.as_str()), id.as_str(), at],
    ).unwrap();
}

fn account(
    repository: &SqliteRepository,
    id: &CreatorId,
    platform: Platform,
    captured: u64,
    state: LinkState,
) {
    let value = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform,
            target_kind: TargetKind::User,
            external_id: format!("{}-{}", id.as_str(), platform.as_str()),
        })
        .unwrap();
    repository
        .link_creator_account(id, &value.id, state, Confidence::Asserted)
        .unwrap();
    repository
        .lock()
        .unwrap()
        .execute(
            "UPDATE platform_accounts SET created_at = ?2 WHERE id = ?1",
            params![value.id.as_str(), captured],
        )
        .unwrap();
}

#[test]
fn stage_and_date_filters_find_all_rows_outside_the_first_hundred() {
    let (_directory, repository) = repository();
    let mut hidden = Vec::new();
    for index in 0..140 {
        let stage = if index >= 137 {
            CreatorStage::Engaged
        } else {
            CreatorStage::Prospect
        };
        let id = creator(
            &repository,
            &format!("Creator {index:03}"),
            stage,
            1000 - index,
        );
        if index >= 137 {
            touch(&repository, &id, 500);
            hidden.push(id);
        }
    }
    let baseline = repository
        .list_creators(ListCreatorsInput {
            limit: Some(100),
            include_facets: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(baseline.items.len(), 100);
    assert!(baseline.items.iter().all(|row| !hidden.contains(&row.id)));
    let facets = baseline.facets.unwrap();
    assert_eq!(facets.total, 140);
    assert_eq!(
        facets
            .stages
            .iter()
            .find(|count| count.value == CreatorStage::Engaged)
            .unwrap()
            .count,
        3
    );

    let criteria = ListCreatorsInput {
        limit: Some(2),
        stages: vec![CreatorStage::Engaged],
        include_facets: true,
        ..Default::default()
    };
    let first = repository.list_creators(criteria.clone()).unwrap();
    assert_eq!(first.items.len(), 2);
    let facets = first.facets.unwrap();
    assert_eq!(facets.total, 3);
    assert_eq!(
        facets
            .stages
            .iter()
            .find(|count| count.value == CreatorStage::Prospect)
            .unwrap()
            .count,
        137
    );
    let second = repository
        .list_creators(ListCreatorsInput {
            cursor: first.next_cursor,
            include_facets: false,
            ..criteria
        })
        .unwrap();
    assert_eq!(second.items.len(), 1);
    assert!(second.next_cursor.is_none());
    assert!(second.facets.is_none());
    assert_eq!(
        first
            .items
            .into_iter()
            .chain(second.items)
            .map(|row| row.id)
            .collect::<Vec<_>>(),
        hidden
    );

    let range = ListCreatorsInput {
        last_touch_range: Some(CreatorDateRange {
            from: Some(500),
            to_exclusive: Some(501),
        }),
        include_facets: true,
        ..Default::default()
    };
    assert_eq!(
        repository
            .list_creators(range.clone())
            .unwrap()
            .facets
            .unwrap()
            .total,
        3
    );
    repository
        .lock()
        .unwrap()
        .execute(
            "DELETE FROM touchpoints WHERE creator_id = ?1",
            [hidden[0].as_str()],
        )
        .unwrap();
    assert_eq!(
        repository
            .list_creators(range)
            .unwrap()
            .facets
            .unwrap()
            .total,
        2,
        "derived date filter follows deleted touches"
    );
    let updated = repository
        .list_creators(ListCreatorsInput {
            stages: vec![CreatorStage::Engaged, CreatorStage::Prospect],
            query: Some("Creator".into()),
            updated_range: Some(CreatorDateRange {
                from: Some(861),
                to_exclusive: Some(863),
            }),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        updated
            .items
            .into_iter()
            .map(|row| row.id)
            .collect::<Vec<_>>(),
        hidden[1..]
    );
}

#[test]
fn primary_platform_and_facets_share_full_query_semantics() {
    let (_directory, repository) = repository();
    let a = creator(&repository, "Cohort A", CreatorStage::Engaged, 10);
    let b = creator(&repository, "Cohort B", CreatorStage::Prospect, 20);
    let c = creator(&repository, "Other C", CreatorStage::Collaborating, 30);
    let d = creator(&repository, "Cohort D", CreatorStage::Engaged, 40);
    account(&repository, &a, Platform::X, 10, LinkState::Confirmed);
    account(&repository, &a, Platform::YouTube, 20, LinkState::Confirmed);
    account(&repository, &a, Platform::Instagram, 1, LinkState::Rejected);
    account(&repository, &b, Platform::YouTube, 5, LinkState::Confirmed);
    account(
        &repository,
        &d,
        Platform::Instagram,
        20,
        LinkState::Confirmed,
    );
    for (id, counts) in [
        (&a, r#"{"x":10,"youtube":900}"#),
        (&b, r#"{"youtube":20}"#),
        (&c, r#"{"instagram":100,"x":7}"#),
        (&d, r#"{"youtube":50}"#),
    ] {
        repository
            .lock()
            .unwrap()
            .execute(
                "UPDATE creators SET follower_counts_json = ?2 WHERE id = ?1",
                params![id.as_str(), counts],
            )
            .unwrap();
    }
    let only_youtube = repository
        .list_creators(ListCreatorsInput {
            primary_platforms: vec![Platform::YouTube],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(only_youtube.items.len(), 1);
    assert_eq!(only_youtube.items[0].id, b);
    let combined = repository
        .list_creators(ListCreatorsInput {
            query: Some("Cohort".into()),
            stages: vec![CreatorStage::Engaged],
            primary_platforms: vec![Platform::YouTube],
            include_facets: true,
            ..Default::default()
        })
        .unwrap();
    assert!(combined.items.is_empty());
    let facets = combined.facets.unwrap();
    assert_eq!(facets.total, 0);
    assert_eq!(facets.stages.len(), 1);
    assert_eq!(facets.stages[0].value, CreatorStage::Prospect);
    assert_eq!(facets.stages[0].count, 1);
    assert_eq!(facets.primary_platforms.len(), 2);
    assert!(
        facets
            .primary_platforms
            .iter()
            .all(|count| count.count == 1)
    );
    let ordered = repository
        .list_creators(ListCreatorsInput {
            sorting: vec![CreatorListSort {
                field: CreatorSortField::Followers,
                direction: CreatorSortDirection::Asc,
            }],
            ..Default::default()
        })
        .unwrap();
    let wire = serde_json::to_value(&ordered.items).unwrap();
    let unlinked = wire
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == c.as_str())
        .unwrap();
    assert_eq!(
        unlinked["follower_counts"]
            .as_object()
            .unwrap()
            .iter()
            .next()
            .unwrap()
            .1,
        &serde_json::json!(100)
    );
    assert_eq!(
        ordered
            .items
            .into_iter()
            .map(|row| row.id)
            .collect::<Vec<_>>(),
        vec![a, b, d, c]
    );
}

#[test]
fn all_sort_fields_page_globally_with_ties_nulls_and_pins() {
    let (_directory, repository) = repository();
    let names = ["Zulu", "alpha", "Bravo", "charlie", "echo", "delta"];
    let scores = [None, Some(40), Some(20), Some(40), None, Some(90)];
    let followers = [None, Some(200), Some(50), Some(200), Some(0), Some(900)];
    let touches = [None, Some(70), Some(40), Some(70), None, Some(10)];
    let mut ids = Vec::new();
    for index in 0..6 {
        let id = creator(
            &repository,
            names[index],
            CreatorStage::Prospect,
            100 + index as u64,
        );
        repository
            .lock()
            .unwrap()
            .execute(
                "UPDATE creators SET created_at=?2,score=?3,follower_counts_json=?4 WHERE id=?1",
                params![
                    id.as_str(),
                    20 + index as u64,
                    scores[index],
                    followers[index]
                        .map(|value| format!("{{\"x\":{value}}}"))
                        .unwrap_or_else(|| "{}".into())
                ],
            )
            .unwrap();
        if let Some(at) = touches[index] {
            touch(&repository, &id, at);
        }
        ids.push(id);
    }
    repository.set_creator_pinned(&ids[0], true).unwrap();
    for field in [
        CreatorSortField::DisplayName,
        CreatorSortField::Followers,
        CreatorSortField::Score,
        CreatorSortField::LastTouchAt,
        CreatorSortField::UpdatedAt,
        CreatorSortField::CreatedAt,
    ] {
        for direction in [CreatorSortDirection::Asc, CreatorSortDirection::Desc] {
            let mut criteria = ListCreatorsInput {
                limit: Some(2),
                sorting: vec![CreatorListSort { field, direction }],
                ..Default::default()
            };
            let mut rows = Vec::new();
            loop {
                let page = repository.list_creators(criteria.clone()).unwrap();
                rows.extend(page.items);
                criteria.cursor = page.next_cursor;
                if criteria.cursor.is_none() {
                    break;
                }
                assert!(rows.len() < 10, "cursor must advance");
            }
            assert_eq!(rows.len(), 6);
            assert_eq!(rows[0].id, ids[0]);
            assert_eq!(
                rows.iter()
                    .map(|row| row.id.as_str())
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                6
            );
            for pair in rows[1..].windows(2) {
                let comparison = if field == CreatorSortField::DisplayName {
                    pair[0]
                        .display_name
                        .to_ascii_lowercase()
                        .cmp(&pair[1].display_name.to_ascii_lowercase())
                } else {
                    let value = |row: &crate::CreatorSummary| match field {
                        CreatorSortField::Followers => row
                            .follower_counts
                            .get(&Platform::X)
                            .map(|v| *v as i64)
                            .unwrap_or(-1),
                        CreatorSortField::Score => row.score.map(i64::from).unwrap_or(-1),
                        CreatorSortField::LastTouchAt => {
                            row.last_touch_at.map(|v| v as i64).unwrap_or(-1)
                        }
                        CreatorSortField::UpdatedAt => row.updated_at as i64,
                        CreatorSortField::CreatedAt => row.created_at as i64,
                        CreatorSortField::DisplayName => unreachable!(),
                    };
                    value(&pair[0]).cmp(&value(&pair[1]))
                };
                if comparison.is_eq() {
                    assert!(pair[0].id.as_str() > pair[1].id.as_str());
                } else if direction == CreatorSortDirection::Asc {
                    assert!(comparison.is_lt(), "{field:?}");
                } else {
                    assert!(comparison.is_gt(), "{field:?}");
                }
            }
        }
    }
}

#[test]
fn mixed_numeric_descending_and_text_ascending_order_crosses_page_boundaries() {
    let (_directory, repository) = repository();
    for (name, score) in [
        ("z", 20),
        ("a", 20),
        ("b", 30),
        ("c", 30),
        ("a", 20),
        ("b", 30),
    ] {
        let id = creator(&repository, name, CreatorStage::Prospect, 100);
        repository
            .lock()
            .unwrap()
            .execute(
                "UPDATE creators SET score=?2 WHERE id=?1",
                params![id.as_str(), score],
            )
            .unwrap();
    }
    let mut input = ListCreatorsInput {
        limit: Some(1),
        sorting: vec![
            CreatorListSort {
                field: CreatorSortField::Score,
                direction: CreatorSortDirection::Desc,
            },
            CreatorListSort {
                field: CreatorSortField::DisplayName,
                direction: CreatorSortDirection::Asc,
            },
        ],
        ..Default::default()
    };
    let mut rows = Vec::new();
    loop {
        let page = repository.list_creators(input.clone()).unwrap();
        rows.extend(page.items);
        input.cursor = page.next_cursor;
        if input.cursor.is_none() {
            break;
        }
        assert!(rows.len() <= 6);
    }
    assert_eq!(
        rows.iter()
            .map(|row| row.display_name.as_str())
            .collect::<Vec<_>>(),
        ["b", "b", "c", "a", "a", "z"]
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.score.unwrap())
            .collect::<Vec<_>>(),
        [30, 30, 30, 20, 20, 20]
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.id.as_str())
            .collect::<std::collections::HashSet<_>>()
            .len(),
        6
    );
    assert!(rows[0].id.as_str() > rows[1].id.as_str());
    assert!(rows[3].id.as_str() > rows[4].id.as_str());
}

#[test]
fn cursor_cannot_switch_filters_or_sort_and_invalid_ranges_fail() {
    let (_directory, repository) = repository();
    for index in 0..3 {
        creator(
            &repository,
            &format!("Creator {index}"),
            CreatorStage::Prospect,
            index,
        );
    }
    let first = repository
        .list_creators(ListCreatorsInput {
            limit: Some(1),
            ..Default::default()
        })
        .unwrap();
    for input in [
        ListCreatorsInput {
            stages: vec![CreatorStage::Engaged],
            ..Default::default()
        },
        ListCreatorsInput {
            primary_platforms: vec![Platform::X],
            ..Default::default()
        },
        ListCreatorsInput {
            sorting: vec![CreatorListSort {
                field: CreatorSortField::DisplayName,
                direction: CreatorSortDirection::Asc,
            }],
            ..Default::default()
        },
        ListCreatorsInput {
            updated_range: Some(CreatorDateRange {
                from: Some(0),
                to_exclusive: None,
            }),
            ..Default::default()
        },
    ] {
        let error = repository
            .list_creators(ListCreatorsInput {
                cursor: first.next_cursor.clone(),
                ..input
            })
            .unwrap_err();
        assert!(error.contains("page cursor"));
    }
    assert!(
        repository
            .list_creators(ListCreatorsInput {
                updated_range: Some(CreatorDateRange {
                    from: Some(10),
                    to_exclusive: Some(10)
                }),
                ..Default::default()
            })
            .is_err()
    );
    assert!(
        repository
            .list_creators(ListCreatorsInput {
                last_touch_range: Some(CreatorDateRange {
                    from: Some(u64::MAX),
                    to_exclusive: None
                }),
                ..Default::default()
            })
            .is_err()
    );
}
