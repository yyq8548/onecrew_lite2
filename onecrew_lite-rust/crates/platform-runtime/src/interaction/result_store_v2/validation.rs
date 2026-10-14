use std::collections::HashSet;

use serde::Serialize;

use super::{
    CallDisposition, CancelResultInput, ExecutedBatch, ExecutedResultInput, InitialResultState,
    MAX_ACTION_OPTIONS, MAX_ACTION_QUESTIONS, MAX_BATCH_OUTCOMES, MAX_PRESENTATION_KIND_BYTES,
    MAX_PROJECTION_BYTES, MAX_PROJECTION_SECTIONS, MAX_RESOURCE_REFS, MAX_SECTION_ITEMS,
    MAX_STRING_BYTES, PendingUserAction, PresentationProjection, PresentationSection,
    ResultChoiceAnswer, ResultChoiceQuestion, ResultContinuationContext, ResultContinuationRequest,
    ResultFailure, ResultFingerprint, ResultOrigin, ResultRef, ResultResolution, ResultState,
    ResultStoreError, ResultTerminalUpdate, ResultUnknown, ResultViewBinding, ResultViewKind,
    SubmittedActionView, TerminalResult,
};

pub(super) fn validate_executed_result(
    input: &ExecutedResultInput,
) -> Result<(), ResultStoreError> {
    let max_resource_refs = if input
        .pending_action
        .as_ref()
        .is_some_and(|action| action.payload().owner_review_context().is_some())
    {
        super::MAX_OWNER_FRAME_RESOURCE_REFS
    } else {
        MAX_RESOURCE_REFS
    };
    validate_execution_frame(input, max_resource_refs)
}

pub(super) fn validate_execution_frame(
    input: &ExecutedResultInput,
    max_resource_refs: usize,
) -> Result<(), ResultStoreError> {
    validate_execution_frame_with_prefix(input, max_resource_refs, 0)
}

pub(super) fn validate_execution_frame_with_prefix(
    input: &ExecutedResultInput,
    max_resource_refs: usize,
    completed_sections_len: usize,
) -> Result<(), ResultStoreError> {
    validate_batch(&input.batch, max_resource_refs)?;
    let current_sections = input
        .presentation
        .sections
        .get(completed_sections_len..)
        .ok_or(ResultStoreError::InvalidInput("completed_sections"))?;
    if matches!(
        input.pending_action,
        Some(PendingUserAction::CreatorSaveBatch { .. })
    ) {
        validate_projection_sections(
            &input.presentation,
            current_sections,
            super::MAX_CREATOR_SAVE_BATCH_PROJECTION_BYTES,
            super::MAX_CREATOR_SAVE_BATCH_SECTIONS,
        )?;
    } else {
        validate_projection_sections(
            &input.presentation,
            current_sections,
            MAX_PROJECTION_BYTES,
            MAX_PROJECTION_SECTIONS,
        )?;
    }
    if completed_sections_len > 0 {
        validate_batch_projection(&input.presentation)?;
    }
    validate_pending_action(input.pending_action.as_ref())?;
    if let Some(pending) = &input.pending_action {
        pending
            .payload()
            .validate_owner_review_context(&input.origin, &input.batch)?;
    }
    if let Some(PendingUserAction::CreatorSaveBatch { payload, .. }) = &input.pending_action {
        payload.validate_creator_save_context(&input.origin, &input.batch)?;
    }
    match (&input.initial_state, input.view_kind, &input.pending_action) {
        (InitialResultState::Available, ResultViewKind::ReadOnly, None)
        | (
            InitialResultState::Available,
            ResultViewKind::Confirmation,
            Some(
                PendingUserAction::Confirm { .. }
                | PendingUserAction::SearchPlan { .. }
                | PendingUserAction::CreatorSaveBatch { .. },
            ),
        )
        | (
            InitialResultState::Available,
            ResultViewKind::Choice,
            Some(PendingUserAction::Choice { .. }),
        )
        | (InitialResultState::Resolved(_), _, None)
        | (InitialResultState::Failed(_), _, None)
        | (InitialResultState::Unknown(_), _, None) => {}
        _ => return Err(ResultStoreError::InvalidInput("result_view_action")),
    }
    match &input.initial_state {
        InitialResultState::Resolved(resolution) => validate_resolution(resolution)?,
        InitialResultState::Failed(failure) => validate_failure(failure)?,
        InitialResultState::Unknown(unknown) => validate_unknown(unknown)?,
        InitialResultState::Available => {}
    }
    Ok(())
}

fn validate_batch(batch: &ExecutedBatch, max_resource_refs: usize) -> Result<(), ResultStoreError> {
    if batch.outcomes.is_empty() || batch.outcomes.len() > MAX_BATCH_OUTCOMES {
        return Err(ResultStoreError::InvalidInput("batch_outcomes"));
    }
    let mut call_ids = HashSet::with_capacity(batch.outcomes.len());
    for outcome in &batch.outcomes {
        validate_identifier(&outcome.call_id, 128, "call_id")?;
        validate_identifier(&outcome.operation_id, 128, "operation_id")?;
        validate_text(&outcome.summary, "outcome_summary")?;
        if !call_ids.insert(outcome.call_id.as_str()) {
            return Err(ResultStoreError::InvalidInput("duplicate_call_id"));
        }
        match &outcome.disposition {
            CallDisposition::Failed { code } | CallDisposition::Unknown { code } => {
                validate_identifier(code, 128, "outcome_code")?;
            }
            CallDisposition::NotRun { reason } => validate_text(reason, "not_run_reason")?,
            CallDisposition::Succeeded => {}
        }
        if outcome.resource_refs.len() > max_resource_refs {
            return Err(ResultStoreError::InvalidInput("resource_refs"));
        }
        for resource_ref in &outcome.resource_refs {
            validate_identifier(&resource_ref.id, 256, "resource_ref")?;
            if let Some(label) = &resource_ref.label {
                validate_text(label, "resource_label")?;
            }
        }
    }
    Ok(())
}

