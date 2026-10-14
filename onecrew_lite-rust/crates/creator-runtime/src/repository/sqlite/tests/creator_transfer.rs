use super::*;
use crate::{CreateCreatorGroupInput, CreatorGroupDestination, SetCreatorGroupInput};
use rusqlite::Connection;

#[test]
fn workbook_export_carries_local_annotations_without_changing_the_portable_bundle() {
    let (directory, repository) = repository();
    let created = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Annotated".into(),
            notes: None,
        })
        .unwrap();
    let group = repository
        .create_creator_group(CreateCreatorGroupInput {
            name: "Local group".into(),
        })
        .unwrap();
    repository
        .set_creator_group(SetCreatorGroupInput {
            creator_id: created.creator.id.clone(),
            destination: CreatorGroupDestination::Group { group_id: group.id },
        })
        .unwrap();
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch(&format!(
            "UPDATE creators SET updated_at = 1700000100 WHERE id = '{id}';
             INSERT INTO touchpoints (id, creator_id, kind, occurred_at, metadata_json)
             VALUES ('workbook-touch', '{id}', 'email', 1700000000, '{{}}');",
            id = created.creator.id.as_str()
        ))
        .unwrap();

    let portable = repository.export_creator_bundle().unwrap();
    let workbook = repository.export_creator_workbook().unwrap();
    assert_eq!(workbook.bundle, portable);
    let display = workbook
        .display_by_record_key
        .get(&portable.creators[0].record_key)
        .unwrap();
    assert_eq!(display.last_touch_at, Some(1_700_000_000));
    assert_eq!(display.updated_at, 1_700_000_100);
    assert_eq!(display.group_name.as_deref(), Some("Local group"));

    let bytes = workbook.to_xlsx_bytes().unwrap();
    {
        use std::io::Read;

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let mut creators_sheet = String::new();
        archive
            .by_name("xl/worksheets/sheet1.xml")
            .unwrap()
            .read_to_string(&mut creators_sheet)
            .unwrap();
        assert!(creators_sheet.contains("r=\"L2\""));
        assert!(creators_sheet.contains("r=\"M2\""));
        assert!(creators_sheet.contains("Local group"));
    }
    assert_eq!(
        crate::CreatorBundle::from_xlsx_bytes(&bytes).unwrap(),
        portable
    );
}

#[test]
fn workbook_export_ignores_unattributed_outbound_email_threads() {
    let (directory, repository) = repository();
    let creator = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Attributed email".into(),
            notes: None,
        })
        .unwrap();
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    connection
        .execute_batch(
            "INSERT INTO mailbox_refs
               (id, provider, label, email_address, capabilities_json, available,
                created_at, updated_at)
             VALUES ('mailbox-export-null', 'resend', 'Export test',
                     'export@example.com', '{}', 1, 1, 1);
             INSERT INTO email_threads
               (id, mailbox_ref_id, creator_id, provider_thread_key, subject,
                active_folder, created_at, updated_at)
             VALUES
               ('thread-attributed-export', 'mailbox-export-null',
                NULL, 'provider-attributed', 'Attributed', 'sent', 10, 10),
               ('thread-unattributed-export', 'mailbox-export-null', NULL,
                'provider-unattributed', 'Unattributed', 'sent', 20, 20);
             INSERT INTO emails
               (id, thread_id, provider_message_id, direction, subject, body_text,
                delivery_status, occurred_at, origin)
             VALUES
               ('email-attributed-export', 'thread-attributed-export',
                'provider-message-attributed', 'outbound', 'Attributed', 'Body',
                'delivered', 100, 'confirmed_send'),
               ('email-unattributed-export', 'thread-unattributed-export',
                'provider-message-unattributed', 'outbound', 'Unattributed', 'Body',
                'delivered', 200, 'confirmed_send');",
        )
        .unwrap();
    connection
        .execute(
            "UPDATE email_threads SET creator_id = ?1
             WHERE id = 'thread-attributed-export'",
            [creator.creator.id.as_str()],
        )
        .unwrap();

    let workbook = repository.export_creator_workbook().unwrap();
    let display = workbook
        .display_by_record_key
        .get(&workbook.bundle.creators[0].record_key)
        .unwrap();
    assert_eq!(display.last_touch_at, Some(100));
    assert!(!workbook.to_xlsx_bytes().unwrap().is_empty());
    assert_eq!(
        connection
            .query_row(
                "SELECT creator_id FROM email_threads
                 WHERE id = 'thread-unattributed-export'",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .unwrap(),
        None
    );
}

