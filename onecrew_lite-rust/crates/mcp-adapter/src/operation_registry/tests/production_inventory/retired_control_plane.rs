use super::*;

#[test]
fn production_registry_has_no_creator_sync_control_plane_surface_or_schema() {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(platform),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();
    let forbidden = [
        "activation",
        "pairing",
        "enrollment",
        "creator_sync",
        "creator-sync",
        "creator.snapshot.sync",
        "snapshot_sync",
        "snapshot-sync",
        "creator-snapshots",
    ];

    for (id, _) in registry.operation_ids_and_surfaces() {
        let crate::operation_registry::descriptor::RegistryEntry::Operation(operation) = registry
            .entry(&id, SurfaceId::Full)
            .expect("full surface contains every operation")
        else {
            unreachable!()
        };
        let descriptor = format!(
            "{} {} {} {} {}",
            id.as_str(),
            operation.metadata.title,
            operation.metadata.summary,
            operation.metadata.domain_id,
            operation.metadata.aliases.join(" ")
        )
        .to_ascii_lowercase();
        let input_schema = serde_json::to_string(operation.input.document())
            .unwrap()
            .to_ascii_lowercase();
        let output_schema = serde_json::to_string(operation.output.document())
            .unwrap()
            .to_ascii_lowercase();
        for token in &forbidden {
            assert!(!descriptor.contains(token), "{id} exposes {token}");
            assert!(!input_schema.contains(token), "{id} input exposes {token}");
            assert!(
                !output_schema.contains(token),
                "{id} output exposes {token}"
            );
        }
    }
}
