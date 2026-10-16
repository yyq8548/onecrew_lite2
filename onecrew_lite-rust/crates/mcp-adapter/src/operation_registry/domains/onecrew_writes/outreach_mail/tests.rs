use std::{collections::BTreeSet, sync::Arc};

use crate::operation_registry::{
    descriptor::{ApprovalClass, EffectClass, EntryId, RegistryEntry, SurfaceId},
    registration::OperationRegistry,
};

const FKO: &[SurfaceId] = &[SurfaceId::Full, SurfaceId::Compact, SurfaceId::Outreach];
const FKOM: &[SurfaceId] = &[
    SurfaceId::Full,
    SurfaceId::Compact,
    SurfaceId::Outreach,
    SurfaceId::Mail,
];
const FKM: &[SurfaceId] = &[SurfaceId::Full, SurfaceId::Compact, SurfaceId::Mail];

fn production_registry() -> OperationRegistry {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let operator_accounts = Arc::new(
        crate::accounts::PlatformRuntimeOperatorAccountWorkflow::new(Arc::clone(&platform)),
    );
    crate::operation_registry::build_registry(
        crate::operation_registry::RegistryOwners {
            creators: platform.creators().unwrap(),
            platform,
            operator_accounts,
        },
        Arc::new(onecrew_local_runtime::CapabilityCatalog::build().unwrap()),
    )
    .unwrap()
}

#[test]
fn outreach_mail_retains_public_reads_and_local_history_changes() {
    let expected = [
        ("onecrew.outreach.contact_target.resolve", EffectClass::SyncOpenWorld, FKO),
        ("onecrew.outreach.relationship.get", EffectClass::SyncOpenWorld, FKO),
        ("onecrew.triage.record", EffectClass::WriteLocal, FKOM),
        ("onecrew.triage.acknowledge", EffectClass::WriteLocal, FKOM),
        ("email.mailbox.sync", EffectClass::SyncOpenWorld, FKM),
        ("email.thread.update_disposition", EffectClass::WriteLocal, FKM),
    ];
    assert_eq!(expected.iter().map(|(id, _, _)| *id).collect::<BTreeSet<_>>().len(), 6);
    let registry = production_registry();
    for (id, effect, surfaces) in expected {
        let id = EntryId::parse(id).unwrap();
        let RegistryEntry::Operation(entry) = registry.entry(&id, SurfaceId::Full).unwrap() else {
            panic!("expected retained operation {id}");
        };
        assert_eq!(entry.effect, effect, "{id}");
        assert_eq!(entry.metadata.surfaces.iter().collect::<Vec<_>>(), surfaces, "{id}");
        assert_eq!(entry.approval, ApprovalClass::OwnerGuarded, "{id}");
        assert!(matches!(entry.connection, crate::operation_registry::descriptor::ConnectionRequirement::None));
        assert!(matches!(entry.secret, crate::operation_registry::descriptor::SecretClass::None));
        assert!(entry.sensitive_input_paths.is_empty());
    }
}
