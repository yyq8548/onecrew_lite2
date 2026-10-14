use super::*;
#[test]
fn target_resolution_commit_is_atomic_across_locator_and_creator_ownership() {
    let (_directory, repository) = repository();
    let first = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "First".into(),
            notes: None,
        })
        .expect("first creator");
    let second = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Second".into(),
            notes: None,
        })
        .expect("second creator");
    let operator = repository
        .upsert_account_ref(
            AccountRefId::new("x-operator-atomic-resolution"),
            Platform::X,
            "profile",
            "@operator",
        )
        .expect("operator");
    let locator = repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::X,
            kind: LocatorKind::Handle,
            normalized_value: "shared-handle".into(),
            display_value: "@shared-handle".into(),
        })
        .expect("locator");
    let first_route = repository
        .link_creator_contact_route(&first.creator.id, &locator.id, Confidence::Exact)
        .unwrap();
    let second_locator = repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::X,
            kind: LocatorKind::Handle,
            normalized_value: "second-alias".into(),
            display_value: "@second-alias".into(),
        })
        .unwrap();
    let second_route = repository
        .link_creator_contact_route(&second.creator.id, &second_locator.id, Confidence::Exact)
        .unwrap();
    let commit = |creator_id: crate::CreatorId, external_target_id: &str| {
        let (route_id, locator_id) = if creator_id == first.creator.id {
            (&first_route.id, &locator.id)
        } else {
            (&second_route.id, &second_locator.id)
        };
        repository.commit_contact_target_resolution(CommitContactTargetResolutionInput {
            creator_id,
            contact_route_id: route_id.clone(),
            locator_id: locator_id.clone(),
            account_ref_id: operator.id.clone(),
            platform: Platform::X,
            target_kind: TargetKind::User,
            external_target_id: external_target_id.into(),
            attach_to_creator: true,
            metadata: serde_json::json!({ "handle": "shared-handle" }),
        })
    };

    let first_result = commit(first.creator.id.clone(), "stable-first").expect("first commit");
    assert!(matches!(
        first_result,
        CommitContactTargetResolutionResult::Active { .. }
    ));

    let owner_conflict = commit(second.creator.id.clone(), "stable-first").expect("owner conflict");
    match owner_conflict {
        CommitContactTargetResolutionResult::CreatorConflict {
            conflict,
            creator_ids,
            ..
        } => {
            assert_eq!(conflict.kind, "platform_account_already_confirmed");
            assert_eq!(creator_ids, vec![first.creator.id.clone()]);
        }
        _ => panic!("shared active resolution bypassed Creator ownership"),
    }

    let locator_conflict =
        commit(first.creator.id.clone(), "stable-changed").expect("locator conflict");
    match locator_conflict {
        CommitContactTargetResolutionResult::LocatorConflict {
            conflict, active, ..
        } => {
            assert_eq!(conflict.kind, "locator_target_changed");
            assert_eq!(active.target.external_target_id, "stable-first");
        }
        _ => panic!("changed locator target overwrote active state"),
    }

    let connection = repository.lock().expect("repository lock");
    let second_link_count = connection
        .query_row(
            "SELECT COUNT(*) FROM creator_account_links WHERE creator_id = ?1",
            [second.creator.id.as_str()],
            |row| row.get::<_, u64>(0),
        )
        .expect("second Creator link count");
    let changed_account_count = connection
        .query_row(
            "SELECT COUNT(*) FROM platform_accounts WHERE external_id = 'stable-changed'",
            [],
            |row| row.get::<_, u64>(0),
        )
        .expect("changed account count");
    let active_external_id = connection
        .query_row(
            "SELECT target.external_target_id
             FROM locator_resolutions resolution
             JOIN resolved_targets target ON target.id = resolution.resolved_target_id
             WHERE resolution.locator_id = ?1 AND resolution.account_ref_id = ?2
               AND resolution.state = 'active'",
            rusqlite::params![locator.id.as_str(), operator.id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .expect("active external id");
    assert_eq!(second_link_count, 0);
    assert_eq!(changed_account_count, 0);
    assert_eq!(active_external_id, "stable-first");
}

#[test]
fn locator_observations_are_append_only_and_do_not_merge_creators() {
    let (_directory, repository) = repository();
    let first = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "First".into(),
            notes: None,
        })
        .expect("first creator");
    let second = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Second".into(),
            notes: None,
        })
        .expect("second creator");
    let locator = repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::Email,
            kind: LocatorKind::EmailAddress,
            normalized_value: "hello@example.com".into(),
            display_value: "hello@example.com".into(),
        })
        .expect("locator");
    let source = repository
        .create_observation_source(ObservationSourceInput {
            kind: SourceKind::ProfileBio,
            platform: Some(Platform::X),
            source_url: Some("https://x.com/example".into()),
            external_ref: None,
            account_ref_id: None,
            metadata: serde_json::json!({}),
        })
        .expect("source");
    repository
        .append_locator_observation(
            &locator.id,
            &source.id,
            Some(&first.creator.id),
            None,
            "hello@example.com",
        )
        .expect("first observation");
    repository
        .append_locator_observation(
            &locator.id,
            &source.id,
            Some(&second.creator.id),
            None,
            "hello@example.com",
        )
        .expect("second observation");
    repository
        .link_creator_contact_route(&first.creator.id, &locator.id, Confidence::Possible)
        .expect("first route");

    assert_eq!(
        repository
            .list_creators(Default::default())
            .expect("creators")
            .items
            .len(),
        2
    );
    assert_eq!(
        repository
            .creator_contact_routes(&first.creator.id)
            .expect("first routes")
            .len(),
        1
    );
    assert!(
        repository
            .creator_contact_routes(&second.creator.id)
            .expect("second routes")
            .is_empty()
    );
}

