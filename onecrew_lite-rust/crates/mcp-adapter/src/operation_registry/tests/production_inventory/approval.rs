use super::*;

#[test]
fn write_and_attended_sets_include_only_payload_confirmation_operations() {
    let owner_operations = [
        "onecrew.creator.content_reply.send",
        "onecrew.creator.archive",
        "onecrew.creator.candidate.save",
        "onecrew.creator.candidate.batch.save",
        "telegram.community.get",
        "telegram.community.administrators.list",
        "onecrew.creator.contact_route.add",
        "onecrew.creator.contact_route.delete.confirm",
        "onecrew.creator.contact_route.delete.prepare",
        "onecrew.creator.contact_route.update",
        "onecrew.creator.delete",
        "onecrew.creator.export",
        "onecrew.creator.manual.create",
        "onecrew.creator.batch.create",
        "onecrew.creator.update",
        "onecrew.creator.stage.update",
        "onecrew.creator.group.create",
        "onecrew.creator.group.update",
        "onecrew.creator.group.delete",
        "onecrew.creator.group.set",
        "onecrew.creator.group.bulk_move",
        "onecrew.outreach.contact_target.resolve",
        "onecrew.outreach.relationship.get",
        "onecrew.outreach.relationship.prepare",
        "onecrew.outreach.relationship.update",
        "onecrew.outreach.message.prepare",
        "onecrew.outreach.message.send",
        "x.message.send",
        "x.conversation.sync",
        "instagram.message.send",
        "instagram.conversation.sync",
        "telegram.message.send",
        "telegram.conversation.sync",
        "onecrew.triage.record",
        "onecrew.triage.acknowledge",
        "email.draft.restore",
        "email.draft.save",
        "email.draft.send",
        "email.draft.trash",
        "email.mailbox.sync",
        "email.manual.send",
        "email.direct.send",
        "email.send_many",
        "email.send_many.reconcile",
        "email.template.delete",
        "email.template.save",
        "email.thread.update_disposition",
    ];

    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();
    for id in owner_operations {
        assert!(
            registry
                .entry(&EntryId::parse(id).unwrap(), SurfaceId::Full)
                .is_some(),
            "missing owner operation {id}"
        );
    }

    let attended =
        registry
            .operation_ids_and_surfaces()
            .into_iter()
            .filter_map(|(id, _)| match registry.entry(&id, SurfaceId::Full) {
                Some(crate::operation_registry::descriptor::RegistryEntry::Operation(
                    operation,
                )) if operation.approval == ApprovalClass::AttendedPrompt => Some(id),
                _ => None,
            })
            .map(|id| id.as_str().to_owned())
            .collect::<BTreeSet<_>>();
    assert_eq!(
        attended,
        [
            "email.credential.delete",
            "email.credential.rotate",
            "email.direct.send",
            "email.draft.send",
            "email.manual.send",
            "email.send_many",
            "email.send_many.reconcile",
            "email.template.delete",
            "instagram.connection.browser.cancel",
            "instagram.connection.browser.complete",
            "instagram.connection.disconnect",
            "instagram.connection.reconnect.begin",
            "instagram.connection.reconnect.cancel",
            "instagram.connection.reconnect.finalize",
            "instagram.message.send",
            "onecrew.creator.batch.create",
            "onecrew.creator.candidate.save",
            "onecrew.creator.candidate.batch.save",
            "onecrew.creator.contact_route.delete.confirm",
            "onecrew.creator.content_reply.send",
            "onecrew.creator.delete",
            "onecrew.creator.group.delete",
            "onecrew.discovery.campaign.approve",
            "onecrew.discovery.target.increase",
            "onecrew.outreach.message.send",
            "onecrew.outreach.relationship.update",
            "onecrew.search.plan.prepare",
            "telegram.connection.disconnect",
            "telegram.connection.login.cancel",
            "telegram.connection.reconnect.begin",
            "telegram.connection.reconnect.cancel",
            "telegram.connection.reconnect.finalize",
            "telegram.message.send",
            "x.chat.passcode.clear",
            "x.connection.browser.cancel",
            "x.connection.browser.complete",
            "x.connection.disconnect",
            "x.connection.reconnect.begin",
            "x.connection.reconnect.cancel",
            "x.connection.reconnect.finalize",
            "x.message.send",
            "youtube.credential.delete",
            "youtube.credential.rename",
            "youtube.credential.rotate",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    );
}
