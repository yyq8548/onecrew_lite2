use crate::operation_registry::{
    descriptor::{
        ConnectionRequirement, EffectClass, OperationRegistration, SurfaceId, SurfaceSet,
    },
    execution::{ClassifiedOwnerFailure, ExecutionDeclaration, PublicFailureCode},
    registration::OperationPolicy,
};

pub(super) fn metadata(
    id: onecrew_local_runtime::OperationId,
    domain_id: &str,
    surfaces: impl IntoIterator<Item = SurfaceId>,
) -> OperationRegistration {
    OperationRegistration {
        id,
        domain_id: domain_id.to_owned(),
        aliases: Vec::new(),
        surfaces: SurfaceSet::new(surfaces).expect("OneCrew read operations have fixed surfaces"),
    }
}

pub(super) fn read_policy() -> OperationPolicy {
    OperationPolicy::read_only(
        EffectClass::ReadLocal,
        ConnectionRequirement::None,
        ExecutionDeclaration::Sequential,
    )
}

pub(super) fn classify_string_failure(
    code: &'static str,
) -> impl Fn(String) -> ClassifiedOwnerFailure {
    move |_error| {
        ClassifiedOwnerFailure::known_no_effect(
            PublicFailureCode::parse(code).expect("static Creator read failure code is valid"),
            false,
            None,
        )
    }
}

pub(super) fn classify_creator_group_read_failure(
    error: onecrew_creator_runtime::CreatorGroupError,
) -> ClassifiedOwnerFailure {
    let code = match error {
        onecrew_creator_runtime::CreatorGroupError::InvalidInput => "creator_group_invalid_input",
        onecrew_creator_runtime::CreatorGroupError::GroupNotFound => "creator_group_not_found",
        onecrew_creator_runtime::CreatorGroupError::DuplicateName
        | onecrew_creator_runtime::CreatorGroupError::CreatorNotFound
        | onecrew_creator_runtime::CreatorGroupError::RevisionConflict => {
            "creator_group_read_unavailable"
        }
        onecrew_creator_runtime::CreatorGroupError::Internal => {
            return ClassifiedOwnerFailure::known_no_effect(
                PublicFailureCode::parse("creator_group_read_unavailable")
                    .expect("static Creator group read code is valid"),
                false,
                None,
            );
        }
    };
    ClassifiedOwnerFailure::known_no_effect(
        PublicFailureCode::parse(code).expect("static Creator group read code is valid"),
        false,
        None,
    )
}

#[cfg(test)]
mod tests {
    use onecrew_creator_runtime::CreatorGroupError;

    use super::{ClassifiedOwnerFailure, PublicFailureCode, classify_creator_group_read_failure};

    #[test]
    fn creator_group_read_failures_are_bounded_and_known_no_effect() {
        for (error, code) in [
            (
                CreatorGroupError::InvalidInput,
                "creator_group_invalid_input",
            ),
            (CreatorGroupError::GroupNotFound, "creator_group_not_found"),
            (
                CreatorGroupError::DuplicateName,
                "creator_group_read_unavailable",
            ),
            (
                CreatorGroupError::CreatorNotFound,
                "creator_group_read_unavailable",
            ),
            (
                CreatorGroupError::RevisionConflict,
                "creator_group_read_unavailable",
            ),
            (
                CreatorGroupError::Internal,
                "creator_group_read_unavailable",
            ),
        ] {
            assert_eq!(
                classify_creator_group_read_failure(error),
                ClassifiedOwnerFailure::known_no_effect(
                    PublicFailureCode::parse(code).unwrap(),
                    false,
                    None,
                )
            );
        }
    }
}