#[test]
fn export_bundle_is_list_complete_and_ignores_groups_and_timestamps() {
    let (directory, repository) = repository();
    let first = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "First".into(),
            notes: Some("first Creator note".into()),
        })
        .unwrap();
    let second = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Second".into(),
            notes: Some("second Creator note".into()),
        })
        .unwrap();
    let account = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::X,
            target_kind: TargetKind::User,
            external_id: "public-x-id".into(),
        })
        .unwrap();
    repository
        .link_creator_account(
            &first.creator.id,
            &account.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .unwrap();
    let private_account_ref = AccountRefId::new("private-account-reference");
    repository
        .upsert_account_ref(
            private_account_ref.clone(),
            Platform::X,
            "profile",
            "Private operator",
        )
        .unwrap();
    let source = repository
        .create_observation_source(ObservationSourceInput {
            kind: SourceKind::InitialPlatformAccount,
            platform: Some(Platform::X),
            source_url: None,
            external_ref: Some("public-source-reference".into()),
            account_ref_id: Some(private_account_ref),
            metadata: serde_json::json!({"public_source_metadata": true}),
        })
        .unwrap();
    repository
        .append_platform_account_observation(PlatformAccountObservationInput {
            platform_account_id: account.id,
            source_id: source.id,
            handle: Some("public_handle".into()),
            display_name: Some("Public Profile".into()),
            profile_url: Some("https://x.com/public_handle".into()),
            public_data: serde_json::json!({
                "profile_image_url": "https://cdn.example/avatar.png",
                "phone": "+10000000000",
                "accessToken": "must-not-export",
                "matched_post": {"text": "must-not-export"}
            }),
        })
        .unwrap();
    repository.archive_creator(&second.creator.id).unwrap();
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch(&format!(
            "INSERT INTO contact_locators
               (id, channel, kind, normalized_value, display_value, created_at, updated_at)
             VALUES
               ('locator-z-first', 'email', 'email_address', 'z-first@example.com',
                'z-first@example.com', 500, 500),
               ('locator-a-second', 'email', 'email_address', 'a-second@example.com',
                'a-second@example.com', 500, 500);
             INSERT INTO creator_contact_routes
               (id, creator_id, locator_id, confidence, created_at, updated_at)
             VALUES
               ('route-z-first', '{}', 'locator-z-first', 'exact', 500, 500),
               ('route-a-second', '{}', 'locator-a-second', 'exact', 500, 500);",
            first.creator.id.as_str(),
            first.creator.id.as_str()
        ))
        .unwrap();

    let first_export = repository.export_creator_bundle().unwrap();
    let group = repository
        .create_creator_group(CreateCreatorGroupInput {
            name: "Never exported".into(),
        })
        .unwrap();
    repository
        .set_creator_group(SetCreatorGroupInput {
            creator_id: first.creator.id.clone(),
            destination: CreatorGroupDestination::Group { group_id: group.id },
        })
        .unwrap();
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch(
            "UPDATE creators SET created_at = created_at + 1000, updated_at = updated_at + 1000;
             UPDATE creator_relationships SET created_at = created_at + 1000, updated_at = updated_at + 1000;
             UPDATE platform_accounts SET created_at = created_at + 1000, updated_at = updated_at + 1000;
             UPDATE creator_account_links SET created_at = created_at + 1000, updated_at = updated_at + 1000;
             UPDATE platform_account_observations SET observed_at = observed_at + 1000;",
        )
        .unwrap();
    let second_export = repository.export_creator_bundle().unwrap();

    assert_eq!(first_export, second_export);
    assert_eq!(first_export.creator_count, 2);
    assert_eq!(
        first_export
            .creators
            .iter()
            .map(|creator| creator.record_key.as_str())
            .collect::<Vec<_>>(),
        {
            let mut keys = first_export
                .creators
                .iter()
                .map(|creator| creator.record_key.as_str())
                .collect::<Vec<_>>();
            keys.sort_unstable();
            keys
        }
    );
    let archived = first_export
        .creators
        .iter()
        .find(|creator| creator.display_name == "Second")
        .unwrap();
    assert_eq!(
        archived.relationship.stage,
        crate::CreatorTransferStage::Archived
    );
    let first_creator = first_export
        .creators
        .iter()
        .find(|creator| creator.display_name == "First")
        .unwrap();
    assert_eq!(
        first_creator
            .contact_routes
            .iter()
            .map(|route| route.normalized_value.as_str())
            .collect::<Vec<_>>(),
        ["z-first@example.com", "a-second@example.com"]
    );

    let json = serde_json::to_string(&first_export).unwrap();
    for excluded in [
        "first Creator note",
        "second Creator note",
        "Never exported",
        "public-source-reference",
        "public_source_metadata",
        "private-account-reference",
        "Private operator",
        "created_at",
        "updated_at",
        "observed_at",
        "+10000000000",
        "must-not-export",
    ] {
        assert!(!json.contains(excluded), "bundle leaked {excluded}");
    }
    for included in [
        "public_handle",
        "avatar_url",
        "https://cdn.example/avatar.png",
    ] {
        assert!(json.contains(included), "bundle omitted {included}");
    }
}

#[test]
fn export_rejects_the_creator_count_cap_before_loading_records() {
    let (_directory, repository) = repository();
    super::super::creator_transfer::insert_creators_for_cap_test(
        &repository,
        crate::CREATOR_BUNDLE_MAX_CREATORS + 1,
    )
    .unwrap();
    assert_eq!(
        repository.export_creator_bundle().unwrap_err(),
        format!(
            "creator bundle exceeds the maximum of {} Creators",
            crate::CREATOR_BUNDLE_MAX_CREATORS
        )
    );
}

#[test]
fn export_rejects_the_account_cap_after_at_most_sixty_five_rows() {
    let (_directory, repository) = repository();
    let creator = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Bounded accounts".into(),
            notes: None,
        })
        .unwrap();
    for index in 0..=crate::CREATOR_BUNDLE_MAX_ACCOUNTS_PER_CREATOR {
        let account = repository
            .upsert_platform_account(UpsertPlatformAccountInput {
                platform: Platform::X,
                target_kind: TargetKind::User,
                external_id: format!("bounded-{index:03}"),
            })
            .unwrap();
        repository
            .link_creator_account(
                &creator.creator.id,
                &account.id,
                LinkState::Confirmed,
                Confidence::Exact,
            )
            .unwrap();
    }

    assert_eq!(
        repository.export_creator_bundle().unwrap_err(),
        format!(
            "creator {} exceeds the maximum of {} transferable accounts",
            creator.creator.id.as_str(),
            crate::CREATOR_BUNDLE_MAX_ACCOUNTS_PER_CREATOR
        )
    );
}

