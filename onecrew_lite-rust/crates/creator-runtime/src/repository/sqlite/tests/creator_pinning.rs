use super::*;
use crate::{ListCreatorsInput, encode_page_cursor};

/// Creators are created in one test tick, so `updated_at` would tie across all
/// of them. Each fixture takes an explicit recency instead, which is what the
/// sort key's second segment reads.
fn creator_at(repository: &SqliteRepository, name: &str, updated_at: u64) -> CreatorId {
    let id = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: name.into(),
            notes: None,
        })
        .expect("creator")
        .creator
        .id;
    repository
        .lock()
        .expect("lock")
        .execute(
            "UPDATE creators SET updated_at = ?2 WHERE id = ?1",
            rusqlite::params![id.as_str(), updated_at],
        )
        .expect("recency");
    id
}

/// `set_creator_pinned` stamps `now()`, whose resolution is one second, so two
/// pins inside a test tie on the first segment. Pin ranks are set explicitly
/// where the test needs pinned rows to have a defined order among themselves.
fn pin_at(repository: &SqliteRepository, id: &CreatorId, pinned_at: u64) {
    repository
        .set_creator_pinned(id, true)
        .expect("pin creator");
    repository
        .lock()
        .expect("lock")
        .execute(
            "UPDATE creators SET pinned_at = ?2 WHERE id = ?1",
            rusqlite::params![id.as_str(), pinned_at],
        )
        .expect("pin rank");
}

fn listed(repository: &SqliteRepository, input: ListCreatorsInput) -> Vec<CreatorId> {
    repository
        .list_creators(input)
        .expect("creators")
        .items
        .into_iter()
        .map(|summary| summary.id)
        .collect()
}

/// Walks every page at `limit`, following the cursor, and returns the ids in
/// the order the caller would accumulate them.
fn paged(repository: &SqliteRepository, limit: u32) -> Vec<CreatorId> {
    let mut ids = Vec::new();
    let mut cursor = None;
    loop {
        let page = repository
            .list_creators(ListCreatorsInput {
                limit: Some(limit),
                cursor: cursor.clone(),
                ..Default::default()
            })
            .expect("creator page");
        ids.extend(page.items.into_iter().map(|summary| summary.id));
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => return ids,
        }
    }
}

#[test]
fn pinning_lifts_a_creator_that_had_fallen_down_the_list() {
    let (_directory, repository) = repository();
    let stale = creator_at(&repository, "Stale", 100);
    let middle = creator_at(&repository, "Middle", 200);
    let recent = creator_at(&repository, "Recent", 300);

    assert_eq!(
        listed(&repository, Default::default()),
        vec![recent.clone(), middle.clone(), stale.clone()]
    );

    repository.set_creator_pinned(&stale, true).expect("pin");

    assert_eq!(
        listed(&repository, Default::default()),
        vec![stale.clone(), recent.clone(), middle.clone()],
        "a pinned Creator comes back first from the query, not first from \
         whichever page happened to be loaded"
    );

    repository.set_creator_pinned(&stale, false).expect("unpin");

    assert_eq!(
        listed(&repository, Default::default()),
        vec![recent, middle, stale],
        "unpinning restores the recency order, so pinning never rewrites \
         updated_at"
    );
}

#[test]
fn paging_across_the_pinned_boundary_neither_repeats_nor_drops_a_row() {
    let (_directory, repository) = repository();
    let ids: Vec<CreatorId> = (0..7)
        .map(|index| creator_at(&repository, &format!("Creator {index}"), 100 + index))
        .collect();
    // Two pinned rows with distinct ranks, and their recency deliberately
    // disagrees with their pin order: index 1 is older than index 5, so a page
    // boundary that fell back to updated_at alone would reorder them.
    pin_at(&repository, &ids[1], 900);
    pin_at(&repository, &ids[5], 800);

    let whole = listed(
        &repository,
        ListCreatorsInput {
            limit: Some(100),
            ..Default::default()
        },
    );
    assert_eq!(whole.len(), 7);
    assert_eq!(whole[0], ids[1]);
    assert_eq!(whole[1], ids[5]);

    // limit 1 and 2 both land a page boundary inside the pinned block and on
    // the pinned/unpinned seam respectively.
    for limit in [1, 2, 3] {
        assert_eq!(
            paged(&repository, limit),
            whole,
            "paging at limit {limit} must reproduce the unpaged order exactly"
        );
    }
}

#[test]
fn a_cursor_minted_under_a_different_sort_key_is_rejected() {
    let (_directory, repository) = repository();
    creator_at(&repository, "Only", 100);
    // What the previous ordering encoded: one number and an id.
    let stale_cursor = encode_page_cursor(
        "creator_list|stage=any|query=|archived=false",
        "100|whatever",
    );

    let error = repository
        .list_creators(ListCreatorsInput {
            cursor: Some(stale_cursor),
            ..Default::default()
        })
        .expect_err("a two-segment cursor cannot place a row in this ordering");
    assert!(
        error.contains("page cursor is not valid for this query"),
        "{error}"
    );
}
