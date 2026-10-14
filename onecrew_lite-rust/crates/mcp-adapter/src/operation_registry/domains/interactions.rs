use std::convert::Infallible;

use schemars::JsonSchema;
use serde::Serialize;

use crate::operation_registry::{
    descriptor::{
        ConnectionRequirement, EffectClass, OperationRegistration, OperationViewKind, SurfaceId,
        SurfaceSet,
    },
    registration::{OperationPolicy, RegistryBuildError, RegistryBuilder},
    v2::AskUserQuestionInput,
};

const SURFACES: [SurfaceId; 2] = [SurfaceId::Full, SurfaceId::Compact];

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct UserInputPrepared {
    input_required: bool,
}

pub(super) fn register(builder: &mut RegistryBuilder) -> Result<(), RegistryBuildError> {
    register_search_plan(builder)?;
    builder.register_operation::<AskUserQuestionInput, UserInputPrepared, Infallible, _, _, _>(
        OperationRegistration {
            id: onecrew_local_runtime::OperationId::ONECREW_ASK_USER_QUESTION,
            domain_id: "interactions".to_owned(),
            aliases: Vec::new(),
            surfaces: SurfaceSet::new(SURFACES).expect("question operation has fixed surfaces"),
        },
        OperationPolicy::read_only(
            EffectClass::ReadLocal,
            ConnectionRequirement::None,
            crate::operation_registry::execution::ExecutionDeclaration::Sequential,
        )
        .with_view_kind(OperationViewKind::Choice),
        |_context, _input| async move {
            Ok(UserInputPrepared {
                input_required: true,
            })
        },
        |never| match never {},
    )
}

fn register_search_plan(builder: &mut RegistryBuilder) -> Result<(), RegistryBuildError> {
    use crate::operation_registry::{approval::ApprovalRequest, descriptor::ApprovalClass};
    let mut policy = OperationPolicy::read_only(
        EffectClass::ReadLocal,
        ConnectionRequirement::None,
        crate::operation_registry::execution::ExecutionDeclaration::Sequential,
    );
    policy.approval = ApprovalClass::AttendedPrompt;
    policy.safety.confirmation.mode =
        crate::operation_registry::descriptor::ConfirmationMode::AttendedPrompt;
    policy.view_kind = OperationViewKind::SearchPlanEditor;
    policy.approval_request = Some(
        ApprovalRequest::new(
            "Review and confirm this search plan before executing its approved searches.",
        )
        .expect("static prompt"),
    );
    builder.register_operation::<onecrew_local_runtime::SearchPlanEditorDraft, UserInputPrepared, Infallible, _, _, _>(
        OperationRegistration {
            id: onecrew_local_runtime::OperationId::ONECREW_SEARCH_PLAN_PREPARE, domain_id: "creators".to_owned(), aliases: vec![],
            surfaces: SurfaceSet::new([SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators]).expect("fixed search surfaces"),
        }, policy,
        |_context, _input| async move { Ok(UserInputPrepared { input_required: true }) },
        |never| match never {},
    )
}
