use super::*;
use crate::search_plan::SearchAuthority;

#[derive(Serialize)]
struct Request {
    connection: &'static str,
    query: &'static str,
    cursor: Option<&'static str>,
}

const REQUEST: Request = Request {
    connection: "account-a",
    query: "rust",
    cursor: None,
};

fn confirmed() -> (SearchAuthority, String, OperationSearchAuthorization) {
    let authority = SearchAuthority::default();
    let reference = authority.confirm(vec!["scope-a".into()]).unwrap();
    let approval = authority.authorize(Some(&reference), "scope-a").unwrap();
    (authority, reference, approval)
}

#[test]
fn search_refusal_codes_preserve_the_complete_wire_contract() {
    let codes = SearchPlanApprovalError::ALL.map(SearchPlanApprovalError::code);
    assert_eq!(
        codes,
        [
            "search_plan_confirmation_required",
            "search_plan_invalid_or_expired",
            "search_plan_scope_mismatch",
            "search_plan_approval_unavailable",
        ]
    );
    assert_eq!(
        codes.iter().collect::<std::collections::HashSet<_>>().len(),
        SearchPlanApprovalError::ALL.len(),
    );
    for reason in SearchPlanApprovalError::ALL {
        assert_eq!(reason.to_string(), reason.code());
        assert_eq!(serde_json::to_value(reason).unwrap(), reason.code());
    }
    assert_eq!(
        schemars::schema_for!(SearchPlanApprovalError)
            .as_value()
            .get("enum"),
        Some(&serde_json::json!(codes)),
    );
}

#[test]
fn a_live_approval_binds_exact_request_and_rechecks_after_cancellation() {
    let (authority, reference, approval) = confirmed();
    let bound = BoundSearchAuthorization::bind(
        &REQUEST,
        SearchPlanPolicy::RequireConfirmation,
        Some(&reference),
        Some(approval),
    )
    .unwrap();
    // Rechecking the same page consumes no budget and does not invalidate it.
    for _ in 0..64 {
        bound.validate_request(&REQUEST).unwrap();
    }
    for changed in [
        Request {
            query: "defi",
            ..REQUEST
        },
        Request {
            connection: "account-b",
            ..REQUEST
        },
        Request {
            cursor: Some("page-2"),
            ..REQUEST
        },
    ] {
        assert_eq!(
            bound.validate_request(&changed),
            Err(SearchPlanApprovalError::ScopeMismatch)
        );
    }
    // A later page gets its own request binding under the same criteria scope.
    let next = Request {
        cursor: Some("page-2"),
        ..REQUEST
    };
    let next_bound = BoundSearchAuthorization::bind(
        &next,
        SearchPlanPolicy::RequireConfirmation,
        Some(&reference),
        Some(authority.authorize(Some(&reference), "scope-a").unwrap()),
    )
    .unwrap();
    next_bound.validate_request(&next).unwrap();
    authority.cancel(&reference).unwrap();
    assert_eq!(
        bound.validate_request(&REQUEST),
        Err(SearchPlanApprovalError::InvalidOrExpired)
    );
    assert_eq!(
        next_bound.validate_request(&next),
        Err(SearchPlanApprovalError::InvalidOrExpired)
    );
}

#[test]
fn binding_requires_the_matching_plan_or_an_unambiguous_direct_policy() {
    let (_authority, reference, approval) = confirmed();
    assert_eq!(
        BoundSearchAuthorization::bind(
            &REQUEST,
            SearchPlanPolicy::RequireConfirmation,
            Some(&reference),
            None
        )
        .unwrap_err(),
        SearchPlanApprovalError::ConfirmationRequired,
    );
    assert_eq!(
        BoundSearchAuthorization::bind(
            &REQUEST,
            SearchPlanPolicy::RequireConfirmation,
            Some("other"),
            Some(approval.clone())
        )
        .unwrap_err(),
        SearchPlanApprovalError::ScopeMismatch,
    );
    for (reference, approval) in [
        (Some(reference.as_str()), None),
        (None, Some(approval.clone())),
        (Some(reference.as_str()), Some(approval)),
    ] {
        assert_eq!(
            BoundSearchAuthorization::bind(
                &REQUEST,
                SearchPlanPolicy::UserRequestedDirect,
                reference,
                approval
            )
            .unwrap_err(),
            SearchPlanApprovalError::ScopeMismatch,
        );
    }
    let direct =
        BoundSearchAuthorization::bind(&REQUEST, SearchPlanPolicy::UserRequestedDirect, None, None)
            .unwrap();
    direct.validate_request(&REQUEST).unwrap();
    assert_eq!(
        direct.validate_request(&Request {
            query: "other",
            ..REQUEST
        }),
        Err(SearchPlanApprovalError::ScopeMismatch)
    );
}

#[test]
fn identical_json_cannot_reuse_authority_for_a_different_operation_input() {
    #[derive(Serialize)]
    struct Channels {
        query: &'static str,
    }
    #[derive(Serialize)]
    struct Videos {
        query: &'static str,
    }
    let channels = Channels { query: "rust" };
    let videos = Videos { query: "rust" };
    assert_eq!(
        serde_json::to_vec(&channels).unwrap(),
        serde_json::to_vec(&videos).unwrap()
    );
    let (_authority, reference, approval) = confirmed();
    for (policy, reference, approval) in [
        (SearchPlanPolicy::UserRequestedDirect, None, None),
        (
            SearchPlanPolicy::RequireConfirmation,
            Some(reference.as_str()),
            Some(approval),
        ),
    ] {
        let bound = BoundSearchAuthorization::bind(&channels, policy, reference, approval).unwrap();
        bound.validate_request(&channels).unwrap();
        assert_eq!(
            bound.validate_request(&videos),
            Err(SearchPlanApprovalError::ScopeMismatch)
        );
    }
}