/// Fixed store used by the golden tests: multiple accounts per Creator,
/// multiple contact routes with equal `created_at`, and several
/// `locator_observations` sharing one `observed_at`, so every sort key that
/// needs a rowid tiebreak is actually exercised.
/// Inserts the golden fixture. `equal_observations_reversed` inserts the three
/// `locator-mail` observations that share `observed_at = 600` in the opposite
/// row order so a test can prove that only the `rowid` tiebreak decides how
/// equal keys sort, and that nothing else in the bundle moves with it.
fn insert_golden_fixture(path: &std::path::Path, equal_observations_reversed: bool) {
    let connection = Connection::open(path).unwrap();
    connection
        .execute_batch(
            "INSERT INTO creators
               (id, display_name, notes, created_at, updated_at, archived_at, score,
                assessment, follower_counts_json, pinned_at)
             VALUES
               ('creator-a', 'Alpha', 'private note', 10, 10, NULL, 50, NULL,
                '{\"x\":10,\"youtube\":2000}', 5),
               ('creator-b', 'Beta', NULL, 20, 20, NULL, NULL, NULL, '{}', NULL),
               ('creator-c', 'Gamma', NULL, 30, 30, 30, 7, NULL, '{\"instagram\":3}', NULL);
             INSERT INTO creator_relationships
               (creator_id, stage, last_touch_at, created_at, updated_at)
             VALUES
               ('creator-a', 'engaged', NULL, 10, 10),
               ('creator-b', 'prospect', NULL, 20, 20),
               ('creator-c', 'archived', NULL, 30, 30);
             INSERT INTO platform_accounts
               (id, platform, target_kind, external_id, created_at, updated_at)
             VALUES
               ('account-x', 'x', 'user', 'x-alpha', 100, 100),
               ('account-ig', 'instagram', 'creator', 'ig-alpha', 100, 100),
               ('account-yt', 'youtube', 'channel', 'yt-alpha', 101, 101),
               ('account-tg', 'telegram', 'channel', 'tg-gamma', 100, 100),
               ('account-orphan', 'x', 'user', 'x-orphan', 99, 99);
             INSERT INTO creator_account_links
               (id, creator_id, platform_account_id, state, confidence, created_at, updated_at)
             VALUES
               ('link-a-x', 'creator-a', 'account-x', 'confirmed', 'exact', 100, 100),
               ('link-a-ig', 'creator-a', 'account-ig', 'claimed', 'asserted', 100, 100),
               ('link-a-yt', 'creator-a', 'account-yt', 'rejected', 'possible', 101, 101),
               ('link-c-tg', 'creator-c', 'account-tg', 'confirmed', 'exact', 100, 100),
               ('link-c-x', 'creator-c', 'account-x', 'claimed', 'possible', 102, 102);
             INSERT INTO observation_sources
               (id, kind, platform, source_url, external_ref, account_ref_id, metadata_json,
                observed_at)
             VALUES
               ('source-initial', 'initial_platform_account', 'x', NULL, NULL, NULL, '{}', 100),
               ('source-bio', 'profile_bio', 'x', NULL, NULL, NULL, '{}', 100),
               ('source-manual', 'manual_entry', NULL, NULL, NULL, NULL, '{}', 100),
               ('source-hub', 'link_hub', 'instagram', NULL, NULL, NULL, '{}', 100),
               ('source-video', 'video_description', 'youtube', NULL, NULL, NULL, '{}', 100);
             INSERT INTO platform_account_observations
               (id, platform_account_id, source_id, handle, display_name, profile_url,
                public_json, observed_at)
             VALUES
               ('obs-x-old', 'account-x', 'source-bio', 'alpha_old', 'Alpha Old',
                'https://x.com/alpha_old', '{}', 200),
               ('obs-x-tie-1', 'account-x', 'source-initial', 'alpha_tie_1', 'Alpha Tie 1',
                'https://x.com/alpha_tie_1', '{\"profile_image_url\":\"https://cdn.example/a1.png\"}', 300),
               ('obs-x-tie-2', 'account-x', 'source-hub', 'alpha_tie_2', 'Alpha Tie 2',
                'https://x.com/alpha_tie_2', '{\"profile_image_url\":\"https://cdn.example/a2.png\"}', 300),
               ('obs-ig', 'account-ig', 'source-hub', NULL, NULL, NULL, '{}', 300);
             INSERT INTO contact_locators
               (id, channel, kind, normalized_value, display_value, created_at, updated_at)
             VALUES
               ('locator-mail', 'email', 'email_address', 'alpha@example.com',
                'Alpha@example.com', 400, 400),
               ('locator-handle', 'x', 'handle', 'alpha', '@alpha', 400, 400),
               ('locator-site', 'website', 'url', 'https://alpha.example', 'https://alpha.example',
                401, 401),
               ('locator-beta', 'email', 'email_address', 'beta@example.com', 'beta@example.com',
                400, 400);
             INSERT INTO creator_contact_routes
               (id, creator_id, locator_id, confidence, created_at, updated_at)
             VALUES
               ('route-a-handle', 'creator-a', 'locator-handle', 'asserted', 500, 500),
               ('route-a-mail', 'creator-a', 'locator-mail', 'exact', 500, 500),
               ('route-a-site', 'creator-a', 'locator-site', 'possible', 499, 499),
               ('route-b-mail', 'creator-b', 'locator-beta', 'exact', 500, 500);
             INSERT INTO locator_observations
               (id, locator_id, source_id, creator_id, platform_account_id, observed_value,
                observed_at)
             VALUES
               ('lobs-mail-4', 'locator-mail', 'source-video', NULL, 'account-yt', 'alpha@example.com', 700),
               ('lobs-mail-orphan', 'locator-mail', 'source-hub', NULL, 'account-orphan', 'alpha@example.com', 800),
               ('lobs-mail-other', 'locator-mail', 'source-manual', 'creator-b', NULL, 'alpha@example.com', 900),
               ('lobs-handle-1', 'locator-handle', 'source-initial', NULL, 'account-x', 'alpha', 600),
               ('lobs-handle-2', 'locator-handle', 'source-manual', 'creator-a', NULL, 'alpha', 550);",
        )
        .unwrap();
    let mut equal_observations = [
        "('lobs-mail-1', 'locator-mail', 'source-bio', NULL, 'account-x', 'alpha@example.com', 600)",
        "('lobs-mail-2', 'locator-mail', 'source-manual', 'creator-a', NULL, 'alpha@example.com', 600)",
        "('lobs-mail-3', 'locator-mail', 'source-hub', NULL, 'account-ig', 'alpha@example.com', 600)",
    ];
    if equal_observations_reversed {
        equal_observations.reverse();
    }
    for row in equal_observations {
        connection
            .execute_batch(&format!(
                "INSERT INTO locator_observations
                   (id, locator_id, source_id, creator_id, platform_account_id, observed_value,
                    observed_at)
                 VALUES {row};"
            ))
            .unwrap();
    }
}

