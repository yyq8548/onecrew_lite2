use std::sync::Arc;

use onecrew_creator_runtime::{
    AccountAvailability, AccountKind, AccountRefId, EmailCapability, EmailFolder,
    EmailThreadGetInput, EmailThreadId, EmailThreadSort, ListCreatorsInput, ListEmailThreadsInput,
    MailboxGetInput, MailboxGetOutcome, MailboxRefId, MailboxRefInput, ManualCreatorCreateInput,
    Platform as CreatorPlatform,
};
use onecrew_local_domain::{Platform, ProfileSecret};

use onecrew_creator_sync::CreatorSyncRuntime;

use super::{LocalPlatform, LocalPlatformHostPaths};

#[tokio::test]
async fn weak_mailbox_callback_preserves_missing_binding_and_releases_the_creator_owner() {
    use crate::credentials::EmailMailboxAuthResolver;
    let directory = tempfile::tempdir().unwrap();
    let creator = Arc::new(
        onecrew_creator_runtime::CreatorRuntime::open(
            directory.path(),
            onecrew_creator_runtime::CreatorGateways::default(),
        )
        .unwrap(),
    );
    let weak = Arc::downgrade(&creator);
    let resolver = super::CreatorEmailMailboxResolver(weak.clone());
    assert_eq!(Arc::strong_count(&creator), 1);
    let missing = resolver
        .resolve_mailbox_auth_connection(&MailboxRefId::new("missing-mailbox"))
        .await
        .expect_err("missing binding must not select another mailbox");
    assert!(!missing.is_empty());
    assert_ne!(missing, "Creator mailbox owner is unavailable");
    drop(creator);
    assert!(
        weak.upgrade().is_none(),
        "the resolver must not create a strong owner cycle"
    );
    assert_eq!(
        resolver
            .resolve_mailbox_auth_connection(&MailboxRefId::new("missing-mailbox"))
            .await
            .unwrap_err(),
        "Creator mailbox owner is unavailable"
    );
}

fn sqlite_data_version(connection: &rusqlite::Connection) -> i64 {
    connection
        .query_row("PRAGMA data_version", [], |row| row.get(0))
        .unwrap()
}

type AccountRefSnapshot = Vec<(String, String, String, String, bool, String, u64, u64)>;

