use super::*;

#[test]
fn production_registry_builds_without_changing_public_surface_construction() {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();
    assert_eq!(
        registry.operation_ids_and_surfaces().len()
            + registry.playbook_ids().len()
            + registry.toolkit_guide_ids().len(),
        188
    );
    assert!(!registry.operation_ids_and_surfaces().is_empty());
    let prepare = EntryId::parse("onecrew.search.plan.prepare").unwrap();
    let guide = EntryId::parse("onecrew.workflow.guide").unwrap();
    for (surface, expected) in [
        (SurfaceId::Full, true),
        (SurfaceId::Compact, true),
        (SurfaceId::Creators, true),
        (SurfaceId::Accounts, false),
        (SurfaceId::Outreach, false),
        (SurfaceId::Mail, false),
        (SurfaceId::Skills, false),
    ] {
        assert_eq!(
            registry.entry(&prepare, surface).is_some(),
            expected,
            "prepare on {surface:?}"
        );
        assert_eq!(
            registry.entry(&guide, surface).is_some(),
            expected,
            "workflow guide on {surface:?}"
        );
    }

    for id in [
        "x.posts.search.advanced",
        "x.profile.get",
        "x.user_posts.list",
        "x.post_thread.get",
        "instagram.users.search",
        "instagram.profile.get",
        "telegram.communities.search",
        "telegram.community.get",
        "telegram.community.administrators.list",
        "youtube.channels.search",
        "youtube.videos.search",
        "youtube.channel.get",
    ] {
        let entry_id = EntryId::parse(id).unwrap();
        let entry = registry.entry(&entry_id, SurfaceId::Creators).unwrap();
        let crate::operation_registry::descriptor::RegistryEntry::Operation(operation) = entry
        else {
            panic!("{id} must be executable");
        };
        assert!(operation.metadata.surfaces.allows(SurfaceId::Full));
        assert!(operation.metadata.surfaces.allows(SurfaceId::Compact));
        assert!(!operation.metadata.surfaces.allows(SurfaceId::Accounts));
    }

    let advanced_x_search = registry_operation(&registry, "x.posts.search.advanced");
    assert_eq!(advanced_x_search.effect, EffectClass::ReadOpenWorld);
    assert_eq!(
        advanced_x_search.connection,
        ConnectionRequirement::OperatorAccount {
            toolkit: OperatorToolkitId::X,
        }
    );
    assert!(advanced_x_search.metadata.summary.contains("typed words"));
    let advanced_schema = advanced_x_search.input.document();
    assert_eq!(advanced_schema["properties"]["count"]["minimum"], 1);
    assert_eq!(advanced_schema["properties"]["count"]["maximum"], 100);
    assert_eq!(advanced_schema["properties"]["all_words"]["maxItems"], 20);
    assert_eq!(
        advanced_schema["properties"]["exact_phrase"]["maxLength"],
        256
    );

    let x_profile = registry
        .entry(&EntryId::parse("x.profile.get").unwrap(), SurfaceId::Full)
        .unwrap();
    let youtube_channel = registry
        .entry(
            &EntryId::parse("youtube.channel.get").unwrap(),
            SurfaceId::Full,
        )
        .unwrap();
    for (entry, required, forbidden) in [
        (x_profile, "handle", ["target", "external_id", "action"]),
        (
            youtube_channel,
            "channel_id",
            ["target", "handle", "action"],
        ),
    ] {
        let crate::operation_registry::descriptor::RegistryEntry::Operation(operation) = entry
        else {
            unreachable!();
        };
        let schema = serde_json::to_string(operation.input.document()).unwrap();
        assert!(schema.contains(required));
        if required == "channel_id" {
            let required_fields = operation.input.document()["required"]
                .as_array()
                .expect("YouTube channel input has required fields");
            assert!(
                required_fields
                    .iter()
                    .any(|field| field.as_str() == Some("depth"))
            );
        }
        assert!(forbidden.iter().all(|field| !schema.contains(field)));
    }

    for id in [
        "x.connection.get",
        "instagram.connection.get",
        "telegram.connection.get",
        "youtube.connection.get",
    ] {
        let entry_id = EntryId::parse(id).unwrap();
        assert!(registry.entry(&entry_id, SurfaceId::Accounts).is_some());
        assert!(registry.entry(&entry_id, SurfaceId::Mail).is_some());
        assert!(registry.entry(&entry_id, SurfaceId::Creators).is_none());
    }
}