/// Digests pinned from the batch loader. Beta and Gamma have no
/// equal-`observed_at` observations, so their values are identical to what the
/// per-Creator loader produced. Alpha's `locator-mail` route has three
/// observations sharing `observed_at = 600`; the per-Creator query left that
/// tie in unspecified (rowid ascending) order while the batch query pins it to
/// `rowid DESC`, so Alpha's digests and the bundle `content_sha256` changed
/// once with the rewrite. `golden_fixture_equal_observations_follow_rowid_only`
/// proves that tiebreak is the only input behind the difference.
const GOLDEN_ALPHA_SEMANTIC: &str =
    "sha256:e4dd7e5f11b8c7045a6149880b8e1c111f6cc158ece112aedf0ff31ad59bac86";
const GOLDEN_ALPHA_RECORD_KEY: &str =
    "sha256:44e9b1e0d34f0176a5274ed037469441c9b46791963b6918f00c7524accd7ca2";
const GOLDEN_BETA_SEMANTIC: &str =
    "sha256:ce437e46267f6b66cea16c8c7afabaf9c62d51f1f2b060592f27272d65b909a1";
const GOLDEN_BETA_RECORD_KEY: &str =
    "sha256:66dfcc3a74153f78c75f5cc754e70b51a6729c0a3b166a16807e5fd367bb0529";
const GOLDEN_GAMMA_SEMANTIC: &str =
    "sha256:e88ca402db2893617451a3b456536aa1f8c05f7e2ba353c69cb376acb76cdc75";
const GOLDEN_GAMMA_RECORD_KEY: &str =
    "sha256:96bc87a324dad1e06fae3b576432229fdf7cb183ffef6d5f1a6930b948db81d1";
const GOLDEN_CONTENT: &str =
    "sha256:354745203d0a18db2f7806aa9878faea4bbd033b0b47d7c6978b7c251e639d32";
/// Digests of the reversed-tie fixture. They equal what the per-Creator loader
/// produced on the forward fixture (its unspecified tie order was rowid
/// ascending, which `rowid DESC` over reversed rows reproduces), so the
/// pre-batch baseline stays re-derivable from the tree.
const GOLDEN_REVERSED_ALPHA_SEMANTIC: &str =
    "sha256:2b6f0311e76c67262fbaa5ac544cd52da653dff08e2e5917845b57121dcc4078";
const GOLDEN_REVERSED_CONTENT: &str =
    "sha256:52ce6115838e766564352047462b73fd8ecd8796e5aa3edf40ed2d9afc0e5d68";

// v2 pins a new root identity while retaining all released v1 golden values.
const GOLDEN_V2_CONTENT: &str =
    "sha256:3215ef1972c8107845f6ad103156ad790562b48ab1b4c4d838ff6fa595a9a594";
const GOLDEN_V2_REVERSED_CONTENT: &str =
    "sha256:490cdfddf5b131e74ac20ed76ae18ea23612f22e16084c6e230a4cc9242b8588";

// Independently construct the documented wire digest array. Verifying both
// versions against frozen constants prevents changing historical goldens to
// disguise a changed Creator projection or root encoding.
fn assert_versioned_root_golden(bundle: &crate::CreatorBundle, v1: &str, v2: &str) {
    use sha2::{Digest, Sha256};
    for (version, expected) in [(1, v1), (2, v2)] {
        let canonical = serde_json::json!([
            "reacher.creator_bundle",
            version,
            "portable_public",
            "full",
            3,
            bundle
                .creators
                .iter()
                .map(|creator| { serde_json::json!([creator.record_key, creator.semantic_sha256]) })
                .collect::<Vec<_>>()
        ]);
        let digest = Sha256::digest(serde_json::to_vec(&canonical).unwrap());
        assert_eq!(format!("sha256:{digest:x}"), expected);
    }
    assert_eq!(bundle.format_version, 2);
    assert_eq!(bundle.content_sha256, v2);
}