pub(super) fn validate_projection(
    projection: &PresentationProjection,
) -> Result<(), ResultStoreError> {
    validate_projection_with_limit(projection, MAX_PROJECTION_BYTES, MAX_PROJECTION_SECTIONS)
}

fn validate_projection_with_limit(
    projection: &PresentationProjection,
    max_bytes: usize,
    max_sections: usize,
) -> Result<(), ResultStoreError> {
    validate_projection_sections(projection, &projection.sections, max_bytes, max_sections)
}

fn validate_projection_sections(
    projection: &PresentationProjection,
    sections: &[PresentationSection],
    max_bytes: usize,
    max_sections: usize,
) -> Result<(), ResultStoreError> {
    validate_identifier(
        projection.kind.as_str(),
        MAX_PRESENTATION_KIND_BYTES,
        "presentation_kind",
    )?;
    validate_text(&projection.title, "projection_title")?;
    if let Some(summary) = &projection.summary {
        validate_text(summary, "projection_summary")?;
    }
    if sections.len() > max_sections {
        return Err(ResultStoreError::InvalidInput("projection_sections"));
    }
    for section in sections {
        match section {
            PresentationSection::CreatorSaveRows { rows } => validate_creator_save_outcomes(rows)?,
            PresentationSection::Fields { heading, fields } => {
                validate_optional_text(heading.as_deref(), "section_heading")?;
                if fields.is_empty() || fields.len() > MAX_SECTION_ITEMS {
                    return Err(ResultStoreError::InvalidInput("projection_fields"));
                }
                for field in fields {
                    validate_text(&field.label, "field_label")?;
                    validate_safe_field_label(&field.label)?;
                    validate_text(&field.value, "field_value")?;
                }
            }
            PresentationSection::Items { heading, items } => {
                validate_optional_text(heading.as_deref(), "section_heading")?;
                if items.is_empty() || items.len() > MAX_SECTION_ITEMS {
                    return Err(ResultStoreError::InvalidInput("projection_items"));
                }
                for item in items {
                    validate_text(&item.title, "item_title")?;
                    validate_optional_text(item.description.as_deref(), "item_description")?;
                }
            }
            PresentationSection::Notice { heading, body, .. } => {
                validate_optional_text(heading.as_deref(), "section_heading")?;
                validate_text(body, "notice_body")?;
            }
        }
    }
    #[derive(Serialize)]
    struct ProjectionSlice<'a> {
        kind: &'a super::ResultPresentationKind,
        title: &'a str,
        summary: &'a Option<String>,
        sections: &'a [PresentationSection],
    }
    let bytes = serde_json::to_vec(&ProjectionSlice {
        kind: &projection.kind,
        title: &projection.title,
        summary: &projection.summary,
        sections,
    })
    .map_err(|_| ResultStoreError::Unavailable("projection_encode"))?;
    if bytes.len() > max_bytes {
        return Err(ResultStoreError::InvalidInput("projection_bytes"));
    }
    Ok(())
}

pub(super) fn validate_batch_projection(
    projection: &PresentationProjection,
) -> Result<(), ResultStoreError> {
    validate_projection_with_limit(
        projection,
        super::MAX_OWNER_BATCH_PROJECTION_BYTES,
        super::MAX_OWNER_BATCH_SECTIONS,
    )
}

pub(super) fn validate_initial_completed_sections(
    projection: &PresentationProjection,
    count: usize,
) -> Result<(), ResultStoreError> {
    let sections = projection
        .sections
        .get(..count)
        .ok_or(ResultStoreError::InvalidInput("completed_sections"))?;
    validate_projection_sections(
        projection,
        sections,
        MAX_PROJECTION_BYTES,
        MAX_PROJECTION_SECTIONS,
    )
}

pub(super) fn validate_terminal_append(
    projection: &PresentationProjection,
    completed_sections_len: usize,
    owner_current: bool,
) -> Result<(), ResultStoreError> {
    let current =
        projection
            .sections
            .get(completed_sections_len..)
            .ok_or(ResultStoreError::InvalidInput(
                "completed_sections_mismatch",
            ))?;
    let (bytes, sections) = if owner_current {
        (
            super::MAX_CREATOR_SAVE_BATCH_PROJECTION_BYTES,
            super::MAX_CREATOR_SAVE_BATCH_SECTIONS,
        )
    } else {
        (MAX_PROJECTION_BYTES, MAX_PROJECTION_SECTIONS)
    };
    validate_projection_sections(projection, current, bytes, sections)?;
    validate_batch_projection(projection)
}

