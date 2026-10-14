use super::*;

#[test]
fn private_operation_ids_are_atomically_toolkit_owned_without_legacy_aliases() {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();

    assert!(
        registry
            .entry(
                &EntryId::parse("onecrew.connection.inventory").unwrap(),
                SurfaceId::Full
            )
            .is_none(),
        "connection inventory is the fixed onecrew_connection_list projection, not a registry operation"
    );

    assert!(
        registry
            .entry(
                &EntryId::parse("onecrew.workflow.guide").unwrap(),
                SurfaceId::Full
            )
            .is_some()
    );

    for summary in registry.catalog_summaries(SurfaceId::Full) {
        if summary.kind == crate::operation_registry::descriptor::EntryKind::Operation {
            assert_eq!(
                summary.id.as_str().split('.').next(),
                Some(summary.toolkit.as_str()),
                "{}",
                summary.id
            );
        }
    }
    for legacy in [
        "platform.x.search",
        "platform.instagram.profile",
        "platform.telegram.community_search",
        "platform.youtube.channel",
        "connection.x.get",
        "connection.inventory",
        "creator.get",
        "outreach.conversation.get",
        "triage.list",
        "mail.thread.list",
    ] {
        assert!(
            registry
                .entry(&EntryId::parse(legacy).unwrap(), SurfaceId::Full)
                .is_none(),
            "{legacy}"
        );
    }
}

#[test]
fn retired_creator_and_renamed_private_ids_are_absent_from_the_v2_registry() {
    const OLD_IDS: &[&str] = &[
        "onecrew.creator.shortlist.add",
        "onecrew.creator.shortlist.list",
        "onecrew.creator.shortlist.remove",
        "onecrew.creator.candidate.save.confirm",
        "onecrew.creator.candidate.batch.save.confirm",
        "onecrew.creator.community_administrators.discover",
        "connection.instagram.get",
        "connection.inventory",
        "connection.telegram.get",
        "connection.x.get",
        "connection.youtube.get",
        "creator.archive",
        "creator.candidate.save",
        "creator.candidate.save.confirm",
        "creator.community_administrators.discover",
        "creator.contact.confirm",
        "creator.contact.discover",
        "creator.contact_route.add",
        "creator.contact_route.delete.confirm",
        "creator.contact_route.delete.prepare",
        "creator.contact_route.update",
        "creator.content_reply.prepare",
        "creator.content_reply.send",
        "creator.delete",
        "creator.discover",
        "creator.export",
        "creator.get",
        "creator.identity.resolve",
        "creator.link_hub.discover",
        "creator.list",
        "creator.manual.create",
        "creator.stage.update",
        "creator.update",
        "mail.direct.prepare",
        "mail.direct.send",
        "mail.draft.get",
        "mail.draft.list",
        "mail.draft.restore",
        "mail.draft.save",
        "mail.draft.send",
        "mail.draft.trash",
        "mail.folder.list",
        "mail.mailbox.get",
        "mail.mailbox.sync",
        "mail.mailbox.sync_status",
        "mail.manual.prepare",
        "mail.manual.send",
        "mail.message.get",
        "mail.template.delete",
        "mail.template.get",
        "mail.template.list",
        "mail.template.render",
        "mail.template.save",
        "mail.thread.get",
        "mail.thread.list",
        "mail.thread.update_disposition",
        "outreach.action.status",
        "outreach.contact_route.last_contact_account",
        "outreach.contact_target.resolve",
        "outreach.conversation.get",
        "outreach.conversation.sync",
        "outreach.message.prepare",
        "outreach.message.send",
        "outreach.relationship.get",
        "outreach.relationship.prepare",
        "outreach.relationship.update",
        "platform.instagram.profile",
        "platform.instagram.user_search",
        "platform.telegram.community_search",
        "platform.x.post_thread",
        "platform.x.profile",
        "platform.x.search",
        "platform.x.user_posts",
        "platform.x.user_search",
        "platform.youtube.channel",
        "platform.youtube.channel_search",
        "platform.youtube.video_search",
        "triage.acknowledge",
        "triage.list",
        "triage.record",
    ];
    assert_eq!(OLD_IDS.len(), 80);
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();
    for old_id in OLD_IDS {
        let id = EntryId::parse(*old_id).unwrap();
        assert!(
            registry.entry(&id, SurfaceId::Full).is_none(),
            "entry {old_id}"
        );
    }
}

/// The Telegram login lifecycle moved under `connection.login.*`, the two
/// candidate-enrichment operations became `enrich` and then retired with the
/// snapshot-keyed contact confirmation (the link-hub parser lives on as
/// `onecrew.link_hub.extract`), and the TikTok One toolkit was retired. None
/// left an alias behind.
#[test]
fn every_renamed_login_and_enrichment_id_is_absent_from_the_registry() {
    const OLD_IDS: &[&str] = &[
        "onecrew.creator.contact.confirm",
        "onecrew.creator.contact.discover",
        "onecrew.creator.contact.enrich",
        "onecrew.creator.link_hub.discover",
        "onecrew.creator.link_hub.enrich",
        "telegram.connection.cancel",
        "telegram.connection.otp.submit",
        "telegram.connection.start",
        "telegram.connection.status",
        "telegram.connection.two_factor.submit",
        "tiktok_one.account.get",
        "tiktok_one.account.list",
        "tiktok_one.authorization.begin",
        "tiktok_one.authorization.cancel",
        "tiktok_one.authorization.get",
        "tiktok_one.connection.delete",
        "tiktok_one.connection.disconnect",
        "tiktok_one.connection.list",
        "tiktok_one.creator.discover",
        "tiktok_one.creator.get",
        "tiktok_one.creator.labels",
        "tiktok_one.creator.onboarding_status",
        "tiktok_one.creator.videos",
        "tiktok_one.onecrew.account.bind",
        "tiktok_one.request.cancel",
    ];
    assert_eq!(OLD_IDS.len(), 25);
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();
    for old_id in OLD_IDS {
        let id = EntryId::parse(*old_id).unwrap();
        assert!(
            registry.entry(&id, SurfaceId::Full).is_none(),
            "entry {old_id}"
        );
    }
}
