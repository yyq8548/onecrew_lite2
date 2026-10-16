use super::*;
use onecrew_local_runtime::ToolkitId;

#[test]
fn full_and_paraphrased_english_use_cases_retrieve_the_expected_playbooks() {
    let registry = production_registry();
    for (query, toolkit, expected) in [
        (
            "Find relevant creator accounts on X and inspect a selected profile",
            ToolkitId::X,
            "playbook.x.creator-discovery",
        ),
        (
            "Finding relevant Instagram creators and inspecting their profiles",
            ToolkitId::Instagram,
            "playbook.instagram.creator-discovery",
        ),
        (
            "Search YouTube channels and inspect the selected channel",
            ToolkitId::Youtube,
            "playbook.youtube.channel-discovery",
        ),
    ] {
        let result = search(&registry, query, vec![toolkit], Vec::new());
        assert_eq!(result.playbooks.len(), 1, "{query}");
        assert_eq!(result.playbooks[0].id.as_str(), expected, "{query}");
        assert!(result.operations.len() <= 6, "{query}");
    }
}

#[test]
fn real_creator_request_normalizes_to_capability_search_before_execution_values() {
    let registry = production_registry();

    // Conversation 01a04641 supplied "five" and "beauty" as search terms.
    // The agent-facing contract instead normalizes the intent while retaining
    // those values for x.posts.search.advanced arguments.
    let normalized = search(
        &registry,
        "Find X creator accounts and inspect public profiles",
        vec![ToolkitId::X],
        Vec::new(),
    );
    assert_eq!(
        normalized.playbooks[0].id.as_str(),
        "playbook.x.creator-discovery"
    );
    let operation_ids = normalized
        .operations
        .iter()
        .map(|operation| operation.id.as_str())
        .collect::<Vec<_>>();
    assert!(operation_ids.contains(&"x.posts.search.advanced"));
    assert!(operation_ids.contains(&"x.profile.get"));

    // P6 search reads native provider pages without creating Creator snapshots.
    // An explicit read filter must therefore retain the same search capability.
    let read_only = search(
        &registry,
        "Search X creator accounts",
        vec![ToolkitId::X],
        vec![EffectClass::ReadOpenWorld],
    );
    assert!(
        read_only
            .operations
            .iter()
            .any(|operation| operation.id.as_str() == "x.posts.search.advanced")
    );
    assert!(read_only.operations.iter().all(|operation| {
        operation.toolkit == ToolkitId::X && operation.effects == vec![EffectClass::ReadOpenWorld]
    }));
    let corrected = search(
        &registry,
        "Search X creator accounts",
        vec![ToolkitId::X],
        Vec::new(),
    );
    assert!(
        corrected
            .operations
            .iter()
            .any(|operation| operation.id.as_str() == "x.posts.search.advanced")
    );
}

#[test]
fn explicit_toolkit_filter_attaches_one_guide_without_displacing_ranked_results() {
    let registry = production_registry();
    let result = search(
        &registry,
        "Find creator accounts and inspect profiles",
        vec![ToolkitId::X],
        Vec::new(),
    );
    assert!(!result.operations.is_empty());
    assert_eq!(result.playbooks.len(), 1);
    assert_eq!(result.toolkit_guides.len(), 1);
    assert_eq!(result.toolkit_guides[0].id.as_str(), "toolkit.x");
    assert_eq!(result.toolkit_guides[0].toolkit, ToolkitId::X);
    assert!(!result.toolkit_guides[0].capability_categories.is_empty());

    let unfiltered = search(
        &registry,
        "Find creator accounts and inspect profiles",
        Vec::new(),
        Vec::new(),
    );
    assert!(unfiltered.toolkit_guides.is_empty());

    let cross_toolkit = search(
        &registry,
        "Find creator accounts and save candidate",
        vec![ToolkitId::X, ToolkitId::OneCrew],
        Vec::new(),
    );
    assert_eq!(cross_toolkit.toolkit_guides.len(), 2);
    assert_eq!(
        cross_toolkit
            .toolkit_guides
            .iter()
            .map(|guide| guide.toolkit)
            .collect::<std::collections::BTreeSet<_>>(),
        [ToolkitId::OneCrew, ToolkitId::X].into_iter().collect()
    );
}