#[test]
fn telegram_locator_stays_unresolved_until_an_account_context_resolves_it() {
    let (_directory, repository) = repository();
    let locator = repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::Telegram,
            kind: LocatorKind::Handle,
            normalized_value: "example".into(),
            display_value: "@example".into(),
        })
        .expect("telegram locator");
    let connection = repository.lock().expect("repository lock");
    let resolution_count = connection
        .query_row(
            "SELECT COUNT(*) FROM locator_resolutions WHERE locator_id = ?1",
            [locator.id.as_str()],
            |row| row.get::<_, u64>(0),
        )
        .expect("resolution count");
    assert_eq!(resolution_count, 0);
}

#[test]
fn changing_a_typed_telegram_target_records_a_conflict_without_overwriting() {
    let (_directory, repository) = repository();
    let actor = repository
        .upsert_account_ref(
            AccountRefId::new("tg-account-1"),
            Platform::Telegram,
            "profile",
            "@operator",
        )
        .expect("account ref");
    let locator = repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::Telegram,
            kind: LocatorKind::Handle,
            normalized_value: "example".into(),
            display_value: "@example".into(),
        })
        .expect("locator");
    let user = repository
        .upsert_resolved_target(
            &actor.id,
            Platform::Telegram,
            TargetKind::User,
            "1001",
            None,
            serde_json::json!({}),
        )
        .expect("user target");
    let channel = repository
        .upsert_resolved_target(
            &actor.id,
            Platform::Telegram,
            TargetKind::Channel,
            "1002",
            None,
            serde_json::json!({}),
        )
        .expect("channel target");
    repository
        .record_locator_resolution(&locator.id, &actor.id, &user.id)
        .expect("initial resolution");
    let result = repository
        .record_locator_resolution(&locator.id, &actor.id, &channel.id)
        .expect("conflict result");
    let LocatorResolutionWriteResult::Conflict {
        conflict,
        active,
        attempted_target,
    } = result
    else {
        panic!("target kind change must return a structured conflict");
    };
    assert_eq!(conflict.kind, "locator_target_changed");
    assert_eq!(active.target.id.as_str(), user.id.as_str());
    assert_eq!(attempted_target.id.as_str(), channel.id.as_str());

    let connection = repository.lock().expect("repository lock");
    let active_target = connection
        .query_row(
            "SELECT resolved_target_id FROM locator_resolutions
             WHERE locator_id = ?1 AND account_ref_id = ?2 AND state = 'active'",
            rusqlite::params![locator.id.as_str(), actor.id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .expect("active target");
    let conflict_count = connection
        .query_row("SELECT COUNT(*) FROM identity_conflicts", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect("conflict count");
    assert_eq!(active_target, user.id.as_str());
    assert_eq!(conflict_count, 1);
}

#[test]
fn locator_resolution_has_one_readable_current_state_and_explicit_invalidation() {
    let (_directory, repository) = repository();
    let actor = repository
        .upsert_account_ref(
            AccountRefId::new("x-account-current-state"),
            Platform::X,
            "profile",
            "@operator",
        )
        .expect("account ref");
    let locator = repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::X,
            kind: LocatorKind::Handle,
            normalized_value: "creator".into(),
            display_value: "@creator".into(),
        })
        .expect("locator");
    let target = repository
        .upsert_resolved_target(
            &actor.id,
            Platform::X,
            TargetKind::User,
            "x-user-current-state",
            None,
            serde_json::json!({ "handle": "creator" }),
        )
        .expect("target");

    let first = repository
        .record_locator_resolution(&locator.id, &actor.id, &target.id)
        .expect("first resolution");
    let second = repository
        .record_locator_resolution(&locator.id, &actor.id, &target.id)
        .expect("idempotent refresh");
    let active = repository
        .active_locator_resolution(&locator.id, &actor.id)
        .expect("active read")
        .expect("active resolution");

    let active_id = match first {
        LocatorResolutionWriteResult::Active { value } => value.resolution.id,
        LocatorResolutionWriteResult::Conflict { .. } => panic!("unexpected conflict"),
    };
    let refreshed_id = match second {
        LocatorResolutionWriteResult::Active { value } => value.resolution.id,
        LocatorResolutionWriteResult::Conflict { .. } => panic!("unexpected conflict"),
    };
    assert_eq!(active_id.as_str(), refreshed_id.as_str());
    assert_eq!(active.resolution.id.as_str(), active_id.as_str());
    assert_eq!(active.target.external_target_id, "x-user-current-state");

    let invalidated = repository
        .invalidate_locator_resolution(&locator.id, &actor.id)
        .expect("invalidate")
        .expect("invalidated resolution");
    assert_eq!(invalidated.id.as_str(), active_id.as_str());
    assert!(invalidated.invalidated_at.is_some());
    assert!(
        repository
            .active_locator_resolution(&locator.id, &actor.id)
            .expect("active read after invalidation")
            .is_none()
    );
    assert!(
        repository
            .invalidate_locator_resolution(&locator.id, &actor.id)
            .expect("idempotent invalidation")
            .is_none()
    );
}

