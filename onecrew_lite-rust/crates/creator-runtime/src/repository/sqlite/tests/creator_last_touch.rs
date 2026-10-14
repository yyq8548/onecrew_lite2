use super::*;
use crate::CreatorStage;

/// The three sources the derivation unions. Written straight to the tables so a
/// test states which source it is exercising, rather than driving five commit
/// paths to reach the same row.
fn touchpoint(repository: &SqliteRepository, creator_id: &CreatorId, at: u64) {
    repository
        .lock()
        .expect("lock")
        .execute(
            "INSERT INTO touchpoints (id, creator_id, kind, occurred_at, metadata_json)
             VALUES (?1, ?2, 'direct_message', ?3, '{}')",
            rusqlite::params![format!("tp-{at}"), creator_id.as_str(), at],
        )
        .expect("touchpoint");
}

fn outbound_email(repository: &SqliteRepository, creator_id: &CreatorId, at: u64) {
    email_row(repository, creator_id, at, "outbound");
}

fn inbound_email(repository: &SqliteRepository, creator_id: &CreatorId, at: u64) {
    email_row(repository, creator_id, at, "inbound");
}

fn email_row(repository: &SqliteRepository, creator_id: &CreatorId, at: u64, direction: &str) {
    let connection = repository.lock().expect("lock");
    connection
        .execute(
            "INSERT OR IGNORE INTO mailbox_refs
               (id, provider, label, email_address, capabilities_json, available,
                created_at, updated_at)
             VALUES ('mailbox', 'resend', 'label', 'us@example.com', '{}', 1, 1, 1)",
            [],
        )
        .expect("mailbox ref");
    let thread_id = format!("thread-{at}-{direction}");
    connection
        .execute(
            "INSERT INTO email_threads
               (id, mailbox_ref_id, creator_id, provider_thread_key, subject,
                active_folder, created_at, updated_at)
             VALUES (?1, 'mailbox', ?2, ?1, 'subject', 'sent', ?3, ?3)",
            rusqlite::params![thread_id, creator_id.as_str(), at],
        )
        .expect("thread");
    connection
        .execute(
            "INSERT INTO emails
               (id, thread_id, provider_message_id, direction, subject, body_text,
                delivery_status, occurred_at, origin)
             VALUES (?1, ?2, ?1, ?3, 'subject', 'body', 'delivered', ?4,
                     CASE ?3 WHEN 'outbound' THEN 'confirmed_send' ELSE 'provider_import' END)",
            rusqlite::params![format!("email-{at}-{direction}"), thread_id, direction, at],
        )
        .expect("email");
}

fn creator(repository: &SqliteRepository, name: &str) -> CreatorId {
    repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: name.into(),
            notes: None,
        })
        .expect("creator")
        .creator
        .id
}

fn last_touch(repository: &SqliteRepository, creator_id: &CreatorId) -> Option<u64> {
    repository
        .list_creators(Default::default())
        .expect("creators")
        .items
        .into_iter()
        .find(|summary| &summary.id == creator_id)
        .expect("creator in page")
        .last_touch_at
}

#[test]
fn a_creator_with_no_activity_has_no_last_touch() {
    let (_directory, repository) = repository();
    let id = creator(&repository, "Quiet");
    assert_eq!(last_touch(&repository, &id), None);
    assert_eq!(
        repository
            .get_creator(&id)
            .expect("creator")
            .relationship
            .last_touch_at,
        None
    );
}

#[test]
fn a_touchpoint_becomes_the_last_touch() {
    let (_directory, repository) = repository();
    let id = creator(&repository, "Touched");
    touchpoint(&repository, &id, 500);
    assert_eq!(last_touch(&repository, &id), Some(500));
}

#[test]
fn outbound_email_counts_even_without_a_touchpoint() {
    // 早于 touchpoint 写入点的历史外发邮件:只查 touchpoints 会漏掉。
    let (_directory, repository) = repository();
    let id = creator(&repository, "Mailed");
    outbound_email(&repository, &id, 700);
    assert_eq!(last_touch(&repository, &id), Some(700));
}

#[test]
fn an_inbound_reply_is_not_a_touch() {
    let (_directory, repository) = repository();
    let id = creator(&repository, "Replied to us");
    inbound_email(&repository, &id, 900);
    assert_eq!(
        last_touch(&repository, &id),
        None,
        "触达是我们主动发出去的动作;入站回复不算"
    );
}

#[test]
fn the_newest_source_wins_across_all_three() {
    let (_directory, repository) = repository();
    let id = creator(&repository, "Busy");
    touchpoint(&repository, &id, 100);
    outbound_email(&repository, &id, 300);
    inbound_email(&repository, &id, 9_999);
    assert_eq!(
        last_touch(&repository, &id),
        Some(300),
        "取三个来源的最大值,而 9999 那条是入站的,不参与"
    );
}

#[test]
fn one_creators_activity_does_not_leak_into_another() {
    let (_directory, repository) = repository();
    let touched = creator(&repository, "Touched");
    let untouched = creator(&repository, "Untouched");
    touchpoint(&repository, &touched, 400);
    assert_eq!(last_touch(&repository, &touched), Some(400));
    assert_eq!(last_touch(&repository, &untouched), None);
}

#[test]
fn the_row_closure_still_maps_every_other_column() {
    // SELECT 是位置取值,去掉 last_touch_at 那一列后所有索引前移了一位。
    // 漏改会静默错位(archived_at 读到 created_at,于是人人都是已归档)。
    let (_directory, repository) = repository();
    let id = creator(&repository, "Shape check");
    let summary = repository
        .list_creators(Default::default())
        .expect("creators")
        .items
        .into_iter()
        .find(|summary| summary.id == id)
        .expect("creator in page");
    assert_eq!(summary.display_name, "Shape check");
    assert_eq!(summary.stage, CreatorStage::Prospect);
    assert_eq!(summary.archived_at, None);
    assert_eq!(summary.platform_account_count, 0);
    assert_eq!(summary.contact_route_count, 0);
    assert_eq!(summary.pinned_at, None);
    assert!(!summary.has_manual_origin);
    assert_eq!(summary.score, None);
    assert!(summary.created_at > 0);
    assert_eq!(summary.created_at, summary.updated_at);
}
