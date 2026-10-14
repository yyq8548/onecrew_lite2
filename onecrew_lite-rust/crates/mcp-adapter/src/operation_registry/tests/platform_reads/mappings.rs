use super::*;

#[test]
fn explicit_direct_policy_reaches_each_real_search_toolkits_connection_admission() {
    let (_directory, _platform, registry) = production_registry();
    for (operation, connection, mut arguments) in search_calls() {
        arguments["search_plan_policy"] = json!("user_requested_direct");
        let failure = execute_registered(&registry, operation, &connection, arguments).unwrap_err();
        let projected = project_execute_failure(&failure);
        assert_eq!(
            projected.code.as_str(),
            "connection_not_found_or_mismatch",
            "{operation}"
        );
        assert_eq!(
            projected.phase,
            ExecuteFailurePhase::AfterInvocation,
            "{operation}"
        );
        assert_eq!(
            projected.effect,
            ExecuteEffectState::KnownNoEffect,
            "{operation}"
        );
        assert!(!failure.effect_may_have_occurred, "{operation}");
    }
}

#[test]
fn user_posts_rejects_removed_count_before_invocation() {
    let (_directory, _platform, registry) = production_registry();
    let connection = ConnectionRef::OperatorAccount {
        toolkit: OperatorToolkitId::X,
        operator_account_id: UNKNOWN_CONNECTION.into(),
    };
    let failure = execute_registered(
        &registry,
        "x.user_posts.list",
        &connection,
        json!({"external_user_id":"subject-1", "count":1}),
    )
    .unwrap_err();
    assert_eq!(failure.phase, ExecuteFailurePhase::BeforeInvocation);
    assert_eq!(failure.kind, ExecuteFailureKind::InputSchemaRejected);
}

#[test]
fn each_search_enforces_its_actual_count_limit_and_nonblank_query_before_invocation() {
    let (_directory, _platform, registry) = production_registry();
    for (operation, connection, mut arguments) in search_calls() {
        arguments["search_plan_policy"] = json!("user_requested_direct");
        let maximum = if operation.starts_with("youtube.") {
            50
        } else {
            100
        };
        for count in [0, maximum + 1] {
            let mut invalid = arguments.clone();
            invalid["count"] = json!(count);
            let failure =
                execute_registered(&registry, operation, &connection, invalid).unwrap_err();
            assert_eq!(
                failure.phase,
                ExecuteFailurePhase::BeforeInvocation,
                "{operation}/{count}"
            );
            assert_eq!(
                failure.kind,
                ExecuteFailureKind::InputSchemaRejected,
                "{operation}/{count}"
            );
        }
        if operation != "x.posts.search.advanced" {
            arguments["query"] = json!("  ");
            let failure =
                execute_registered(&registry, operation, &connection, arguments).unwrap_err();
            assert_eq!(
                failure.phase,
                ExecuteFailurePhase::BeforeInvocation,
                "{operation}"
            );
            assert_eq!(
                failure.kind,
                ExecuteFailureKind::TypedDecodeRejected,
                "{operation}"
            );
        }
    }
}

#[test]
fn advanced_x_requires_a_positive_selector_and_an_ordered_date_range() {
    let (_directory, _platform, registry) = production_registry();
    let connection = ConnectionRef::OperatorAccount {
        toolkit: OperatorToolkitId::X,
        operator_account_id: UNKNOWN_CONNECTION.into(),
    };
    for arguments in [
        json!({"all_words":[], "search_plan_policy":"user_requested_direct"}),
        json!({"all_words":["rust"], "since":"2026-09-14", "until":"2026-09-01", "search_plan_policy":"user_requested_direct"}),
    ] {
        let failure =
            execute_registered(&registry, "x.posts.search.advanced", &connection, arguments)
                .unwrap_err();
        assert_eq!(failure.phase, ExecuteFailurePhase::BeforeInvocation);
        assert_eq!(failure.kind, ExecuteFailureKind::TypedDecodeRejected);
    }
}

