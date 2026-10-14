use super::*;
#[test]
fn conversations_and_relationships_are_owned_by_typed_accounts_not_campaigns() {
    let (_directory, repository) = repository();
    let actor = repository
        .upsert_account_ref(
            AccountRefId::new("x-account-1"),
            Platform::X,
            "profile",
            "@operator",
        )
        .expect("account ref");
    let target_account = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::X,
            target_kind: TargetKind::User,
            external_id: "x-user-1".into(),
        })
        .expect("target account");
    let target = repository
        .upsert_resolved_target(
            &actor.id,
            Platform::X,
            TargetKind::User,
            "x-user-1",
            Some(&target_account.id),
            serde_json::json!({}),
        )
        .expect("resolved target");
    let conversation = repository
        .upsert_conversation(
            &actor.id,
            &target.id,
            ContactChannel::X,
            "thread-1",
            None,
            0,
        )
        .expect("conversation");
    let first = repository
        .append_message(AppendMessageInput {
            conversation_id: &conversation.id,
            external_message_id: Some("message-1"),
            sender_target_id: Some(&target.id),
            direction: "inbound",
            body: "hello",
            status: "received",
            public_data: serde_json::json!({}),
            sent_at: 100,
        })
        .expect("message");
    let repeated = repository
        .append_message(AppendMessageInput {
            conversation_id: &conversation.id,
            external_message_id: Some("message-1"),
            sender_target_id: Some(&target.id),
            direction: "inbound",
            body: "hello",
            status: "received",
            public_data: serde_json::json!({}),
            sent_at: 100,
        })
        .expect("idempotent message");
    assert_eq!(first.id, repeated.id);

    let relationship = repository
        .upsert_account_relationship(
            &actor.id,
            &target_account.id,
            RelationshipKind::Following,
            RelationshipValue::Yes,
            101,
        )
        .expect("relationship");
    assert_eq!(relationship.actor_account_ref_id, actor.id);

    let connection = repository.lock().expect("repository lock");
    let columns = ["conversations", "messages"]
        .into_iter()
        .flat_map(|table| {
            let mut statement = connection
                .prepare(&format!("PRAGMA table_info({table})"))
                .expect("table info");
            statement
                .query_map([], |row| row.get::<_, String>(1))
                .expect("columns")
                .map(|row| row.expect("column"))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert!(!columns.iter().any(|column| column == "lead_id"));
    assert!(!columns.iter().any(|column| column == "campaign_id"));
}

#[test]
fn last_contact_account_uses_the_exact_route_and_latest_real_outbound_sent_message() {
    let (_directory, repository) = repository();
    let creator = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Route owner".into(),
            notes: None,
        })
        .expect("creator");
    let other_creator = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Other owner".into(),
            notes: None,
        })
        .expect("other creator");
    let route_a_locator = repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::X,
            kind: LocatorKind::Handle,
            normalized_value: "route_a".into(),
            display_value: "@route_a".into(),
        })
        .expect("route A locator");
    let route_a = repository
        .link_creator_contact_route(&creator.creator.id, &route_a_locator.id, Confidence::Exact)
        .expect("route A");
    let route_b_locator = repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::X,
            kind: LocatorKind::Handle,
            normalized_value: "route_b".into(),
            display_value: "@route_b".into(),
        })
        .expect("route B locator");
    let route_b = repository
        .link_creator_contact_route(&creator.creator.id, &route_b_locator.id, Confidence::Exact)
        .expect("route B");
    let first_operator = repository
        .upsert_account_ref(
            AccountRefId::new("operator-first"),
            Platform::X,
            "profile",
            "@first",
        )
        .expect("first operator");
    let latest_operator = repository
        .upsert_account_ref(
            AccountRefId::new("operator-latest"),
            Platform::X,
            "profile",
            "@latest",
        )
        .expect("latest operator");
    let first_target = repository
        .upsert_resolved_target(
            &first_operator.id,
            Platform::X,
            TargetKind::User,
            "route-a-target",
            None,
            serde_json::json!({ "handle": "route_a" }),
        )
        .expect("first target");
    let latest_target = repository
        .upsert_resolved_target(
            &latest_operator.id,
            Platform::X,
            TargetKind::User,
            "route-a-target",
            None,
            serde_json::json!({ "handle": "route_a" }),
        )
        .expect("latest target");
    repository
        .record_locator_resolution(&route_a_locator.id, &first_operator.id, &first_target.id)
        .expect("first route resolution");
    repository
        .record_locator_resolution(&route_a_locator.id, &latest_operator.id, &latest_target.id)
        .expect("latest route resolution");
    let first_conversation = repository
        .upsert_conversation(
            &first_operator.id,
            &first_target.id,
            ContactChannel::X,
            "thread-first",
            None,
            0,
        )
        .expect("first conversation");
    repository
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO conversation_contact_routes
               (conversation_id, contact_route_id, created_at) VALUES (?1, ?2, 0)",
            rusqlite::params![first_conversation.id.as_str(), route_a.id.as_str()],
        )
        .expect("first conversation route owner");
    repository
        .append_message(AppendMessageInput {
            conversation_id: &first_conversation.id,
            external_message_id: Some("inbound-only"),
            sender_target_id: Some(&first_target.id),
            direction: "inbound",
            body: "hello",
            status: "received",
            public_data: serde_json::json!({}),
            sent_at: 500,
        })
        .expect("inbound message");
    assert!(
        repository
            .contact_route_last_contact_account(&creator.creator.id, &route_a.id)
            .expect("inbound-only lookup")
            .is_none()
    );

    repository
        .append_message(AppendMessageInput {
            conversation_id: &first_conversation.id,
            external_message_id: Some("first-outbound"),
            sender_target_id: None,
            direction: "outbound",
            body: "first",
            status: "sent",
            public_data: serde_json::json!({}),
            sent_at: 100,
        })
        .expect("first outbound");
    repository
        .append_message(AppendMessageInput {
            conversation_id: &first_conversation.id,
            external_message_id: Some("queued-newer"),
            sender_target_id: None,
            direction: "outbound",
            body: "not actually sent",
            status: "queued",
            public_data: serde_json::json!({}),
            sent_at: 900,
        })
        .expect("queued outbound");
    let latest_conversation = repository
        .upsert_conversation(
            &latest_operator.id,
            &latest_target.id,
            ContactChannel::X,
            "thread-latest",
            None,
            0,
        )
        .expect("latest conversation");
    repository
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO conversation_contact_routes
               (conversation_id, contact_route_id, created_at) VALUES (?1, ?2, 0)",
            rusqlite::params![latest_conversation.id.as_str(), route_a.id.as_str()],
        )
        .expect("latest conversation route owner");
    repository
        .append_message(AppendMessageInput {
            conversation_id: &latest_conversation.id,
            external_message_id: Some("latest-outbound"),
            sender_target_id: None,
            direction: "outbound",
            body: "latest",
            status: "sent",
            public_data: serde_json::json!({}),
            sent_at: 200,
        })
        .expect("latest outbound");

    let route_b_target = repository
        .upsert_resolved_target(
            &latest_operator.id,
            Platform::X,
            TargetKind::User,
            "route-b-target",
            None,
            serde_json::json!({ "handle": "route_b" }),
        )
        .expect("route B target");
    repository
        .record_locator_resolution(&route_b_locator.id, &latest_operator.id, &route_b_target.id)
        .expect("route B resolution");
    let route_b_conversation = repository
        .upsert_conversation(
            &latest_operator.id,
            &route_b_target.id,
            ContactChannel::X,
            "thread-route-b",
            None,
            0,
        )
        .expect("route B conversation");
    repository
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO conversation_contact_routes
               (conversation_id, contact_route_id, created_at) VALUES (?1, ?2, 0)",
            rusqlite::params![route_b_conversation.id.as_str(), route_b.id.as_str()],
        )
        .expect("route B conversation owner");
    repository
        .append_message(AppendMessageInput {
            conversation_id: &route_b_conversation.id,
            external_message_id: Some("route-b-newest"),
            sender_target_id: None,
            direction: "outbound",
            body: "other route",
            status: "sent",
            public_data: serde_json::json!({}),
            sent_at: 300,
        })
        .expect("route B outbound");

    let result = repository
        .contact_route_last_contact_account(&creator.creator.id, &route_a.id)
        .expect("route A lookup")
        .expect("route A last contact");
    assert_eq!(result.contact_route_id, route_a.id);
    assert_eq!(result.account_ref.id, latest_operator.id);
    assert_eq!(result.channel, ContactChannel::X);
    assert_eq!(result.last_contact_at, 200);
    assert!(
        repository
            .contact_route_last_contact_account(&other_creator.creator.id, &route_a.id)
            .expect_err("route ownership must be exact")
            .contains("does not belong")
    );

    repository
        .archive_creator(&creator.creator.id)
        .expect("archive creator");
    assert_eq!(
        repository
            .contact_route_last_contact_account(&creator.creator.id, &route_a.id)
            .expect("archived historical lookup")
            .expect("archived historical contact")
            .last_contact_at,
        200
    );
    assert_eq!(
        repository
            .contact_route_last_contact_account(&creator.creator.id, &route_b.id)
            .expect("route B lookup")
            .expect("route B last contact")
            .last_contact_at,
        300
    );
}