#[test]
fn resolving_a_route_updates_identity_without_replacing_saved_profile_observations() {
    let (_directory, repository) = repository();
    let creator = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Harry".into(),
            notes: None,
        })
        .expect("creator");
    let operator = repository
        .upsert_account_ref(
            AccountRefId::new("telegram-operator-observation"),
            Platform::Telegram,
            "profile",
            "@operator",
        )
        .expect("operator");
    let locator = repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::Telegram,
            kind: LocatorKind::Handle,
            normalized_value: "harry_dgf".into(),
            display_value: "@harry_dgf".into(),
        })
        .expect("locator");
    let route = repository
        .link_creator_contact_route(&creator.creator.id, &locator.id, Confidence::Exact)
        .unwrap();
    let account = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::Telegram,
            target_kind: TargetKind::User,
            external_id: "5540665197".into(),
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
    let source = repository
        .create_observation_source(ObservationSourceInput {
            kind: SourceKind::InitialPlatformAccount,
            platform: Some(Platform::Telegram),
            source_url: Some("https://t.me/harry_dgf".into()),
            external_ref: Some("saved-profile".into()),
            account_ref_id: Some(operator.id.clone()),
            metadata: serde_json::json!({}),
        })
        .unwrap();
    repository.append_platform_account_observation(PlatformAccountObservationInput {
        platform_account_id: account.id, source_id: source.id, handle: Some("saved_handle".into()),
        display_name: Some("Saved profile".into()), profile_url: Some("https://t.me/saved_handle".into()),
        public_data: serde_json::json!({"avatar_url":"https://example.com/avatar.png","follower_count":42000}),
    }).unwrap();
    let before = repository.get_creator(&creator.creator.id).unwrap();
    let observations = serde_json::to_value(&before.account_observations).unwrap();
    let sources = serde_json::to_value(&before.observation_sources).unwrap();
    let commit = |display_name: &str| {
        repository.commit_contact_target_resolution(CommitContactTargetResolutionInput {
            creator_id: creator.creator.id.clone(), contact_route_id: route.id.clone(),
            locator_id: locator.id.clone(), account_ref_id: operator.id.clone(), platform: Platform::Telegram,
            target_kind: TargetKind::User, external_target_id: "5540665197".into(), attach_to_creator: true,
            metadata: serde_json::json!({"handle":"harry_dgf","display_name":display_name,"canonical_url":"https://t.me/harry_dgf"}),
        })
    };
    for display_name in ["Harry DGF", "Harry Renamed"] {
        let CommitContactTargetResolutionResult::Active { value } = commit(display_name).unwrap()
        else {
            panic!("active target")
        };
        assert_eq!(value.target.metadata["display_name"], display_name);
        assert_eq!(value.target.metadata["handle"], "harry_dgf");
        assert_eq!(
            value.target.metadata["canonical_url"],
            "https://t.me/harry_dgf"
        );
        let record = repository.get_creator(&creator.creator.id).unwrap();
        assert_eq!(
            serde_json::to_value(&record.account_observations).unwrap(),
            observations
        );
        assert_eq!(
            serde_json::to_value(&record.observation_sources).unwrap(),
            sources
        );
        assert_eq!(
            record.account_observations[0].public_data["avatar_url"],
            "https://example.com/avatar.png"
        );
        assert_eq!(
            record.account_observations[0].public_data["follower_count"],
            42000
        );
    }
}