/// Frozen wire JSON of every platform read's output: each provider object as
/// its model serializes, inside the operation's output wrapper.
fn frozen_outputs() -> Vec<(&'static str, Value)> {
    let post = json!({
        "id": "9007199254740993",
        "text": "A public reply",
        "author": {
            "id": "9007199254740995",
            "name": "",
            "username": "creator",
            "profile_image_url": "",
            "verified": false,
            "blue_verified": false,
            "viewer_follows": null,
            "viewer_followed_by": null,
            "can_dm": null
        },
        "metrics": {
            "likes": 0,
            "retweets": 0,
            "replies": 0,
            "quotes": 0,
            "views": 0,
            "bookmarks": 0
        },
        "created_at": "",
        "language": "",
        "media": [],
        "urls": [],
        "promoted": false,
        "article": null,
        "in_reply_to_status_id": null,
        "conversation_id": null
    });
    let page = json!({"items": [post], "next_cursor": "provider+/cursor=="});
    let youtube_page = |video_id: Value| {
        json!({
            "items": [{
                "channel_id": "UC_x5XG1OV2P6uZZ5FSM9Ttw",
                "channel_url": "https://www.youtube.com/channel/UC_x5XG1OV2P6uZZ5FSM9Ttw",
                "matched": {
                    "title": "Rust tutorial",
                    "description": "Video search result",
                    "video_id": video_id,
                    "published_at": null
                }
            }],
            "channels": {
                "UC_x5XG1OV2P6uZZ5FSM9Ttw": {
                    "title": "Rust tutorials",
                    "thumbnail_url": null,
                    "description": "Weekly Rust lessons",
                    "custom_url": "@rusttutorials",
                    "country": "TR",
                    "subscriber_count": 42,
                    "view_count": null,
                    "video_count": null
                }
            },
            "next_page_token": "provider-youtube-token"
        })
    };
    // Frozen public Telegram wire shapes; the provider owner's tests prove the
    // typed models omit access hashes, phones, and session material.
    let community = json!({
        "external_id": "9007199254740993",
        "title": "Rust community",
        "username": "rust",
        "kind": "supergroup"
    });
    vec![
        (
            "x.profile.get",
            json!({"profile": {
                "id": "9007199254740995",
                "name": "Creator",
                "username": "creator",
                "bio": "Rust tutorials",
                "location": "",
                "profile_links": [{
                    "display_text": "example.com",
                    "target_url": "https://example.com/"
                }],
                "followers": 42,
                "following": 7,
                "posts": 120,
                "likes": 0,
                "verified": false,
                "blue_verified": true,
                "profile_image_url": "",
                "viewer_follows": null,
                "viewer_followed_by": null,
                "can_dm": null
            }}),
        ),
        ("x.user_posts.list", json!({"page": page.clone()})),
        ("x.post_thread.get", page.clone()),
        (
            "x.posts.search.advanced",
            json!({"effective_query": "rust", "page": page}),
        ),
        (
            "instagram.profile.get",
            json!({"profile": {
                "id": "9007199254740993",
                "username": "creator",
                "full_name": "Creator",
                "is_private": false,
                "is_verified": true,
                "follower_count": 42,
                "biography": "Rust tutorials",
                "external_url": "https://example.com/",
                "bio_links": ["https://example.com/"]
            }}),
        ),
        (
            "instagram.users.search",
            json!([{
                "id": "9007199254740993",
                "username": "creator",
                "full_name": "Creator",
                "is_verified": true,
                "follower_count": 42
            }]),
        ),
        (
            "youtube.channel.get",
            json!({"channel": {
                "channel_id": "UC_x5XG1OV2P6uZZ5FSM9Ttw",
                "channel_url": "https://www.youtube.com/channel/UC_x5XG1OV2P6uZZ5FSM9Ttw",
                "title": "Rust tutorials",
                "thumbnail_url": null,
                "description": "Weekly Rust lessons",
                "description_source_url": "https://www.youtube.com/channel/UC_x5XG1OV2P6uZZ5FSM9Ttw/about",
                "custom_url": "@rusttutorials",
                "country": "TR",
                "subscriber_count": 42,
                "view_count": null,
                "video_count": null,
                "matched": null,
                "recent_videos": [{
                    "video_id": "video-1",
                    "title": "Rust tutorial",
                    "description": "Full description https://sponsor.example.com/offer",
                    "published_at": null
                }],
                "public_comments": [],
                "about_links": [{
                    "target_url": "https://example.com/",
                    "source_url": "https://www.youtube.com/channel/UC_x5XG1OV2P6uZZ5FSM9Ttw/about"
                }],
                "hidden_business_email": false,
                "unavailable_sections": ["public_comments"]
            }}),
        ),
        ("youtube.channels.search", youtube_page(Value::Null)),
        ("youtube.videos.search", youtube_page(json!("video-1"))),
        (
            "telegram.communities.search",
            json!([{
                "id": "9007199254740993",
                "title": "Rust community",
                "username": "rust",
                "kind": "supergroup",
                "participant_count": 42,
                "verified": false,
                "scam": false,
            }]),
        ),
        ("telegram.community.get", community.clone()),
        (
            "telegram.community.administrators.list",
            json!({
                "community": community,
                "administrators": [
                    {
                        "role": "owner",
                        "user": {
                            "id": "9007199254740995",
                            "username": "ferris",
                            "first_name": "Ferris",
                            "last_name": null,
                            "is_bot": false,
                            "verified": false
                        }
                    },
                    {
                        "role": "administrator",
                        "user": {
                            "id": "9007199254740997",
                            "username": null,
                            "first_name": "Crab",
                            "last_name": "Admin",
                            "is_bot": false,
                            "verified": true
                        }
                    }
                ],
                "page_limit": 200
            }),
        ),
    ]
}