#[test]
fn golden_fixture_export_matches_the_recorded_digests() {
    let (directory, repository) = repository();
    insert_golden_fixture(&directory.path().join("crm.db"), false);
    let current = repository.export_creator_bundle().unwrap();
    assert_eq!(current.format_version, 3);
    let bundle = crate::creator_transfer::legacy_v2_projection_for_test(current);
    let by_name = |name: &str| {
        bundle
            .creators
            .iter()
            .find(|creator| creator.display_name == name)
            .unwrap()
    };
    let alpha = by_name("Alpha");
    let beta = by_name("Beta");
    let gamma = by_name("Gamma");

    assert_eq!(
        alpha.relationship.stage,
        crate::CreatorTransferStage::Engaged
    );
    assert!(alpha.pinned);
    assert_eq!(alpha.score, Some(50));
    assert_eq!(
        alpha.follower_counts,
        std::collections::BTreeMap::from([
            (crate::CreatorTransferPlatform::X, 10),
            (crate::CreatorTransferPlatform::YouTube, 2000),
        ])
    );
    assert_eq!(
        alpha
            .accounts
            .iter()
            .map(|account| (
                account.capture_order,
                account.external_id.as_str(),
                account.source_kind.as_str(),
                account
                    .latest_profile
                    .as_ref()
                    .and_then(|profile| profile.handle.as_deref()),
                account
                    .latest_profile
                    .as_ref()
                    .and_then(|profile| profile.avatar_url.as_deref()),
            ))
            .collect::<Vec<_>>(),
        [
            (
                0,
                "x-alpha",
                "link_hub",
                Some("alpha_tie_2"),
                Some("https://cdn.example/a2.png")
            ),
            (1, "ig-alpha", "link_hub", None, None),
            (3, "yt-alpha", "initial_platform_account", None, None),
        ]
    );
    assert_eq!(
        alpha
            .contact_routes
            .iter()
            .map(|route| (
                route.save_order,
                route.normalized_value.as_str(),
                route
                    .source_kinds
                    .iter()
                    .map(|kind| kind.as_str())
                    .collect::<Vec<_>>(),
            ))
            .collect::<Vec<_>>(),
        [
            (0, "https://alpha.example", vec![]),
            (1, "alpha", vec!["initial_platform_account", "manual_entry"]),
            (
                2,
                "alpha@example.com",
                vec![
                    "video_description",
                    "link_hub",
                    "manual_entry",
                    "profile_bio"
                ]
            ),
        ]
    );
    assert!(beta.accounts.is_empty());
    assert_eq!(beta.score, None);
    assert!(beta.follower_counts.is_empty());
    assert_eq!(beta.contact_routes.len(), 1);
    assert!(beta.contact_routes[0].source_kinds.is_empty());
    assert_eq!(
        gamma.relationship.stage,
        crate::CreatorTransferStage::Archived
    );
    assert_eq!(
        gamma
            .accounts
            .iter()
            .map(|account| (account.capture_order, account.external_id.as_str()))
            .collect::<Vec<_>>(),
        [(0, "x-alpha"), (2, "tg-gamma")]
    );
    assert!(gamma.contact_routes.is_empty());

    assert_eq!(beta.semantic_sha256, GOLDEN_BETA_SEMANTIC);
    assert_eq!(beta.record_key, GOLDEN_BETA_RECORD_KEY);
    assert_eq!(gamma.semantic_sha256, GOLDEN_GAMMA_SEMANTIC);
    assert_eq!(gamma.record_key, GOLDEN_GAMMA_RECORD_KEY);
    assert_eq!(alpha.semantic_sha256, GOLDEN_ALPHA_SEMANTIC);
    assert_eq!(alpha.record_key, GOLDEN_ALPHA_RECORD_KEY);
    assert_versioned_root_golden(&bundle, GOLDEN_CONTENT, GOLDEN_V2_CONTENT);
    assert_eq!(
        crate::creator_transfer::legacy_v2_projection_for_test(
            repository.export_creator_bundle().unwrap()
        )
        .content_sha256,
        GOLDEN_V2_CONTENT
    );
}