pub(super) fn validate_completed_append(
    projection: &PresentationProjection,
    old_count: usize,
    new_count: usize,
    owner_current: bool,
) -> Result<(), ResultStoreError> {
    let appended =
        projection
            .sections
            .get(old_count..new_count)
            .ok_or(ResultStoreError::InvalidInput(
                "completed_sections_mismatch",
            ))?;
    let (bytes, sections) = if owner_current {
        (
            super::MAX_CREATOR_SAVE_BATCH_PROJECTION_BYTES,
            super::MAX_CREATOR_SAVE_BATCH_SECTIONS,
        )
    } else {
        (MAX_PROJECTION_BYTES, MAX_PROJECTION_SECTIONS)
    };
    validate_projection_sections(projection, appended, usize::MAX, sections)?;
    if serde_json::to_vec(appended)
        .map_err(|_| ResultStoreError::Unavailable("projection_encode"))?
        .len()
        > bytes
    {
        return Err(ResultStoreError::InvalidInput("completed_projection_bytes"));
    }
    Ok(())
}

fn validate_pending_action(action: Option<&PendingUserAction>) -> Result<(), ResultStoreError> {
    let Some(action) = action else {
        return Ok(());
    };
    match action {
        PendingUserAction::CreatorSaveBatch { review, payload } => {
            payload.validate_creator_save_review(review)?;
            if payload.bytes().len() > super::MAX_CREATOR_SAVE_BATCH_OWNER_ACTION_BYTES
                || !crate::creator_save::valid_save_batch_size(
                    review.rows.len() + review.failures.len(),
                )
                || serde_json::to_vec(review)
                    .map_err(|_| ResultStoreError::InvalidInput("creator_save_batch"))?
                    .len()
                    > super::MAX_CREATOR_SAVE_BATCH_PROJECTION_BYTES
            {
                return Err(ResultStoreError::InvalidInput("creator_save_batch"));
            }
            let mut ids = HashSet::new();
            for row in &review.rows {
                validate_identifier(&row.row_id, 128, "creator_save_row")?;
                if !ids.insert(&row.row_id) {
                    return Err(ResultStoreError::InvalidInput("creator_save_duplicate_row"));
                }
                // The public failure is deliberately coarse. Only this log says
                // which row and which rule lost the card, or the next report of
                // a rejected save starts from nothing again. A prepared row
                // identifier is generated here and names no user content.
                if let Err(error) = validate_creator_save_preview(&row.preview) {
                    log::warn!("creator save review rejected row {} ({error})", row.row_id);
                    return Err(error);
                }
                if row.preview.intent != review.intent {
                    return Err(ResultStoreError::InvalidInput("creator_save_review_intent"));
                }
            }
            for failure in &review.failures {
                validate_identifier(&failure.row_id, 128, "creator_save_row")?;
                if !ids.insert(&failure.row_id) {
                    return Err(ResultStoreError::InvalidInput("creator_save_duplicate_row"));
                }
                validate_optional_text(
                    failure.display_name.as_deref(),
                    "creator_save_display_name",
                )?;
                validate_optional_text(failure.handle.as_deref(), "creator_save_handle")?;
                validate_optional_text(failure.profile_url.as_deref(), "creator_save_profile_url")?;
                if failure.reason
                    != onecrew_creator_runtime::CreatorSavePreparedReason::PreparationFailed
                {
                    return Err(ResultStoreError::InvalidInput(
                        "creator_save_failure_reason",
                    ));
                }
            }
            validate_creator_save_preparation_summary(review)?;
        }
        PendingUserAction::SearchPlan { draft, payload } => {
            let compiled = super::OwnerActionPayload::from_search_plan(draft)?;
            if &compiled != payload {
                return Err(ResultStoreError::InvalidInput(
                    "search_plan_payload_mismatch",
                ));
            }
        }
        PendingUserAction::Confirm {
            prompt,
            label,
            payload,
        } => {
            if payload.action_bytes_len() > payload.confirmation_payload_limit() {
                return Err(ResultStoreError::InvalidInput("owner_action_payload"));
            }
            validate_text(prompt, "confirmation_prompt")?;
            validate_text(label, "confirmation_label")?;
        }
        PendingUserAction::Choice { questions, payload } => {
            if payload.action_bytes_len() > super::MAX_OWNER_ACTION_BYTES {
                return Err(ResultStoreError::InvalidInput("owner_action_payload"));
            }
            if questions.is_empty() || questions.len() > MAX_ACTION_QUESTIONS {
                return Err(ResultStoreError::InvalidInput("choice_questions"));
            }
            let mut question_ids = HashSet::with_capacity(questions.len());
            for question in questions {
                validate_identifier(&question.id, 128, "choice_question_id")?;
                validate_text(&question.header, "choice_question_header")?;
                validate_text(&question.prompt, "choice_question_prompt")?;
                if !question_ids.insert(question.id.as_str()) {
                    return Err(ResultStoreError::InvalidInput("duplicate_choice_question"));
                }
                if question.options.is_empty() || question.options.len() > MAX_ACTION_OPTIONS {
                    return Err(ResultStoreError::InvalidInput("choice_options"));
                }
                let mut option_ids = HashSet::with_capacity(question.options.len());
                for option in &question.options {
                    validate_identifier(&option.id, 128, "choice_option_id")?;
                    validate_text(&option.label, "choice_option_label")?;
                    validate_optional_text(
                        option.description.as_deref(),
                        "choice_option_description",
                    )?;
                    if !option_ids.insert(option.id.as_str()) {
                        return Err(ResultStoreError::InvalidInput("duplicate_choice_option"));
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_creator_save_preparation_summary(
    review: &super::CreatorSaveBatchReview,
) -> Result<(), ResultStoreError> {
    let summary = &review.summary;
    if summary.total != review.rows.len() + review.failures.len()
        || summary.total
            != summary.eligible_new
                + summary.eligible_update
                + summary.existing_skipped
                + summary.duplicate_skipped
                + summary.blocked
                + summary.failed
    {
        return Err(ResultStoreError::InvalidInput(
            "creator_save_review_summary",
        ));
    }
    let mut row_counts = [0usize; 6];
    for row in &review.rows {
        match row.preview.status {
            onecrew_creator_runtime::CreatorSavePreparedStatus::EligibleNew => row_counts[0] += 1,
            onecrew_creator_runtime::CreatorSavePreparedStatus::EligibleUpdate => {
                row_counts[1] += 1
            }
            onecrew_creator_runtime::CreatorSavePreparedStatus::ExistingSkipped => {
                row_counts[2] += 1
            }
            onecrew_creator_runtime::CreatorSavePreparedStatus::DuplicateSkipped => {
                row_counts[3] += 1
            }
            onecrew_creator_runtime::CreatorSavePreparedStatus::IdentityPending
            | onecrew_creator_runtime::CreatorSavePreparedStatus::ContactConflict => {
                row_counts[4] += 1
            }
            onecrew_creator_runtime::CreatorSavePreparedStatus::Failed => row_counts[5] += 1,
        }
    }
    let expected = [
        row_counts[0],
        row_counts[1],
        row_counts[2],
        row_counts[3],
        row_counts[4],
        row_counts[5] + review.failures.len(),
    ];
    let reported = [
        summary.eligible_new,
        summary.eligible_update,
        summary.existing_skipped,
        summary.duplicate_skipped,
        summary.blocked,
        summary.failed,
    ];
    if reported != expected {
        return Err(ResultStoreError::InvalidInput(
            "creator_save_review_summary",
        ));
    }
    Ok(())
}

fn validate_creator_save_preview(
    preview: &super::CreatorSavePreview,
) -> Result<(), ResultStoreError> {
    if serde_json::to_vec(preview)
        .map_err(|_| ResultStoreError::InvalidInput("creator_save_preview"))?
        .len()
        > MAX_PROJECTION_BYTES
        || preview.routes.is_empty()
        || preview.routes.len() > super::MAX_PRESENTATION_PREVIEW_ROUTES
        || !preview.routes.iter().any(|route| route.required)
    {
        return Err(ResultStoreError::InvalidInput("creator_save_preview"));
    }
    let mut ids = HashSet::new();
    for route in &preview.routes {
        validate_identifier(&route.route_id, 128, "creator_save_route")?;
        if !ids.insert(&route.route_id) {
            return Err(ResultStoreError::InvalidInput("creator_save_route"));
        }
    }
    let intent_valid = !matches!(
        (preview.intent, preview.status),
        (
            onecrew_creator_runtime::CreatorSaveIntent::AddNew,
            onecrew_creator_runtime::CreatorSavePreparedStatus::EligibleUpdate,
        ) | (
            onecrew_creator_runtime::CreatorSaveIntent::SaveOrUpdate,
            onecrew_creator_runtime::CreatorSavePreparedStatus::ExistingSkipped,
        )
    );
    let reason_valid = match preview.status {
        onecrew_creator_runtime::CreatorSavePreparedStatus::EligibleNew
        | onecrew_creator_runtime::CreatorSavePreparedStatus::EligibleUpdate => {
            preview.reason.is_none()
        }
        onecrew_creator_runtime::CreatorSavePreparedStatus::ExistingSkipped => {
            preview.reason
                == Some(onecrew_creator_runtime::CreatorSavePreparedReason::ExistingCreator)
        }
        onecrew_creator_runtime::CreatorSavePreparedStatus::DuplicateSkipped => {
            preview.reason
                == Some(onecrew_creator_runtime::CreatorSavePreparedReason::DuplicateSelection)
        }
        onecrew_creator_runtime::CreatorSavePreparedStatus::IdentityPending => matches!(
            preview.reason,
            Some(
                onecrew_creator_runtime::CreatorSavePreparedReason::IdentityAmbiguous
                    | onecrew_creator_runtime::CreatorSavePreparedReason::ConflictingUpdate
            )
        ),
        onecrew_creator_runtime::CreatorSavePreparedStatus::ContactConflict => preview.reason
            == Some(
                onecrew_creator_runtime::CreatorSavePreparedReason::RequiredContactRouteConflict,
            ),
        onecrew_creator_runtime::CreatorSavePreparedStatus::Failed => {
            preview.reason
                == Some(onecrew_creator_runtime::CreatorSavePreparedReason::PreparationFailed)
        }
    };
    if !intent_valid {
        return Err(ResultStoreError::InvalidInput(
            "creator_save_preview_intent",
        ));
    }
    if !reason_valid {
        return Err(ResultStoreError::InvalidInput(
            "creator_save_preview_reason",
        ));
    }
    Ok(())
}

fn validate_creator_save_outcomes(
    rows: &[super::CreatorSaveBatchOutcome],
) -> Result<(), ResultStoreError> {
    if !crate::creator_save::valid_save_batch_size(rows.len()) {
        return Err(ResultStoreError::InvalidInput(
            "creator_save_batch_outcomes",
        ));
    }
    let mut ids = HashSet::new();
    for row in rows {
        validate_identifier(&row.row_id, 128, "creator_save_row")?;
        if !ids.insert(&row.row_id) {
            return Err(ResultStoreError::InvalidInput("creator_save_duplicate_row"));
        }
        if row.confirmed_route_ids.len() > 128 {
            return Err(ResultStoreError::InvalidInput("creator_save_routes"));
        }
        for id in &row.confirmed_route_ids {
            validate_identifier(id, 128, "creator_save_route")?;
        }
        if let Some(id) = &row.creator_id {
            validate_identifier(id, 256, "creator_save_creator")?;
        }
        if (row.status == super::CreatorSaveBatchRowStatus::Saved) != row.creator_id.is_some() {
            return Err(ResultStoreError::InvalidInput(
                "creator_save_saved_resource",
            ));
        }
        if !creator_save_operation_valid(row.status.clone(), row.operation, row.reason) {
            return Err(ResultStoreError::InvalidInput(
                "creator_save_outcome_operation",
            ));
        }
    }
    Ok(())
}

fn validate_continuation_creator_outcomes(
    rows: &[super::ResultContinuationCreatorSaveOutcome],
) -> Result<(), ResultStoreError> {
    if !crate::creator_save::valid_save_batch_size(rows.len()) {
        return Err(ResultStoreError::InvalidInput(
            "creator_save_batch_outcomes",
        ));
    }
    let mut row_ids = HashSet::new();
    for row in rows {
        validate_identifier(&row.row_id, 128, "creator_save_row")?;
        if !row_ids.insert(&row.row_id) {
            return Err(ResultStoreError::InvalidInput("creator_save_duplicate_row"));
        }
        if let Some(id) = &row.creator_id {
            validate_identifier(id, 256, "creator_save_creator")?;
        }
        if let Some(code) = &row.failure_code {
            validate_identifier(code, 128, "creator_save_failure_code")?;
        }
        if (row.status == super::CreatorSaveBatchRowStatus::Saved) != row.creator_id.is_some() {
            return Err(ResultStoreError::InvalidInput(
                "creator_save_saved_resource",
            ));
        }
        if !creator_save_operation_valid(row.status.clone(), row.operation, row.reason) {
            return Err(ResultStoreError::InvalidInput(
                "creator_save_outcome_operation",
            ));
        }
    }
    Ok(())
}

fn creator_save_operation_valid(
    status: super::CreatorSaveBatchRowStatus,
    operation: super::CreatorSaveBatchOperation,
    reason: Option<super::CreatorSaveBatchReason>,
) -> bool {
    match operation {
        super::CreatorSaveBatchOperation::Added | super::CreatorSaveBatchOperation::Updated => {
            status == super::CreatorSaveBatchRowStatus::Saved && reason.is_none()
        }
        super::CreatorSaveBatchOperation::ExistingSkipped
        | super::CreatorSaveBatchOperation::DuplicateSkipped => {
            matches!(
                status,
                super::CreatorSaveBatchRowStatus::NotRun
                    | super::CreatorSaveBatchRowStatus::NotSelected
            ) && reason
                == Some(match operation {
                    super::CreatorSaveBatchOperation::ExistingSkipped => {
                        super::CreatorSaveBatchReason::ExistingCreator
                    }
                    super::CreatorSaveBatchOperation::DuplicateSkipped => {
                        super::CreatorSaveBatchReason::DuplicateSelection
                    }
                    _ => unreachable!("operation arm is exhaustive"),
                })
        }
        super::CreatorSaveBatchOperation::NotSelected => {
            status == super::CreatorSaveBatchRowStatus::NotSelected
                && reason == Some(super::CreatorSaveBatchReason::UserNotSelected)
        }
        super::CreatorSaveBatchOperation::Failed => {
            matches!(
                status,
                super::CreatorSaveBatchRowStatus::Failed | super::CreatorSaveBatchRowStatus::NotRun
            ) && matches!(
                reason,
                Some(
                    super::CreatorSaveBatchReason::IdentityAmbiguous
                        | super::CreatorSaveBatchReason::RequiredContactRouteConflict
                        | super::CreatorSaveBatchReason::ConflictingUpdate
                        | super::CreatorSaveBatchReason::PreparationFailed
                        | super::CreatorSaveBatchReason::RowFailed
                )
            )
        }
        super::CreatorSaveBatchOperation::Unknown => {
            status == super::CreatorSaveBatchRowStatus::Unknown
                && reason == Some(super::CreatorSaveBatchReason::CommitUnknown)
        }
    }
}

pub(super) fn validate_choice_answer(answer: &ResultChoiceAnswer) -> Result<(), ResultStoreError> {
    validate_identifier(answer.question_id(), 128, "choice_answer_question_id")?;
    match answer {
        ResultChoiceAnswer::Options { option_ids, .. } => {
            if option_ids.is_empty() || option_ids.len() > MAX_ACTION_OPTIONS {
                return Err(ResultStoreError::InvalidInput("choice_answer_option_count"));
            }
            let mut selected = HashSet::with_capacity(option_ids.len());
            for option_id in option_ids {
                validate_identifier(option_id, 128, "choice_answer_option_id")?;
                if !selected.insert(option_id) {
                    return Err(ResultStoreError::InvalidInput(
                        "duplicate_choice_answer_option",
                    ));
                }
            }
            Ok(())
        }
        ResultChoiceAnswer::Option { option_id, .. } => {
            validate_identifier(option_id, 128, "choice_answer_option_id")
        }
        ResultChoiceAnswer::Custom { text, .. } => {
            if text.trim().is_empty()
                || text.len() > 2048
                || text
                    .chars()
                    .any(|c| c.is_control() && c != '\n' && c != '\t')
            {
                return Err(ResultStoreError::InvalidInput("choice_answer_text"));
            }
            Ok(())
        }
    }
}

fn validate_choice_answer_bytes(answers: &[ResultChoiceAnswer]) -> Result<(), ResultStoreError> {
    if serde_json::to_vec(answers)
        .map_err(|_| ResultStoreError::InvalidInput("choice_answers_encoding"))?
        .len()
        > 24 * 1024
    {
        return Err(ResultStoreError::InvalidInput("choice_answers_too_large"));
    }
    Ok(())
}

pub(super) fn validate_choice_answers(
    questions: &[ResultChoiceQuestion],
    answers: &[ResultChoiceAnswer],
) -> Result<Vec<ResultChoiceAnswer>, ResultStoreError> {
    if answers.len() != questions.len() {
        return Err(ResultStoreError::InvalidInput("choice_answer_count"));
    }
    let mut answered_questions = HashSet::with_capacity(answers.len());
    for answer in answers {
        validate_choice_answer(answer)?;
        if !answered_questions.insert(answer.question_id()) {
            return Err(ResultStoreError::InvalidInput("duplicate_choice_answer"));
        }
        if !questions
            .iter()
            .any(|question| question.id == answer.question_id())
        {
            return Err(ResultStoreError::InvalidInput("unknown_choice_question"));
        }
    }

    validate_choice_answer_bytes(answers)?;
    questions
        .iter()
        .map(|question| {
            let answer = answers
                .iter()
                .find(|answer| answer.question_id() == question.id)
                .ok_or(ResultStoreError::InvalidInput("missing_choice_answer"))?;
            if !answer.matches_question(question) {
                return Err(ResultStoreError::InvalidInput("unknown_choice_option"));
            }
            Ok(match answer {
                ResultChoiceAnswer::Options {
                    question_id,
                    option_ids,
                } => ResultChoiceAnswer::Options {
                    question_id: question_id.clone(),
                    option_ids: question
                        .options
                        .iter()
                        .filter(|option| option_ids.contains(&option.id))
                        .map(|option| option.id.clone())
                        .collect(),
                },
                _ => answer.clone(),
            })
        })
        .collect()
}

pub(super) fn validate_terminal(terminal: &TerminalResult) -> Result<(), ResultStoreError> {
    match terminal {
        TerminalResult::Resolved(resolution) => validate_resolution(resolution),
        TerminalResult::Failed(failure) => validate_failure(failure),
        TerminalResult::Unknown(unknown) => validate_unknown(unknown),
    }
}

pub(super) fn validate_terminal_update(
    update: &ResultTerminalUpdate,
) -> Result<(), ResultStoreError> {
    let creator_save = update
        .presentation
        .sections
        .iter()
        .any(|section| matches!(section, PresentationSection::CreatorSaveRows { .. }));
    let max_resource_refs = if creator_save {
        super::MAX_OWNER_FRAME_RESOURCE_REFS
    } else {
        MAX_RESOURCE_REFS
    };
    validate_terminal_frame(update, max_resource_refs)
}

pub(super) fn validate_terminal_frame(
    update: &ResultTerminalUpdate,
    max_resource_refs: usize,
) -> Result<(), ResultStoreError> {
    validate_terminal_frame_with_prefix(update, max_resource_refs, false)
}

pub(super) fn validate_terminal_frame_with_prefix(
    update: &ResultTerminalUpdate,
    max_resource_refs: usize,
    retained_owner_sections: bool,
) -> Result<(), ResultStoreError> {
    validate_terminal(&update.terminal)?;
    validate_batch(&update.batch, max_resource_refs)?;
    let max_bytes = if update
        .presentation
        .sections
        .iter()
        .any(|section| matches!(section, PresentationSection::CreatorSaveRows { .. }))
    {
        super::MAX_CREATOR_SAVE_BATCH_PROJECTION_BYTES
    } else {
        MAX_PROJECTION_BYTES
    };
    if retained_owner_sections {
        validate_batch_projection(&update.presentation)?;
    } else {
        validate_projection_with_limit(&update.presentation, max_bytes, MAX_PROJECTION_SECTIONS)?;
    }
    if let Some(request) = &update.continuation {
        validate_continuation_request(request)?;
    }
    Ok(())
}

pub(super) fn terminal_update_payload_bytes(
    update: &ResultTerminalUpdate,
) -> Result<usize, ResultStoreError> {
    let state = match &update.terminal {
        TerminalResult::Resolved(resolution) => ResultState::Resolved(resolution.clone()),
        TerminalResult::Failed(failure) => ResultState::Failed(failure.clone()),
        TerminalResult::Unknown(unknown) => ResultState::Unknown(unknown.clone()),
    };
    stored_terminal_payload_bytes(
        &update.batch,
        &update.presentation,
        &state,
        update.continuation.as_ref(),
    )
}

pub(super) fn stored_terminal_payload_bytes(
    batch: &ExecutedBatch,
    presentation: &PresentationProjection,
    state: &ResultState,
    continuation: Option<&ResultContinuationRequest>,
) -> Result<usize, ResultStoreError> {
    serde_json::to_vec(&(batch, presentation, state, continuation))
        .map_err(|_| ResultStoreError::Unavailable("result_payload_size"))
        .map(|bytes| bytes.len())
}

pub(super) fn validate_continuation_for_action(
    action: &SubmittedActionView,
    request: Option<&ResultContinuationRequest>,
) -> Result<(), ResultStoreError> {
    let Some(request) = request else {
        return Ok(());
    };
    match (action, &request.context) {
        (SubmittedActionView::Confirmed {}, ResultContinuationContext::Choice { .. }) => Err(
            ResultStoreError::InvalidInput("confirmation_continuation_context"),
        ),
        (
            SubmittedActionView::Selected { answers: selected },
            ResultContinuationContext::Choice { answers },
        ) if selected == answers => Ok(()),
        (SubmittedActionView::Selected { .. }, ResultContinuationContext::Choice { .. }) => Err(
            ResultStoreError::InvalidInput("choice_continuation_answers"),
        ),
        (SubmittedActionView::Selected { .. }, _) => Err(ResultStoreError::InvalidInput(
            "choice_continuation_context",
        )),
        (_, ResultContinuationContext::EditRequested) => {
            Err(ResultStoreError::InvalidInput("edit_continuation_context"))
        }
        _ => Ok(()),
    }
}

pub(super) fn validate_continuation_request(
    request: &ResultContinuationRequest,
) -> Result<(), ResultStoreError> {
    validate_text(&request.message, "continuation_message")?;
    match &request.context {
        ResultContinuationContext::SearchPlanConfirmed {
            confirmed_plan_ref,
            description,
            keywords,
        } => {
            validate_identifier(confirmed_plan_ref, 128, "confirmed_plan_ref")?;
            crate::SearchPlanEditorDraft::validate_criteria(description, keywords)
                .map_err(|_| ResultStoreError::InvalidInput("search_plan"))?;
        }
        ResultContinuationContext::CreatorSaveBatch { summary, outcomes } => {
            validate_continuation_creator_outcomes(outcomes)?;
            validate_creator_save_result_summary(summary, outcomes)?;
        }
        ResultContinuationContext::Execution { outcomes } => {
            if outcomes.is_empty() || outcomes.len() > MAX_BATCH_OUTCOMES {
                return Err(ResultStoreError::InvalidInput("continuation_outcomes"));
            }
            let mut call_ids = HashSet::with_capacity(outcomes.len());
            for outcome in outcomes {
                validate_identifier(&outcome.call_id, 128, "continuation_call_id")?;
                validate_identifier(&outcome.operation_id, 128, "continuation_operation_id")?;
                validate_text(&outcome.summary, "continuation_summary")?;
                if outcome.resource_refs.len() > MAX_RESOURCE_REFS {
                    return Err(ResultStoreError::InvalidInput("continuation_resource_refs"));
                }
                for resource in &outcome.resource_refs {
                    validate_identifier(&resource.id, 256, "continuation_resource_ref")?;
                    if let Some(label) = &resource.label {
                        validate_text(label, "continuation_resource_label")?;
                    }
                }
                if !call_ids.insert(outcome.call_id.as_str()) {
                    return Err(ResultStoreError::InvalidInput(
                        "duplicate_continuation_call_id",
                    ));
                }
                match &outcome.disposition {
                    CallDisposition::Failed { code } | CallDisposition::Unknown { code } => {
                        validate_identifier(code, 128, "continuation_outcome_code")?;
                    }
                    CallDisposition::NotRun { reason } => {
                        validate_text(reason, "continuation_not_run_reason")?;
                    }
                    CallDisposition::Succeeded => {}
                }
            }
        }
        ResultContinuationContext::Choice { answers } => {
            if answers.is_empty() || answers.len() > MAX_ACTION_QUESTIONS {
                return Err(ResultStoreError::InvalidInput(
                    "continuation_choice_answers",
                ));
            }
            let mut question_ids = HashSet::with_capacity(answers.len());
            for answer in answers {
                validate_choice_answer(answer)?;
                if !question_ids.insert(answer.question_id()) {
                    return Err(ResultStoreError::InvalidInput(
                        "duplicate_continuation_question",
                    ));
                }
            }
            validate_choice_answer_bytes(answers)?;
        }
        ResultContinuationContext::Cancelled | ResultContinuationContext::EditRequested => {}
    }
    Ok(())
}

fn validate_creator_save_result_summary(
    summary: &super::CreatorSaveResultSummary,
    outcomes: &[super::ResultContinuationCreatorSaveOutcome],
) -> Result<(), ResultStoreError> {
    if summary.total != outcomes.len() {
        return Err(ResultStoreError::InvalidInput("creator_save_summary_total"));
    }
    let mut actual = super::CreatorSaveResultSummary {
        total: outcomes.len(),
        ..super::CreatorSaveResultSummary::default()
    };
    for outcome in outcomes {
        match outcome.operation {
            super::CreatorSaveBatchOperation::Added => actual.added += 1,
            super::CreatorSaveBatchOperation::Updated => actual.updated += 1,
            super::CreatorSaveBatchOperation::ExistingSkipped => actual.existing_skipped += 1,
            super::CreatorSaveBatchOperation::DuplicateSkipped => actual.duplicate_skipped += 1,
            super::CreatorSaveBatchOperation::NotSelected => actual.not_selected += 1,
            super::CreatorSaveBatchOperation::Failed => actual.failed += 1,
            super::CreatorSaveBatchOperation::Unknown => actual.unknown += 1,
        }
    }
    if &actual != summary {
        return Err(ResultStoreError::InvalidInput(
            "creator_save_summary_counts",
        ));
    }
    Ok(())
}

pub(super) fn validate_cancel_result_input(
    input: &CancelResultInput,
) -> Result<(), ResultStoreError> {
    let Some(request) = &input.continuation else {
        return Ok(());
    };
    validate_continuation_request(request)?;
    if !matches!(&request.context, ResultContinuationContext::Cancelled) {
        return Err(ResultStoreError::InvalidInput(
            "cancel_continuation_context",
        ));
    }
    Ok(())
}

fn validate_resolution(resolution: &ResultResolution) -> Result<(), ResultStoreError> {
    validate_identifier(&resolution.code, 128, "resolution_code")?;
    validate_text(&resolution.summary, "resolution_summary")
}

fn validate_failure(failure: &ResultFailure) -> Result<(), ResultStoreError> {
    validate_identifier(&failure.code, 128, "failure_code")?;
    validate_text(&failure.summary, "failure_summary")
}

fn validate_unknown(unknown: &ResultUnknown) -> Result<(), ResultStoreError> {
    validate_identifier(&unknown.code, 128, "unknown_code")?;
    validate_text(&unknown.summary, "unknown_summary")
}

fn validate_text(value: &str, code: &'static str) -> Result<(), ResultStoreError> {
    if value.trim().is_empty() || value.len() > MAX_STRING_BYTES {
        return Err(ResultStoreError::InvalidInput(code));
    }
    Ok(())
}

fn validate_optional_text(value: Option<&str>, code: &'static str) -> Result<(), ResultStoreError> {
    if let Some(value) = value {
        validate_text(value, code)?;
    }
    Ok(())
}

pub(super) fn validate_identifier(
    value: &str,
    max_bytes: usize,
    code: &'static str,
) -> Result<(), ResultStoreError> {
    if value.is_empty()
        || value.len() > max_bytes
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
    {
        return Err(ResultStoreError::InvalidInput(code));
    }
    Ok(())
}

fn validate_safe_field_label(label: &str) -> Result<(), ResultStoreError> {
    let normalized: String = label
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    const FORBIDDEN: &[&str] = &[
        "secret",
        "password",
        "passwd",
        "token",
        "cookie",
        "authorization",
        "apikey",
        "privatekey",
        "clientsecret",
        "rawpayload",
        "rawresponse",
        "rawrequest",
    ];
    if normalized == "raw" || FORBIDDEN.iter().any(|needle| normalized.contains(needle)) {
        return Err(ResultStoreError::InvalidInput("secret_or_raw_field"));
    }
    Ok(())
}

#[derive(Serialize)]
struct ResultFingerprintMaterial<'a> {
    result_ref: &'a str,
    view: &'a ResultViewBinding,
    origin: &'a ResultOrigin,
    batch: &'a ExecutedBatch,
    presentation: &'a PresentationProjection,
    pending_action: Option<PendingActionFingerprint<'a>>,
    initial_state: &'a InitialResultState,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PendingActionFingerprint<'a> {
    CreatorSaveBatch {
        review: &'a super::CreatorSaveBatchReview,
        payload_digest: String,
    },
    SearchPlan {
        draft: &'a crate::SearchPlanEditorDraft,
        payload_digest: String,
    },
    Confirm {
        prompt: &'a str,
        label: &'a str,
        payload_digest: String,
    },
    Choice {
        questions: &'a [ResultChoiceQuestion],
        payload_digest: String,
    },
}

impl<'a> From<&'a PendingUserAction> for PendingActionFingerprint<'a> {
    fn from(action: &'a PendingUserAction) -> Self {
        match action {
            PendingUserAction::CreatorSaveBatch { review, payload } => Self::CreatorSaveBatch {
                review,
                payload_digest: payload.digest(),
            },
            PendingUserAction::SearchPlan { draft, payload } => Self::SearchPlan {
                draft,
                payload_digest: payload.digest(),
            },
            PendingUserAction::Confirm {
                prompt,
                label,
                payload,
            } => Self::Confirm {
                prompt,
                label,
                payload_digest: payload.digest(),
            },
            PendingUserAction::Choice { questions, payload } => Self::Choice {
                questions,
                payload_digest: payload.digest(),
            },
        }
    }
}

pub(super) fn fingerprint_result(
    result_ref: &ResultRef,
    view: &ResultViewBinding,
    input: &ExecutedResultInput,
) -> Result<ResultFingerprint, ResultStoreError> {
    let material = ResultFingerprintMaterial {
        result_ref: result_ref.as_str(),
        view,
        origin: &input.origin,
        batch: &input.batch,
        presentation: &input.presentation,
        pending_action: input
            .pending_action
            .as_ref()
            .map(PendingActionFingerprint::from),
        initial_state: &input.initial_state,
    };
    let bytes = serde_json::to_vec(&material)
        .map_err(|_| ResultStoreError::Unavailable("result_fingerprint"))?;
    Ok(ResultFingerprint::digest(&bytes))
}

pub(super) fn result_payload_bytes(input: &ExecutedResultInput) -> Result<usize, ResultStoreError> {
    let public_bytes = serde_json::to_vec(&(
        &input.batch,
        &input.presentation,
        input
            .pending_action
            .as_ref()
            .map(PendingActionFingerprint::from),
        &input.initial_state,
    ))
    .map_err(|_| ResultStoreError::Unavailable("result_payload_size"))?
    .len();
    let opaque_bytes = match &input.pending_action {
        Some(
            PendingUserAction::Confirm { payload, .. }
            | PendingUserAction::SearchPlan { payload, .. }
            | PendingUserAction::CreatorSaveBatch { payload, .. },
        ) => payload.held_bytes()?,
        Some(PendingUserAction::Choice { payload, .. }) => payload.held_bytes()?,
        None => 0,
    };
    public_bytes
        .checked_add(opaque_bytes)
        .ok_or(ResultStoreError::Capacity("result_payload_bytes"))
}