#[test]
fn every_platform_read_output_matches_its_registered_output_contract() {
    let (_directory, _platform, registry) = production_registry();
    let outputs = frozen_outputs();
    assert_eq!(
        outputs
            .iter()
            .map(|(operation, _)| *operation)
            .collect::<BTreeSet<_>>()
            .len(),
        12,
        "one frozen output for each platform read"
    );
    for (operation, value) in outputs {
        let entry = registry_operation(&registry, operation);
        assert_eq!(entry.effect, EffectClass::ReadOpenWorld, "{operation}");
        assert_eq!(entry.approval, ApprovalClass::None, "{operation}");
        assert!(entry.output.validate(&value), "{operation}: {value}");
        let array = matches!(
            operation,
            "instagram.users.search" | "telegram.communities.search"
        );
        assert_eq!(
            entry.output.document()["type"],
            if array { "array" } else { "object" },
            "{operation}"
        );
        let wrapper = match operation {
            "x.profile.get" | "instagram.profile.get" => Some("profile"),
            "x.user_posts.list" => Some("page"),
            "youtube.channel.get" => Some("channel"),
            _ => None,
        };
        if let Some(key) = wrapper {
            assert_eq!(
                value
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                vec![key],
                "{operation}"
            );
            assert!(
                !entry.output.validate(&value[key]),
                "{operation}: the {key} wrapper is required"
            );
        }
        let wire = serde_json::to_string(&value).unwrap();
        for retired in [
            "candidate_id",
            "discovery_id",
            "identity_resolution",
            "allowed_actions",
            "access_hash",
            "phone",
        ] {
            assert!(!wire.contains(retired), "{operation}: {retired}");
        }
        match operation {
            "x.posts.search.advanced" => {
                assert_eq!(value["page"]["items"][0]["id"], "9007199254740993");
                assert_eq!(value["page"]["next_cursor"], "provider+/cursor==");
            }
            "x.post_thread.get" => {
                assert_eq!(value["items"][0]["id"], "9007199254740993");
                assert_eq!(value["next_cursor"], "provider+/cursor==");
                let connection = ConnectionRef::OperatorAccount {
                    toolkit: OperatorToolkitId::X,
                    operator_account_id: UNKNOWN_CONNECTION.into(),
                };
                let failure = execute_registered(
                    &registry,
                    operation,
                    &connection,
                    json!({"content_id":"9007199254740993", "cursor":value["next_cursor"]}),
                )
                .unwrap_err();
                assert_eq!(
                    project_execute_failure(&failure).code.as_str(),
                    "connection_not_found_or_mismatch"
                );
            }
            "x.user_posts.list" => {
                assert_eq!(value["page"]["next_cursor"], "provider+/cursor==");
            }
            "x.profile.get" => {
                assert_eq!(value["profile"]["followers"], 42);
            }
            "instagram.profile.get" => {
                assert_eq!(value["profile"]["follower_count"], 42);
            }
            "youtube.channel.get" => {
                assert_eq!(value["channel"]["channel_id"], "UC_x5XG1OV2P6uZZ5FSM9Ttw");
                assert!(value["channel"].get("contacts").is_none());
            }
            "youtube.channels.search" | "youtube.videos.search" => {
                assert_eq!(value["items"][0]["channel_id"], "UC_x5XG1OV2P6uZZ5FSM9Ttw");
                assert_eq!(
                    value["items"][0]["matched"]["video_id"],
                    if operation == "youtube.videos.search" {
                        json!("video-1")
                    } else {
                        Value::Null
                    }
                );
                assert_eq!(value["next_page_token"], "provider-youtube-token");
            }
            "telegram.community.get" => {
                assert_eq!(value["external_id"], "9007199254740993");
            }
            "telegram.community.administrators.list" => {
                assert_eq!(value["community"]["external_id"], "9007199254740993");
                assert_eq!(value["page_limit"], 200);
            }
            "instagram.users.search" | "telegram.communities.search" => {
                assert_eq!(value[0]["id"], "9007199254740993");
            }
            other => panic!("unexpected platform read {other}"),
        }
    }
}

#[test]
fn instagram_and_telegram_reject_a_synthetic_cursor() {
    let (_directory, _platform, registry) = production_registry();
    for (operation, connection, mut arguments) in
        search_calls().into_iter().filter(|(operation, _, _)| {
            matches!(
                *operation,
                "instagram.users.search" | "telegram.communities.search"
            )
        })
    {
        arguments["search_plan_policy"] = json!("user_requested_direct");
        arguments["cursor"] = json!("fabricated-page");
        let failure = execute_registered(&registry, operation, &connection, arguments).unwrap_err();
        assert_eq!(
            failure.phase,
            ExecuteFailurePhase::BeforeInvocation,
            "{operation}"
        );
        assert_eq!(
            failure.kind,
            ExecuteFailureKind::InputSchemaRejected,
            "{operation}"
        );
    }
}