#[test]
fn fixed_playbook_eval_covers_the_complete_registered_use_case_set() {
    struct Case {
        query: &'static str,
        toolkits: &'static [ToolkitId],
        expected: &'static str,
    }
    let cases = [
        Case {
            query: "Search X posts with advanced filters, inspect an author profile, and review recent posts",
            toolkits: &[ToolkitId::X],
            expected: "playbook.x.creator-discovery",
        },
        Case {
            query: "Discover Instagram creator accounts and inspect one profile",
            toolkits: &[ToolkitId::Instagram],
            expected: "playbook.instagram.creator-discovery",
        },
        Case {
            query: "Find Telegram communities about a topic",
            toolkits: &[ToolkitId::Telegram],
            expected: "playbook.telegram.community-discovery",
        },
        Case {
            query: "Discover YouTube channels and inspect one channel",
            toolkits: &[ToolkitId::Youtube],
            expected: "playbook.youtube.channel-discovery",
        },
        Case {
            query: "Locate an X post and read its conversation thread",
            toolkits: &[ToolkitId::X],
            expected: "playbook.x.thread-read",
        },
        Case {
            query: "Prepare a Creator save for one selected platform target, review the contact routes and confirm",
            toolkits: &[ToolkitId::OneCrew],
            expected: "playbook.onecrew.creator-candidate-save",
        },
        Case {
            query: "Read a creator profile and its link hubs, then confirm selected contact routes",
            toolkits: &[ToolkitId::OneCrew],
            expected: "playbook.onecrew.creator-contact-confirm",
        },
        Case {
            query: "Read a platform profile, update the saved Creator follower count, and verify the Creator record",
            toolkits: &[ToolkitId::OneCrew],
            expected: "playbook.onecrew.creator-profile-update",
        },
        Case {
            query: "Save several discovery candidates through one batch payload, confirm the selected rows, and read back the saved creators",
            toolkits: &[ToolkitId::OneCrew],
            expected: "playbook.onecrew.creator-candidate-batch-save",
        },
        Case {
            query: "Create several manual creators from one confirmed batch and verify the records",
            toolkits: &[ToolkitId::OneCrew],
            expected: "playbook.onecrew.creator-manual-batch-create",
        },
        Case {
            query: "Save an email draft, send the confirmed revision, and verify its thread",
            toolkits: &[ToolkitId::Email],
            expected: "playbook.email.draft-and-send",
        },
        Case {
            query: "Sync inbound email, inspect threads, and record a Creator triage verdict",
            toolkits: &[ToolkitId::Email, ToolkitId::OneCrew],
            expected: "playbook.email.inbox-triage",
        },
        Case {
            query: "Resolve a Creator contact target, prepare a direct message, send, and read status",
            toolkits: &[ToolkitId::OneCrew],
            expected: "playbook.onecrew.direct-message",
        },
        Case {
            query: "Search X creators, inspect profiles, and save one candidate as a Creator",
            toolkits: &[ToolkitId::X, ToolkitId::OneCrew],
            expected: "playbook.recipe.find-x-creators",
        },
        Case {
            query: "Search Instagram creators, inspect profiles, and save one candidate as a Creator",
            toolkits: &[ToolkitId::Instagram, ToolkitId::OneCrew],
            expected: "playbook.recipe.find-instagram-creators",
        },
        Case {
            query: "Search YouTube channels, inspect one, and save the candidate as a Creator",
            toolkits: &[ToolkitId::Youtube, ToolkitId::OneCrew],
            expected: "playbook.recipe.find-youtube-creators",
        },
        Case {
            query: "Find a Telegram community, discover administrators, and save one as a Creator",
            toolkits: &[ToolkitId::Telegram, ToolkitId::OneCrew],
            expected: "playbook.recipe.find-telegram-administrators",
        },
        Case {
            query: "Resolve the best Creator contact route, draft an email, send, and verify the thread",
            toolkits: &[ToolkitId::OneCrew, ToolkitId::Email],
            expected: "playbook.recipe.email-best-route",
        },
        Case {
            query: "Verify an X connection, resolve the Creator route, and send a confirmed message",
            toolkits: &[ToolkitId::X, ToolkitId::OneCrew],
            expected: "playbook.recipe.x-best-route",
        },
        Case {
            query: "Verify an Instagram connection, resolve the Creator route, and send a confirmed message",
            toolkits: &[ToolkitId::Instagram, ToolkitId::OneCrew],
            expected: "playbook.recipe.instagram-best-route",
        },
        Case {
            query: "Sync email, record Creator verdicts, verify triage, and acknowledge resolved reminders",
            toolkits: &[ToolkitId::Email, ToolkitId::OneCrew],
            expected: "playbook.recipe.triage-email-and-onecrew",
        },
        Case {
            query: "Connect an X operator through the attended browser login lifecycle",
            toolkits: &[ToolkitId::X],
            expected: "playbook.x.connection-browser",
        },
        Case {
            query: "Connect an Instagram operator through the attended browser login lifecycle",
            toolkits: &[ToolkitId::Instagram],
            expected: "playbook.instagram.connection-browser",
        },
        Case {
            query: "Connect Telegram with QR login and handle a possible two factor continuation",
            toolkits: &[ToolkitId::Telegram],
            expected: "playbook.telegram.connection-qr",
        },
        Case {
            query: "Connect Telegram by phone, submit OTP and possible 2FA",
            toolkits: &[ToolkitId::Telegram],
            expected: "playbook.telegram.connection-phone",
        },
        Case {
            query: "Connect, inspect, rename, rotate, or delete one YouTube API credential",
            toolkits: &[ToolkitId::Youtube],
            expected: "playbook.youtube.credential-lifecycle",
        },
    ];
    let registry = production_registry();
    let expected_ids = cases
        .iter()
        .map(|case| case.expected.to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(expected_ids.len(), cases.len());
    let registered_ids = registry
        .playbook_ids()
        .into_iter()
        .map(|id| id.as_str().to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(expected_ids, registered_ids);

    for case in cases {
        let result = search(&registry, case.query, case.toolkits.to_vec(), Vec::new());
        assert!((1..=3).contains(&result.playbooks.len()), "{}", case.query);
        assert_eq!(
            result.playbooks[0].id.as_str(),
            case.expected,
            "{}",
            case.query
        );
    }
}

#[test]
fn fixed_standalone_operation_eval_preserves_recall_across_all_toolkits() {
    let cases = [
        (
            "Search X posts by keyword",
            ToolkitId::X,
            "x.posts.search.advanced",
        ),
        (
            "Use typed advanced X post filters for accounts language dates and engagement",
            ToolkitId::X,
            "x.posts.search.advanced",
        ),
        (
            "Read an Instagram user profile",
            ToolkitId::Instagram,
            "instagram.profile.get",
        ),
        (
            "Search Telegram communities by keyword",
            ToolkitId::Telegram,
            "telegram.communities.search",
        ),
        (
            "Read one YouTube channel by channel ID",
            ToolkitId::Youtube,
            "youtube.channel.get",
        ),
        (
            "List creators stored in the Creator store",
            ToolkitId::OneCrew,
            "onecrew.creator.list",
        ),
        (
            "List reusable mail templates",
            ToolkitId::Email,
            "email.template.list",
        ),
        (
            "Read X operator connection",
            ToolkitId::X,
            "x.connection.get",
        ),
        (
            "Get one saved creator",
            ToolkitId::OneCrew,
            "onecrew.creator.get",
        ),
        (
            "Read one canonical mail thread",
            ToolkitId::Email,
            "email.thread.get",
        ),
    ];
    let registry = production_registry();
    for (query, toolkit, expected) in cases {
        let result = search(&registry, query, vec![toolkit], Vec::new());
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
fn fixed_operation_eval_covers_every_registered_operation_at_recall_six() {
    let registry = production_registry();
    assert_eq!(OPERATION_EVAL_CASES.len(), 156);

    let expected_ids = OPERATION_EVAL_CASES
        .iter()
        .map(|case| case.expected_id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(expected_ids.len(), OPERATION_EVAL_CASES.len());
    let registered_ids = registry
        .catalog_summaries(SurfaceId::Full)
        .into_iter()
        .map(|operation| operation.id.as_str().to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        expected_ids
            .iter()
            .map(|id| (*id).to_owned())
            .collect::<std::collections::BTreeSet<_>>(),
        registered_ids
    );

    for case in OPERATION_EVAL_CASES {
        assert!(!case.query.contains('.'), "{}", case.query);
        let result = search(&registry, case.query, vec![case.toolkit], Vec::new());
        assert!(
            result
                .operations
                .iter()
                .any(|operation| operation.id.as_str() == case.expected_id),
            "{}: {} not in Recall@6; got {:?}",
            case.query,
            case.expected_id,
            result
                .operations
                .iter()
                .map(|operation| operation.id.as_str())
                .collect::<Vec<_>>()
        );
        let primary_ids = result
            .operations
            .iter()
            .filter(|operation| operation.match_role == CatalogMatchRole::Primary)
            .map(|operation| operation.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        if let Some(playbook) = result.playbooks.first() {
            assert_eq!(
                primary_ids,
                playbook
                    .primary_operation_ids
                    .iter()
                    .map(|id| id.as_str())
                    .collect::<std::collections::BTreeSet<_>>(),
                "{}",
                case.query
            );
        } else {
            assert_eq!(primary_ids.len(), 1, "{}", case.query);
        }
    }
}

#[test]
fn an_exact_onecrew_operation_id_resolves_to_that_operation() {
    let registry = production_registry();
    for toolkits in [vec![ToolkitId::OneCrew], Vec::new()] {
        let result = search(&registry, "onecrew.creator.get", toolkits, Vec::new());
        assert_eq!(
            result
                .operations
                .iter()
                .map(|operation| operation.id.as_str())
                .collect::<Vec<_>>(),
            ["onecrew.creator.get"]
        );
        assert_eq!(result.operations[0].match_role, CatalogMatchRole::Primary);
        assert!(result.playbooks.is_empty());
    }
    // A dotted id nobody registered is an exact miss, not a lexical query.
    let missing = search(&registry, "onecrew.creator.missing", Vec::new(), Vec::new());
    assert!(missing.operations.is_empty());
    assert!(missing.playbooks.is_empty());
}
