use super::*;
use onecrew_local_runtime::ToolkitId;

#[test]
fn production_toolkit_and_playbook_inventory_is_exact() {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();

    let mut counts = std::collections::BTreeMap::new();
    for (id, _) in registry.operation_ids_and_surfaces() {
        let crate::operation_registry::descriptor::RegistryEntry::Operation(operation) = registry
            .entry(&id, SurfaceId::Full)
            .expect("full surface contains every operation")
        else {
            unreachable!()
        };
        *counts.entry(operation.metadata.toolkit).or_insert(0_usize) += 1;
    }
    assert_eq!(
        counts,
        [
            (ToolkitId::X, 20),
            (ToolkitId::Instagram, 16),
            (ToolkitId::Telegram, 18),
            (ToolkitId::Youtube, 9),
            (ToolkitId::OneCrew, 67),
            (ToolkitId::Email, 26),
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(registry.operation_ids_and_surfaces().len(), 156);
    for removed in [
        "email.credential.connect",
        "email.credential.rotate",
        "email.credential.delete",
        "task.setup_form",
        "task.list",
        "task.get",
        "task.attention.list",
        "task.attention.acknowledge",
        "task.create",
        "task.update",
        "task.cancel",
        "task.delete",
        "task.claim",
        "task.report",
        "playbook.task.claim-and-report",
        "onecrew.user_choice.form",
    ] {
        assert!(
            registry
                .entry(&EntryId::parse(removed).unwrap(), SurfaceId::Full)
                .is_none(),
            "retired form operation survived: {removed}"
        );
    }
    for retired in [
        "onecrew.creator.profile.refresh",
        "playbook.onecrew.creator-profile-refresh",
    ] {
        assert!(
            registry
                .entry(&EntryId::parse(retired).unwrap(), SurfaceId::Full)
                .is_none(),
            "retired Creator profile refresh survived: {retired}"
        );
    }

    let expected_playbooks = [
        "playbook.email.draft-and-send",
        "playbook.email.inbox-triage",
        "playbook.instagram.connection-browser",
        "playbook.instagram.creator-discovery",
        "playbook.onecrew.creator-candidate-batch-save",
        "playbook.onecrew.creator-candidate-save",
        "playbook.onecrew.creator-contact-confirm",
        "playbook.onecrew.creator-manual-batch-create",
        "playbook.onecrew.creator-profile-update",
        "playbook.onecrew.direct-message",
        "playbook.recipe.email-best-route",
        "playbook.recipe.find-instagram-creators",
        "playbook.recipe.find-telegram-administrators",
        "playbook.recipe.find-x-creators",
        "playbook.recipe.find-youtube-creators",
        "playbook.recipe.instagram-best-route",
        "playbook.recipe.triage-email-and-onecrew",
        "playbook.recipe.x-best-route",
        "playbook.telegram.community-discovery",
        "playbook.telegram.connection-phone",
        "playbook.telegram.connection-qr",
        "playbook.x.connection-browser",
        "playbook.x.creator-discovery",
        "playbook.x.thread-read",
        "playbook.youtube.channel-discovery",
        "playbook.youtube.credential-lifecycle",
    ];
    assert_eq!(
        registry.playbook_ids(),
        expected_playbooks
            .into_iter()
            .map(|id| EntryId::parse(id).unwrap())
            .collect::<Vec<_>>()
    );
    let expected_guides = [
        "toolkit.email",
        "toolkit.instagram",
        "toolkit.onecrew",
        "toolkit.telegram",
        "toolkit.x",
        "toolkit.youtube",
    ];
    assert_eq!(
        registry.toolkit_guide_ids(),
        expected_guides
            .into_iter()
            .map(|id| EntryId::parse(id).unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        registry.operation_ids_and_surfaces().len()
            + registry.playbook_ids().len()
            + registry.toolkit_guide_ids().len(),
        156 + expected_playbooks.len() + 6
    );

    for surface in [
        SurfaceId::Full,
        SurfaceId::Compact,
        SurfaceId::Accounts,
        SurfaceId::Creators,
        SurfaceId::Outreach,
        SurfaceId::Mail,
        SurfaceId::Skills,
    ] {
        let operation_toolkits = registry.operation_toolkits(surface);
        for guide_id in registry.toolkit_guide_ids() {
            let toolkit = match guide_id.as_str() {
                "toolkit.x" => ToolkitId::X,
                "toolkit.instagram" => ToolkitId::Instagram,
                "toolkit.telegram" => ToolkitId::Telegram,
                "toolkit.youtube" => ToolkitId::Youtube,
                "toolkit.onecrew" => ToolkitId::OneCrew,
                "toolkit.email" => ToolkitId::Email,
                _ => unreachable!(),
            };
            assert_eq!(
                registry.entry(&guide_id, surface).is_some(),
                operation_toolkits.contains(&toolkit),
                "{guide_id} surface closure drifted on {surface:?}"
            );
        }
    }

    for id in registry.playbook_ids() {
        let crate::operation_registry::descriptor::RegistryEntry::Playbook(playbook) =
            registry.entry(&id, SurfaceId::Full).unwrap()
        else {
            unreachable!()
        };
        let primary = playbook
            .primary_operation_ids
            .iter()
            .collect::<BTreeSet<_>>();
        let supporting = playbook
            .supporting_operation_ids
            .iter()
            .collect::<BTreeSet<_>>();
        assert_eq!(primary.len(), playbook.primary_operation_ids.len(), "{id}");
        assert_eq!(
            supporting.len(),
            playbook.supporting_operation_ids.len(),
            "{id}"
        );
        assert!(primary.is_disjoint(&supporting), "{id}");
        assert!(playbook.recommended_steps.len() <= 6, "{id}");
        assert!(playbook.known_pitfalls.len() <= 5, "{id}");
        for step in &playbook.recommended_steps {
            assert!(
                step.operation_ids
                    .iter()
                    .all(|op| primary.contains(op) || supporting.contains(op)),
                "{id}"
            );
        }
    }
}
