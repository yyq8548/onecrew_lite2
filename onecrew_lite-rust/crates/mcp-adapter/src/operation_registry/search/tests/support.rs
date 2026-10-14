use super::*;
use onecrew_local_runtime::ToolkitId;

pub(super) fn production_registry() -> OperationRegistry {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let operator_accounts = Arc::new(PlatformRuntimeOperatorAccountWorkflow::new(Arc::clone(
        &platform,
    )));
    build_registry(
        RegistryOwners {
            creators: platform.creators().unwrap(),
            platform,
            operator_accounts,
        },
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap()
}

pub(super) fn search(
    registry: &OperationRegistry,
    query: &str,
    toolkits: Vec<ToolkitId>,
    effects: Vec<EffectClass>,
) -> TestSearchResult {
    let query = SearchQuery {
        text: SearchText::parse(query).unwrap(),
        toolkits: toolkits.clone(),
        effects,
        ..SearchQuery::default()
    };
    let (operations, playbooks) = registry.search_for_test(SurfaceId::Full, &query);
    let toolkit_guides = registry
        .toolkit_directory(SurfaceId::Full)
        .into_iter()
        .filter(|guide| toolkits.contains(&guide.toolkit))
        .collect();
    TestSearchResult {
        operations,
        playbooks,
        toolkit_guides,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct TestSearchResult {
    pub(super) operations: Vec<CatalogEntrySummary>,
    pub(super) playbooks: Vec<PlaybookSummary>,
    pub(super) toolkit_guides: Vec<ToolkitDirectory>,
}
