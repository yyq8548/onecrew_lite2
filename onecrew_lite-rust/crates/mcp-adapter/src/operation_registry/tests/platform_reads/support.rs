use super::*;

/// A real registry over an empty connection store. Valid UUIDs reach toolkit
/// admission and fail there, so these tests cannot make a provider request.
pub(super) fn production_registry() -> (
    tempfile::TempDir,
    Arc<onecrew_local_runtime::LocalPlatform>,
    crate::operation_registry::OperationRegistry,
) {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
    let registry = crate::operation_registry::build_registry(
        production_owners(Arc::clone(&platform)),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();
    (directory, platform, registry)
}

pub(super) const UNKNOWN_CONNECTION: &str = "10000000-0000-4000-8000-000000000001";

pub(super) fn search_calls() -> Vec<(&'static str, ConnectionRef, Value)> {
    let operator = |toolkit| ConnectionRef::OperatorAccount {
        toolkit,
        operator_account_id: UNKNOWN_CONNECTION.into(),
    };
    let youtube = ConnectionRef::ServiceCredential {
        toolkit: ServiceToolkitId::Youtube,
        credential_id: UNKNOWN_CONNECTION.into(),
    };
    vec![
        (
            "instagram.users.search",
            operator(OperatorToolkitId::Instagram),
            json!({"query":"rust"}),
        ),
        (
            "telegram.communities.search",
            operator(OperatorToolkitId::Telegram),
            json!({"query":"rust"}),
        ),
        (
            "youtube.channels.search",
            youtube.clone(),
            json!({"query":"rust"}),
        ),
        ("youtube.videos.search", youtube, json!({"query":"rust"})),
        (
            "x.posts.search.advanced",
            operator(OperatorToolkitId::X),
            json!({"all_words":["rust"]}),
        ),
    ]
}

pub(super) fn assert_connection_admission_failure(
    outcomes: &[crate::operation_registry::v2::ToolExecutionOutcome],
) {
    let [crate::operation_registry::v2::ToolExecutionOutcome::Failed { failure, .. }] = outcomes
    else {
        panic!("expected one toolkit connection failure, got {outcomes:?}");
    };
    assert_eq!(failure.code.as_str(), "connection_not_found_or_mismatch");
    assert_eq!(failure.phase, ExecuteFailurePhase::AfterInvocation);
    assert_eq!(failure.effect, ExecuteEffectState::KnownNoEffect);
    assert!(!failure.retry_safe);
    assert_eq!(failure.recovery, Some(RecoveryDirective::CorrectInput));
    assert_eq!(failure.provider_status, None);
}
