use crate::operation_registry::{
    approval::ApprovalRequest,
    descriptor::{ApprovalClass, ConnectionRequirement, EffectClass, OperationViewKind, SurfaceId},
    registration::{OperationPolicy, RegistryBuilder},
    v2::{
        contract::{
            PresentationRequest, ResultPresentInput, ResultStateInput, ResultStateOutput,
            ToolExecuteInput,
        },
        projection::AdapterPresentationKind,
        state::result_present_v2,
    },
};
use serde_json::json;

use super::fixtures::{
    TestInput, TestOutput, TestOwnerError, call, metadata, policy, result_owner, revision,
};

#[test]
fn operation_view_kind_mapping_is_closed_and_uses_only_strict_constants() {
    let mappings = [
        (OperationViewKind::SearchPlanEditor, "search_plan_editor"),
        (OperationViewKind::OutreachDraft, "outreach_draft"),
        (OperationViewKind::RecordPreview, "record_preview"),
        (OperationViewKind::SimpleOperation, "simple_operation"),
        (OperationViewKind::ReadOnly, "read_only"),
        (OperationViewKind::Choice, "choice"),
    ];
    for (view_kind, expected) in mappings {
        let mapped = AdapterPresentationKind::from(view_kind);
        assert_eq!(mapped.as_str(), expected);
        assert_eq!(mapped.runtime().as_str(), expected);
    }
    assert_eq!(
        AdapterPresentationKind::ALL.map(AdapterPresentationKind::as_str),
        [
            "search_plan_editor",
            "outreach_draft",
            "record_preview",
            "simple_operation",
            "read_only",
            "choice",
            "batch_result",
        ]
    );
}

#[tokio::test]
async fn confirmation_and_resumed_terminal_preserve_view_kind_across_remount() {
    let mut draft_policy = policy(ApprovalClass::AttendedPrompt, EffectClass::ExternalWrite);
    draft_policy.view_kind = OperationViewKind::OutreachDraft;
    draft_policy.approval_request = Some(ApprovalRequest::new("Confirm this draft?").unwrap());
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.draft-kind",
        ]));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.draft-kind"),
            draft_policy,
            |_context, input| async move {
                Ok::<_, TestOwnerError>(TestOutput { value: input.value })
            },
            |_| unreachable!("test owner succeeds"),
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: vec![call(
            "call.draft-kind",
            "onecrew.test.draft-kind",
            revision(&registry, "onecrew.test.draft-kind"),
            json!({"value":"draft"}),
        )],
        presentation: PresentationRequest::Never,
    };
    let result_ref = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "batch-draft-kind",
            &input,
        )
        .await
        .unwrap()
        .result_ref
        .unwrap();
    let available = result_present_v2(
        &platform,
        &origin,
        ResultPresentInput {
            result_ref,
            locale: Default::default(),
        },
    )
    .unwrap()
    .snapshot;
    assert_eq!(available.presentation.kind.as_str(), "outreach_draft");
    let terminal = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::Confirm {
                binding: available.binding,
            },
        )
        .await
        .unwrap();
    let ResultStateOutput::Snapshot { snapshot, .. } = terminal else {
        panic!("terminal snapshot")
    };
    assert_eq!(snapshot.presentation.kind.as_str(), "outreach_draft");
    let remounted = platform.read_current_result(&snapshot.binding).unwrap();
    assert_eq!(remounted.presentation.kind.as_str(), "outreach_draft");
}

#[tokio::test]
async fn mixed_operation_view_kinds_project_batch_result() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.mixed-draft",
            "onecrew.test.mixed-record",
        ]));
    for (id, view_kind) in [
        ("onecrew.test.mixed-draft", OperationViewKind::OutreachDraft),
        (
            "onecrew.test.mixed-record",
            OperationViewKind::RecordPreview,
        ),
    ] {
        let operation_policy = OperationPolicy::read_only(
            EffectClass::ReadLocal,
            ConnectionRequirement::None,
            crate::operation_registry::execution::ExecutionDeclaration::Sequential,
        )
        .with_view_kind(view_kind);
        builder
            .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
                metadata(id),
                operation_policy,
                |_context, input| async move {
                    Ok::<_, TestOwnerError>(TestOutput { value: input.value })
                },
                |_| unreachable!("test owner succeeds"),
            )
            .unwrap();
    }
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: [
            ("call.mixed-draft", "onecrew.test.mixed-draft"),
            ("call.mixed-record", "onecrew.test.mixed-record"),
        ]
        .into_iter()
        .map(|(call_id, operation_id)| {
            call(
                call_id,
                operation_id,
                revision(&registry, operation_id),
                json!({"value":call_id}),
            )
        })
        .collect(),
        presentation: PresentationRequest::Requested,
    };
    let result_ref = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "batch-mixed-kind",
            &input,
        )
        .await
        .unwrap()
        .result_ref
        .unwrap();
    let snapshot = result_present_v2(
        &platform,
        &origin,
        ResultPresentInput {
            result_ref,
            locale: Default::default(),
        },
    )
    .unwrap()
    .snapshot;
    assert_eq!(snapshot.presentation.kind.as_str(), "batch_result");
}
