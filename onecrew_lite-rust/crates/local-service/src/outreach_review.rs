//! Web singleton review projection; the runtime owns immutable intent and effects.
use crate::{ServiceState, onecrew_outreach::WebDirectMessageError, web_result::WebResultBinding};
use onecrew_local_runtime::{
    CallDisposition, CancelResultInput, ExecutedBatch, ExecutedCallOutcome, LocalPlatform,
    OutreachCompletion, OwnerCallOutput, OwnerReviewContext, OwnerReviewSelection,
    PreparedOwnerReview, ResultOrigin, ResultSnapshot,
};
use serde::Serialize;

#[derive(Serialize)]
pub(super) struct WebOutreachConfirmationResult {
    snapshot: ResultSnapshot,
    completion: Option<OutreachCompletion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure: Option<WebDirectMessageError>,
}

pub(super) fn context(
    origin: &ResultOrigin,
    operation_id: &'static str,
) -> Result<OwnerReviewContext, String> {
    OwnerReviewContext::new(
        origin.clone(),
        uuid::Uuid::new_v4().to_string(),
        operation_id,
        None,
    )
    .map_err(|error| error.to_string())
}

pub(super) fn present(
    platform: &LocalPlatform,
    review: PreparedOwnerReview,
) -> Result<ResultSnapshot, String> {
    let context = review.context();
    let origin = context.origin().clone();
    let frame = ExecutedBatch {
        outcomes: vec![ExecutedCallOutcome {
            call_id: context.call_id().to_owned(),
            operation_id: context.operation_id().to_owned(),
            disposition: CallDisposition::NotRun {
                reason: "confirmation_required".into(),
            },
            summary: "Review the prepared action before execution".into(),
            resource_refs: vec![],
        }],
    };
    let reference = platform
        .mint_owner_review(review, frame, None, vec![])
        .map_err(|error| error.to_string())?;
    platform
        .present_result(&reference, &origin)
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_outreach_result_get(
    state: &ServiceState,
    input: WebResultBinding,
    origin: &ResultOrigin,
) -> Result<ResultSnapshot, String> {
    state
        .platform()
        .await?
        .read_current_result(&input.bind(origin)?)
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_outreach_confirm(
    state: &ServiceState,
    input: WebResultBinding,
    origin: &ResultOrigin,
) -> Result<WebOutreachConfirmationResult, WebDirectMessageError> {
    let platform = state
        .platform()
        .await
        .map_err(|_| WebDirectMessageError::runtime_unavailable())?;
    let binding = input.bind(origin).map_err(|_| {
        WebDirectMessageError::local(
            "review_binding_invalid",
            "The outreach review binding is invalid",
        )
    })?;
    let grant = platform
        .submit_owner_review(&binding, OwnerReviewSelection::Confirm)
        .map_err(|_| {
            WebDirectMessageError::local(
                "review_submission_rejected",
                "The outreach review could not be confirmed",
            )
        })?;
    let submitted = grant.binding().clone();
    match platform.execute_submitted_owner(grant).await {
        Ok(completion) => {
            // Rows require their own selection and cannot be submitted with
            // Confirm. Never turn a different owner's output into outreach.
            let output = match &completion.output {
                OwnerCallOutput::Outreach(value) => Some(value.as_ref().clone()),
                OwnerCallOutput::CreatorSave(_)
                | OwnerCallOutput::Email(_)
                | OwnerCallOutput::CreatorBatch(_) => None,
            };
            let snapshot = platform
                .transition_result_terminal(&submitted, completion.into_singleton_terminal())
                .map_err(|_| result_unavailable())?;
            Ok(WebOutreachConfirmationResult {
                snapshot,
                completion: output,
                failure: None,
            })
        }
        Err(failure) => {
            let snapshot = platform
                .fail_singleton_owner_execution(&submitted, &failure)
                .map_err(|_| result_unavailable())?;
            Ok(WebOutreachConfirmationResult {
                snapshot,
                completion: None,
                failure: Some(project_owner_failure(failure)),
            })
        }
    }
}

fn project_owner_failure(
    failure: onecrew_local_runtime::OwnerExecutionFailure,
) -> WebDirectMessageError {
    let mut projected = WebDirectMessageError::local(failure.code, failure.message);
    projected.failure = failure.dm_failure;
    projected
}

fn result_unavailable() -> WebDirectMessageError {
    WebDirectMessageError::local(
        "outreach_result_unavailable",
        "The outreach result could not be displayed. Keep the intent key and inspect action status without resending.",
    )
}

pub(super) async fn onecrew_outreach_cancel(
    state: &ServiceState,
    input: WebResultBinding,
    origin: &ResultOrigin,
) -> Result<ResultSnapshot, String> {
    state
        .platform()
        .await?
        .cancel_result(CancelResultInput {
            binding: input.bind(origin)?,
            continuation: None,
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use onecrew_local_runtime::{
        DmDiagnostic, DmFailure, OwnerExecutionEffect, OwnerExecutionFailure,
    };

    #[test]
    fn confirmed_owner_failure_uses_the_dm_http_contract_without_reparsing_message() {
        for message in ["login 429 timeout", "sent successfully"] {
            let mut diagnostic = DmDiagnostic::provider("instagram", "ig_private_api", message);
            diagnostic.code = Some("challenge_required".into());
            diagnostic.http_status = Some(400);
            let failure = DmFailure {
                stage: "capability",
                diagnostic,
            };
            let projected = project_owner_failure(OwnerExecutionFailure {
                code: "outreach_preflight_rejected",
                message: "The current preflight could not complete.",
                effect: OwnerExecutionEffect::KnownNoEffect,
                dm_failure: Some(Box::new(failure.clone())),
            });
            let value = serde_json::to_value(projected).unwrap();
            assert_eq!(value["failure"], serde_json::to_value(failure).unwrap());
            assert_eq!(value["code"], "outreach_preflight_rejected");
        }
        let projected = project_owner_failure(OwnerExecutionFailure {
            code: "outreach_execution_unavailable",
            message: "Inspect the existing action status without resending.",
            effect: OwnerExecutionEffect::Unknown,
            dm_failure: None,
        });
        assert!(
            serde_json::to_value(projected)
                .unwrap()
                .get("failure")
                .is_none()
        );
    }
}