fn account_ref_snapshot(connection: &rusqlite::Connection) -> AccountRefSnapshot {
    let mut statement = connection
        .prepare(
            "SELECT id, platform, kind, label, active, availability, created_at, updated_at
             FROM account_refs ORDER BY id",
        )
        .unwrap();
    statement
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn profile_secret(provider_account_id: &str) -> ProfileSecret {
    ProfileSecret {
        provider_account_id: provider_account_id.into(),
        cookies: Vec::new(),
        x_chat_passcode: None,
        instagram_session_settings: None,
        telegram_api_id: None,
        telegram_api_hash: None,
        telegram_session: None,
        profile_name: "Pure accessor fixture".into(),
        browser_profile_dir: String::new(),
        fingerprint_seed: String::new(),
        proxy: None,
        proxy_egress: None,
    }
}

#[test]
fn creator_accessor_and_reads_do_not_reconcile_vault_projections() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let creator_runtime = platform.creators().unwrap();

    let existing_account_ref_id = AccountRefId::new("existing-account");
    creator_runtime
        .sync_account_ref_with_state(
            existing_account_ref_id.clone(),
            CreatorPlatform::X,
            AccountKind::Profile,
            "Existing account",
            AccountAvailability::Available,
        )
        .unwrap();
    let creator = creator_runtime
        .create_manual_creator(ManualCreatorCreateInput {
            tags: None,
            display_name: "Readable Creator".into(),
            notes: Some("non-empty read fixture".into()),
            contact_routes: Vec::new(),
        })
        .unwrap();
    let mailbox = creator_runtime
        .sync_mailbox_ref(MailboxRefInput {
            id: MailboxRefId::new("11111111-1111-4111-8111-111111111111"),
            provider: "resend".into(),
            label: "Readable Mailbox".into(),
            email_address: "reads@example.com".into(),
            display_name: Some("Read Fixture".into()),
            capabilities: vec![EmailCapability::Inbox],
            available: true,
            last_error: None,
        })
        .unwrap();
    let thread_id = EmailThreadId::new("pure-read-thread");
    {
        let fixture_writer = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
        fixture_writer
            .execute(
                "INSERT INTO email_threads
                   (id, mailbox_ref_id, creator_id, contact_route_id, provider_thread_key,
                    subject, unread_count, active_folder, archived_at, deleted_at,
                    created_at, updated_at)
                 VALUES (?1, ?2, ?3, NULL, 'provider-thread-1',
                         'Readable Thread', 1, 'inbox', NULL, NULL, 10, 10)",
                rusqlite::params![
                    thread_id.as_str(),
                    mailbox.mailbox_ref_id.as_str(),
                    creator.creator_id.as_str()
                ],
            )
            .unwrap();
        fixture_writer
            .execute(
                "INSERT INTO emails
                   (id, thread_id, provider_message_id, internet_message_id, direction,
                    subject, body_text, body_html, in_reply_to, references_json,
                    delivery_status, headers_json, occurred_at, origin)
                 VALUES ('pure-read-email', ?1, 'provider-message-1', '<read@example.com>',
                         'inbound', 'Readable Thread', 'fixture body', NULL, NULL, '[]',
                         'unread', '{}', 10, 'provider_import')",
                [thread_id.as_str()],
            )
            .unwrap();
        fixture_writer
            .execute(
                "INSERT INTO mailbox_sync_state
                   (mailbox_ref_id, direction, newest_provider_message_id,
                    newest_occurred_at, oldest_backfill_cursor, backfill_complete,
                    last_page_at, last_full_sync_at, failure_json, updated_at)
                 VALUES (?1, 'inbound', 'provider-message-1', 10, NULL, 1,
                         10, 10, NULL, 10)",
                [mailbox.mailbox_ref_id.as_str()],
            )
            .unwrap();
        for (kind, address) in [
            ("from", "creator@example.com"),
            ("to", mailbox.email_address.as_str()),
        ] {
            fixture_writer
                .execute(
                    "INSERT INTO email_recipients
                       (email_id, recipient_kind, address, locator_id)
                     VALUES ('pure-read-email', ?1, ?2, NULL)",
                    rusqlite::params![kind, address],
                )
                .unwrap();
        }
    }

    let profile = platform
        .vault
        .upsert(Platform::X, "pure_accessor", &profile_secret("provider-1"))
        .unwrap();
    let observer = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
    let data_version_before_reads = sqlite_data_version(&observer);
    let account_refs_before_reads = account_ref_snapshot(&observer);
    assert_eq!(account_refs_before_reads.len(), 1);
    assert_eq!(
        account_refs_before_reads[0].0,
        existing_account_ref_id.as_str()
    );
    let writer_leases_before_reads =
        creator_runtime.writer_lease_acquisition_count_snapshot_for_test();

    // Repeated accessors return the initialized application graph. Every
    // read below must also succeed against a real row, so a query failure
    // cannot masquerade as evidence that the writer owner stayed free.
    let first = platform.creators().unwrap();
    let second = platform.creators().unwrap();
    let third = platform.creators().unwrap();

    assert_eq!(
        first
            .get_account_ref(&existing_account_ref_id)
            .unwrap()
            .label,
        "Existing account"
    );
    let account_refs = second
        .list_account_refs(CreatorPlatform::X, AccountKind::Profile)
        .unwrap();
    assert_eq!(account_refs.len(), 1);
    assert_eq!(account_refs[0].id, existing_account_ref_id);

    let creators = first.list_creators(ListCreatorsInput::default()).unwrap();
    assert_eq!(creators.items.len(), 1);
    assert_eq!(creators.items[0].id, creator.creator_id);
    assert_eq!(
        second
            .get_creator(&creator.creator_id)
            .unwrap()
            .creator
            .display_name,
        "Readable Creator"
    );

    let mailboxes = third.list_mailboxes().unwrap();
    assert_eq!(mailboxes.len(), 1);
    assert_eq!(mailboxes[0].mailbox_ref_id, mailbox.mailbox_ref_id);
    assert!(matches!(
        first
            .get_mailbox(MailboxGetInput {
                mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
            })
            .unwrap(),
        MailboxGetOutcome::Found { detail }
            if detail.mailbox_ref_id == mailbox.mailbox_ref_id
    ));
    let threads = second
        .list_email_threads(ListEmailThreadsInput {
            mailbox_ref_id: Some(mailbox.mailbox_ref_id.clone()),
            folder: EmailFolder::Inbox,
            sort: EmailThreadSort::Newest,
            limit: None,
            cursor: None,
        })
        .unwrap();
    assert_eq!(threads.items.len(), 1);
    assert_eq!(threads.items[0].thread.id, thread_id);
    assert_eq!(threads.items[0].message_count, 1);
    let thread = third
        .get_email_thread(EmailThreadGetInput {
            thread_id: thread_id.clone(),
            limit: None,
            cursor: None,
        })
        .unwrap();
    assert_eq!(thread.thread.id, thread_id);
    assert_eq!(thread.messages.len(), 1);
    assert_eq!(thread.messages[0].text.as_deref(), Some("fixture body"));
    let sync_status = first.mailbox_sync_status(&mailbox.mailbox_ref_id).unwrap();
    assert_eq!(sync_status.mailbox.mailbox_ref_id, mailbox.mailbox_ref_id);
    assert!(!sync_status.running);
    assert_eq!(sync_status.inbound.unwrap().last_full_sync_at, Some(10));
    assert!(sync_status.outbound.is_none());

    assert_eq!(
        third
            .get_creator(&creator.creator_id)
            .unwrap()
            .creator
            .notes
            .as_deref(),
        Some("non-empty read fixture")
    );
    assert_eq!(
        creator_runtime.writer_lease_acquisition_count_snapshot_for_test(),
        writer_leases_before_reads
    );
    assert_eq!(sqlite_data_version(&observer), data_version_before_reads);
    assert_eq!(account_ref_snapshot(&observer), account_refs_before_reads);

    platform.reconcile_vault_projections().unwrap();
    assert!(
        creator_runtime.writer_lease_acquisition_count_snapshot_for_test()
            > writer_leases_before_reads
    );
    assert!(sqlite_data_version(&observer) > data_version_before_reads);
    let account_refs_after_reconcile = account_ref_snapshot(&observer);
    assert_eq!(account_refs_after_reconcile.len(), 2);
    assert!(account_refs_after_reconcile.iter().any(|row| {
        row.0 == profile.id.to_string()
            && row.1 == CreatorPlatform::X.as_str()
            && row.2 == AccountKind::Profile.as_str()
            && row.3 == "pure_accessor"
    }));
    let account_ref_id = AccountRefId::new(profile.id.to_string());
    assert_eq!(
        first.get_account_ref(&account_ref_id).unwrap().id,
        account_ref_id
    );
}

#[test]
fn adapter_entries_retain_one_injected_creator_sync_coordinator() {
    let directory = tempfile::tempdir().unwrap();
    let coordinator = Arc::new(CreatorSyncRuntime::open(directory.path()).unwrap());
    let platform = Arc::new(
        LocalPlatform::open_with_host_paths_and_creator_sync(
            directory.path(),
            LocalPlatformHostPaths::without_system_desktop(),
            coordinator.clone(),
        )
        .unwrap(),
    );
    let tauri_entry = platform.clone();
    let lifecycle_entry = platform;

    assert!(Arc::ptr_eq(
        &tauri_entry.creator_sync,
        &lifecycle_entry.creator_sync
    ));
    assert!(Arc::ptr_eq(&tauri_entry.creator_sync, &coordinator));
    assert!(Arc::ptr_eq(tauri_entry.creator_sync(), &coordinator));
}