#[test]
fn golden_fixture_equal_observations_follow_rowid_only() {
    let (directory, repository) = repository();
    insert_golden_fixture(&directory.path().join("crm.db"), true);
    let current = repository.export_creator_bundle().unwrap();
    assert_eq!(current.format_version, 3);
    let bundle = crate::creator_transfer::legacy_v2_projection_for_test(current);
    let by_name = |name: &str| {
        bundle
            .creators
            .iter()
            .find(|creator| creator.display_name == name)
            .unwrap()
    };
    let alpha = by_name("Alpha");
    let beta = by_name("Beta");
    let gamma = by_name("Gamma");

    // Same rows, reversed insertion order: the `observed_at = 600` tie flips
    // to the new rowid order and nothing else in the route changes.
    assert_eq!(
        alpha
            .contact_routes
            .iter()
            .map(|route| route
                .source_kinds
                .iter()
                .map(|kind| kind.as_str())
                .collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        [
            vec![],
            vec!["initial_platform_account", "manual_entry"],
            vec![
                "video_description",
                "profile_bio",
                "manual_entry",
                "link_hub"
            ]
        ]
    );
    assert_eq!(alpha.semantic_sha256, GOLDEN_REVERSED_ALPHA_SEMANTIC);
    assert_versioned_root_golden(&bundle, GOLDEN_REVERSED_CONTENT, GOLDEN_V2_REVERSED_CONTENT);
    assert_ne!(alpha.semantic_sha256, GOLDEN_ALPHA_SEMANTIC);
    assert_ne!(bundle.content_sha256, GOLDEN_V2_CONTENT);
    assert_eq!(beta.semantic_sha256, GOLDEN_BETA_SEMANTIC);
    assert_eq!(beta.record_key, GOLDEN_BETA_RECORD_KEY);
    assert_eq!(gamma.semantic_sha256, GOLDEN_GAMMA_SEMANTIC);
    assert_eq!(gamma.record_key, GOLDEN_GAMMA_RECORD_KEY);
}

#[test]
fn export_fails_when_a_creator_has_no_relationship_row() {
    let (directory, repository) = repository();
    insert_golden_fixture(&directory.path().join("crm.db"), false);
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch(
            "INSERT INTO creators (id, display_name, created_at, updated_at)
             VALUES ('creator-orphan', 'Orphan', 40, 40);",
        )
        .unwrap();
    assert_eq!(
        repository.export_creator_bundle().unwrap_err(),
        "creator creator-orphan has no relationship row"
    );
}

/// Inserts `count` Creators that each carry two linked accounts with a
/// latest profile observation and one contact route with two observations,
/// so the export touches every batch query with a body proportional to
/// `count`.
fn insert_bulk_fixture(path: &std::path::Path, count: usize) {
    let mut connection = Connection::open(path).unwrap();
    let transaction = connection.transaction().unwrap();
    transaction
        .execute_batch(
            "INSERT INTO observation_sources
               (id, kind, platform, source_url, external_ref, account_ref_id, metadata_json,
                observed_at)
             VALUES
               ('bulk-source-initial', 'initial_platform_account', 'x', NULL, NULL, NULL, '{}', 1),
               ('bulk-source-bio', 'profile_bio', 'x', NULL, NULL, NULL, '{}', 1),
               ('bulk-source-manual', 'manual_entry', NULL, NULL, NULL, NULL, '{}', 1);",
        )
        .unwrap();
    for index in 0..count {
        let id = format!("bulk-creator-{index:05}");
        transaction
            .execute(
                "INSERT INTO creators
                   (id, display_name, created_at, updated_at, score, follower_counts_json,
                    pinned_at)
                 VALUES (?1, ?2, ?3, ?3, 42, '{\"x\":1234}', ?3)",
                rusqlite::params![id, format!("Bulk Creator {index}"), index as i64 + 1],
            )
            .unwrap();
        transaction
            .execute(
                "INSERT INTO creator_relationships
                   (creator_id, stage, last_touch_at, created_at, updated_at)
                 VALUES (?1, 'engaged', NULL, 1, 1)",
                [&id],
            )
            .unwrap();
        for (platform, target_kind) in [("x", "user"), ("instagram", "creator")] {
            let account_id = format!("{id}-{platform}");
            transaction
                .execute(
                    "INSERT INTO platform_accounts
                       (id, platform, target_kind, external_id, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?1, ?4, ?4)",
                    rusqlite::params![account_id, platform, target_kind, index as i64 + 1],
                )
                .unwrap();
            transaction
                .execute(
                    "INSERT INTO creator_account_links
                       (id, creator_id, platform_account_id, state, confidence, created_at,
                        updated_at)
                     VALUES (?1, ?2, ?3, 'confirmed', 'exact', 1, 1)",
                    rusqlite::params![format!("link-{account_id}"), id, account_id],
                )
                .unwrap();
            transaction
                .execute(
                    "INSERT INTO platform_account_observations
                       (id, platform_account_id, source_id, handle, display_name, profile_url,
                        public_json, observed_at)
                     VALUES (?1, ?2, 'bulk-source-bio', ?3, ?3, ?4,
                             '{\"profile_image_url\":\"https://cdn.example/bulk.png\"}', 1)",
                    rusqlite::params![
                        format!("obs-{account_id}"),
                        account_id,
                        format!("handle_{index}"),
                        format!("https://{platform}.example/{index}")
                    ],
                )
                .unwrap();
        }
        let locator_id = format!("locator-{id}");
        transaction
            .execute(
                "INSERT INTO contact_locators
                   (id, channel, kind, normalized_value, display_value, created_at, updated_at)
                 VALUES (?1, 'email', 'email_address', ?2, ?2, 1, 1)",
                rusqlite::params![locator_id, format!("{index}@bulk.example")],
            )
            .unwrap();
        transaction
            .execute(
                "INSERT INTO creator_contact_routes
                   (id, creator_id, locator_id, confidence, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'exact', 1, 1)",
                rusqlite::params![format!("route-{id}"), id, locator_id],
            )
            .unwrap();
        transaction
            .execute(
                "INSERT INTO locator_observations
                   (id, locator_id, source_id, creator_id, platform_account_id, observed_value,
                    observed_at)
                 VALUES (?1, ?3, 'bulk-source-manual', ?4, NULL, ?5, 2),
                        (?2, ?3, 'bulk-source-initial', NULL, ?6, ?5, 1)",
                rusqlite::params![
                    format!("lobs-manual-{id}"),
                    format!("lobs-initial-{id}"),
                    locator_id,
                    id,
                    format!("{index}@bulk.example"),
                    format!("{id}-x")
                ],
            )
            .unwrap();
    }
    transaction.commit().unwrap();
}

static EXPORT_TRACE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static TRACED_STATEMENTS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
static EXPORT_TRACE_PAUSE: std::sync::Mutex<
    Option<(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>,
> = std::sync::Mutex::new(None);

fn record_traced_statement(event: rusqlite::trace::TraceEvent<'_>) {
    if let rusqlite::trace::TraceEvent::Stmt(statement, sql) = event {
        TRACED_STATEMENTS
            .lock()
            .unwrap()
            .push(statement.expanded_sql().unwrap_or_else(|| sql.to_owned()));
    }
}

fn pause_export_after_snapshot(event: rusqlite::trace::TraceEvent<'_>) {
    let rusqlite::trace::TraceEvent::Stmt(_, sql) = event else {
        return;
    };
    if !sql.contains("SELECT DISTINCT account.id") {
        return;
    }
    let Some((entered, release)) = EXPORT_TRACE_PAUSE.lock().unwrap().take() else {
        return;
    };
    entered.send(()).unwrap();
    release.recv().unwrap();
}

fn arm_read_traces(repository: &SqliteRepository, callback: fn(rusqlite::trace::TraceEvent<'_>)) {
    let first = repository.read_connection().unwrap();
    let second = repository.read_connection().unwrap();
    let events = rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT;
    first.trace_v2(events, Some(callback));
    second.trace_v2(events, Some(callback));
}

fn clear_read_traces(repository: &SqliteRepository) {
    let first = repository.read_connection().unwrap();
    let second = repository.read_connection().unwrap();
    let events = rusqlite::trace::TraceEventCodes::empty();
    first.trace_v2(events, None);
    second.trace_v2(events, None);
}

fn traced_export_statements(count: usize) -> Vec<String> {
    let (directory, repository) = repository();
    insert_bulk_fixture(&directory.path().join("crm.db"), count);
    arm_read_traces(&repository, record_traced_statement);
    TRACED_STATEMENTS.lock().unwrap().clear();
    let bundle = repository.export_creator_bundle().unwrap();
    assert_eq!(bundle.creator_count as usize, count);
    let statements = std::mem::take(&mut *TRACED_STATEMENTS.lock().unwrap());
    clear_read_traces(&repository);
    TRACED_STATEMENTS.lock().unwrap().clear();
    assert!(!statements.is_empty(), "export trace was not attached");
    assert!(
        statements
            .iter()
            .all(|statement| !statement.contains("creator_id = 'bulk-creator")),
        "export must not issue per-Creator statements: {statements:#?}"
    );
    statements
}

fn traced_workbook_export_statements(count: usize) -> Vec<String> {
    let (directory, repository) = repository();
    insert_bulk_fixture(&directory.path().join("crm.db"), count);
    arm_read_traces(&repository, record_traced_statement);
    TRACED_STATEMENTS.lock().unwrap().clear();
    let workbook = repository.export_creator_workbook().unwrap();
    assert_eq!(workbook.bundle.creator_count as usize, count);
    let statements = std::mem::take(&mut *TRACED_STATEMENTS.lock().unwrap());
    clear_read_traces(&repository);
    TRACED_STATEMENTS.lock().unwrap().clear();
    statements
}

#[test]
fn workbook_export_bundle_and_annotations_share_one_fixed_query_snapshot() {
    let _trace_guard = EXPORT_TRACE_TEST_LOCK.lock().unwrap();
    let fifty = traced_workbook_export_statements(50);
    let two_hundred = traced_workbook_export_statements(200);
    assert_eq!(fifty, two_hundred);
    assert!(
        fifty.len() <= 11,
        "workbook export ran {} statements: {fifty:#?}",
        fifty.len()
    );
    let normalized = fifty
        .iter()
        .map(|statement| statement.trim_start().to_ascii_uppercase())
        .collect::<Vec<_>>();
    assert_eq!(
        normalized
            .iter()
            .filter(|statement| *statement == "BEGIN DEFERRED")
            .count(),
        1
    );
    assert_eq!(
        normalized
            .iter()
            .filter(|statement| *statement == "COMMIT")
            .count(),
        1
    );
}

#[test]
fn export_sql_phase_is_one_bounded_deferred_read_transaction() {
    let _trace_guard = EXPORT_TRACE_TEST_LOCK.lock().unwrap();
    let fifty = traced_export_statements(50);
    let two_hundred = traced_export_statements(200);
    assert_eq!(fifty, two_hundred);
    assert!(
        fifty.len() <= 8,
        "export ran {} statements: {fifty:#?}",
        fifty.len()
    );
    let normalized = fifty
        .iter()
        .map(|statement| statement.trim_start().to_ascii_uppercase())
        .collect::<Vec<_>>();
    assert_eq!(
        normalized
            .iter()
            .filter(|statement| statement.as_str() == "SELECT 1;")
            .count(),
        1,
        "r2d2 should validate exactly the one exported read lease"
    );
    assert_eq!(
        normalized
            .iter()
            .filter(|statement| statement.starts_with("BEGIN DEFERRED"))
            .count(),
        1
    );
    assert_eq!(
        normalized
            .iter()
            .filter(|statement| {
                statement.starts_with("SELECT") && statement.as_str() != "SELECT 1;"
            })
            .count(),
        5
    );
    assert_eq!(
        normalized
            .iter()
            .filter(|statement| statement.starts_with("COMMIT"))
            .count(),
        1
    );
    let begin = normalized
        .iter()
        .position(|statement| statement.starts_with("BEGIN DEFERRED"))
        .unwrap();
    let commit = normalized
        .iter()
        .position(|statement| statement.starts_with("COMMIT"))
        .unwrap();
    let business_selects = normalized
        .iter()
        .enumerate()
        .filter(|(_, statement)| {
            statement.starts_with("SELECT") && statement.as_str() != "SELECT 1;"
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert!(
        business_selects
            .iter()
            .all(|select| begin < *select && *select < commit),
        "all export queries must stay inside one transaction: {fifty:#?}"
    );
}

#[test]
fn export_uses_one_read_lease_and_allows_wal_writer_commit_during_snapshot() {
    use std::{sync::mpsc, time::Duration};

    let _trace_guard = EXPORT_TRACE_TEST_LOCK.lock().unwrap();
    let (directory, repository) = repository();
    let fixture_count = 50;
    insert_bulk_fixture(&directory.path().join("crm.db"), fixture_count);
    arm_read_traces(&repository, pause_export_after_snapshot);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    *EXPORT_TRACE_PAUSE.lock().unwrap() = Some((entered_tx, release_rx));

    let export_repository = repository.clone();
    let export = std::thread::spawn(move || export_repository.export_creator_bundle());
    entered_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("export did not establish its read snapshot");
    assert_eq!(repository.read_pool().state().idle_connections, 1);

    let mut second_reader = repository.read_connection().unwrap();
    let second_snapshot = second_reader
        .transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)
        .unwrap();
    assert_eq!(
        second_snapshot
            .query_row("SELECT COUNT(*) FROM creators", [], |row| {
                row.get::<_, usize>(0)
            })
            .unwrap(),
        fixture_count
    );
    assert_eq!(repository.read_pool().state().idle_connections, 0);

    let writer_repository = repository.clone();
    let (writer_started_tx, writer_started_rx) = mpsc::channel();
    let (writer_tx, writer_rx) = mpsc::channel();
    let writer = std::thread::spawn(move || {
        writer_started_tx.send(()).unwrap();
        writer_tx
            .send((|| {
                let mut connection = writer_repository.lock()?;
                let transaction = connection
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                transaction
                    .execute_batch(
                        "INSERT INTO creators
                           (id, display_name, created_at, updated_at, follower_counts_json)
                         VALUES
                           ('committed-during-export', 'Committed during export', 2, 2, '{}');
                         INSERT INTO creator_relationships
                           (creator_id, stage, last_touch_at, created_at, updated_at)
                         VALUES
                           ('committed-during-export', 'prospect', NULL, 2, 2);
                         INSERT INTO platform_account_observations
                           (id, platform_account_id, source_id, handle, display_name, profile_url,
                            public_json, observed_at)
                         VALUES
                           ('committed-profile-during-export', 'bulk-creator-00000-x',
                            'bulk-source-bio', 'committed_during_export',
                            'Committed during export',
                            'https://x.example/committed_during_export', '{}', 2);",
                    )
                    .map_err(|error| error.to_string())?;
                transaction.commit().map_err(|error| error.to_string())
            })())
            .unwrap();
    });
    writer_started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("writer thread did not start");
    writer_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("WAL writer waited for active read snapshots")
        .unwrap();
    writer.join().unwrap();

    release_tx.send(()).unwrap();
    let bundle = export.join().unwrap().unwrap();
    assert_eq!(bundle.creator_count as usize, fixture_count);
    assert!(
        bundle
            .creators
            .iter()
            .all(|creator| creator.display_name != "Committed during export")
    );
    let original = bundle
        .creators
        .iter()
        .find(|creator| creator.display_name == "Bulk Creator 0")
        .unwrap();
    assert!(original.accounts.iter().all(|account| {
        account
            .latest_profile
            .as_ref()
            .and_then(|profile| profile.handle.as_deref())
            != Some("committed_during_export")
    }));

    second_snapshot.commit().unwrap();
    drop(second_reader);
    clear_read_traces(&repository);
    let next_bundle = repository.export_creator_bundle().unwrap();
    assert_eq!(next_bundle.creator_count as usize, fixture_count + 1);
    let refreshed = next_bundle
        .creators
        .iter()
        .find(|creator| creator.display_name == "Bulk Creator 0")
        .unwrap();
    assert!(refreshed.accounts.iter().any(|account| {
        account
            .latest_profile
            .as_ref()
            .and_then(|profile| profile.handle.as_deref())
            == Some("committed_during_export")
    }));
}

#[test]
fn export_returns_read_lease_before_bundle_encoding() {
    let (directory, repository) = repository();
    insert_bulk_fixture(&directory.path().join("crm.db"), 50);

    let bundle = super::super::creator_transfer::export_creator_bundle_with_test_builder(
        &repository,
        |creators| {
            assert_eq!(repository.read_pool().state().idle_connections, 2);
            let first = repository.read_connection().unwrap();
            let second = repository.read_connection().unwrap();
            assert_eq!(repository.read_pool().state().idle_connections, 0);
            drop((first, second));
            crate::creator_transfer::build_creator_bundle(creators)
        },
    )
    .unwrap();
    assert_eq!(bundle.creator_count, 50);
}