#[test]
fn phase_six_connection_and_credential_operation_inventory_is_exact_and_surface_closed() {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();
    let expected = [
        "x.connection.browser.start",
        "x.connection.browser.status",
        "x.connection.browser.complete",
        "x.connection.browser.cancel",
        "x.chat.passcode.set",
        "x.chat.passcode.clear",
        "x.connection.disconnect",
        "x.connection.reconnect.prepare",
        "x.connection.reconnect.begin",
        "x.connection.reconnect.status",
        "x.connection.reconnect.finalize",
        "x.connection.reconnect.cancel",
        "instagram.connection.browser.start",
        "instagram.connection.browser.status",
        "instagram.connection.browser.complete",
        "instagram.connection.browser.cancel",
        "instagram.connection.disconnect",
        "instagram.connection.reconnect.prepare",
        "instagram.connection.reconnect.begin",
        "instagram.connection.reconnect.status",
        "instagram.connection.reconnect.finalize",
        "instagram.connection.reconnect.cancel",
        "telegram.connection.login.start",
        "telegram.connection.login.status",
        "telegram.connection.login.otp.submit",
        "telegram.connection.login.two_factor.submit",
        "telegram.connection.login.cancel",
        "telegram.connection.disconnect",
        "telegram.connection.reconnect.prepare",
        "telegram.connection.reconnect.begin",
        "telegram.connection.reconnect.status",
        "telegram.connection.reconnect.finalize",
        "telegram.connection.reconnect.cancel",
        "youtube.credential.connect",
        "youtube.credential.rotate",
        "youtube.credential.rename",
        "youtube.credential.delete",
    ];
    assert_eq!(expected.len(), 37);
    for id in expected {
        let id = EntryId::parse(id).unwrap();
        assert!(registry.entry(&id, SurfaceId::Full).is_some(), "{id}");
        assert!(registry.entry(&id, SurfaceId::Accounts).is_some(), "{id}");
        let credential = matches!(id.as_str().split('.').next(), Some("youtube" | "email"));
        assert_eq!(
            registry.entry(&id, SurfaceId::Compact).is_some(),
            credential,
            "{id} compact"
        );
        assert_eq!(
            registry.entry(&id, SurfaceId::Mail).is_some(),
            credential,
            "{id} mail"
        );
        for forbidden in [SurfaceId::Creators, SurfaceId::Outreach, SurfaceId::Skills] {
            assert!(
                registry.entry(&id, forbidden).is_none(),
                "{id} on {forbidden:?}"
            );
        }
    }

    let exact_secrets = [
        (
            "x.connection.browser.start",
            SecretClass::Transient,
            &["/proxy/password", "/x_chat_passcode"][..],
        ),
        (
            "instagram.connection.browser.start",
            SecretClass::Transient,
            &["/proxy/password", "/x_chat_passcode"][..],
        ),
        (
            "telegram.connection.login.start",
            SecretClass::Transient,
            &["/api_hash", "/proxy/password", "/phone"][..],
        ),
        (
            "telegram.connection.login.otp.submit",
            SecretClass::Transient,
            &["/otp"][..],
        ),
        (
            "telegram.connection.login.two_factor.submit",
            SecretClass::Transient,
            &["/password"][..],
        ),
        (
            "x.chat.passcode.set",
            SecretClass::VaultBound,
            &["/x_chat_passcode"][..],
        ),
        (
            "youtube.credential.connect",
            SecretClass::VaultBound,
            &["/api_key"][..],
        ),
        (
            "youtube.credential.rotate",
            SecretClass::VaultBound,
            &["/api_key"][..],
        ),
    ];
    for (id, secret, paths) in exact_secrets {
        let Some(crate::operation_registry::descriptor::RegistryEntry::Operation(operation)) =
            registry.entry(&EntryId::parse(id).unwrap(), SurfaceId::Full)
        else {
            panic!("missing {id}")
        };
        assert_eq!(operation.secret, secret, "{id}");
        assert_eq!(operation.sensitive_input_paths, paths, "{id}");
    }
}
