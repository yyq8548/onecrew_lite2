use std::time::Instant;

use super::super::validation::stored_terminal_payload_bytes;
use super::super::{
    RequestResultEditInput, ResultContinuationContext, ResultContinuationRequest, ResultResolution,
    ResultSnapshot, ResultState, ResultStoreError,
};
use super::{ResultMemoryStore, expire_target, require_user_action_available, validate_binding};

const MAX_EDIT_INSTRUCTION_BYTES: usize = 2_048;
const EDIT_REQUESTED_CODE: &str = "edit_requested";
const EDIT_REQUESTED_SUMMARY: &str =
    "The user requested changes. The pending action was not executed.";

impl ResultMemoryStore {
    pub(in crate::interaction::result_store_v2) fn request_edit(
        &self,
        input: RequestResultEditInput,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        let instruction = normalize_instruction(&input.instruction)?;
        let final_state = ResultState::Resolved(ResultResolution {
            code: EDIT_REQUESTED_CODE.to_owned(),
            summary: EDIT_REQUESTED_SUMMARY.to_owned(),
        });
        let now = Instant::now();

        let mut map = self.lock()?;
        expire_target(&mut map, &input.binding.result_ref, self.limits, now)?;
        let (previous_payload_bytes, final_payload_bytes, next_revision, request) = {
            let record = map
                .records
                .get(&input.binding.result_ref)
                .ok_or(ResultStoreError::Absent)?;
            validate_binding(record, &input.binding)?;
            require_user_action_available(record)?;
            let request = edit_continuation(instruction, record.locale);
            record
                .continuation
                .revision
                .checked_add(1)
                .ok_or(ResultStoreError::Unavailable("continuation_revision"))?;
            let final_payload_bytes = stored_terminal_payload_bytes(
                &record.batch,
                &record.presentation,
                &final_state,
                Some(&request),
            )?;
            let next_revision = record
                .revision
                .checked_add(1)
                .ok_or(ResultStoreError::Unavailable("result_revision"))?;
            (
                record.payload_bytes,
                final_payload_bytes,
                next_revision,
                request,
            )
        };
        let next_total_payload_bytes = map
            .total_payload_bytes
            .checked_sub(previous_payload_bytes)
            .and_then(|retained| retained.checked_add(final_payload_bytes))
            .ok_or(ResultStoreError::Capacity("result_payload_limit"))?;
        if next_total_payload_bytes > self.limits.max_total_payload_bytes {
            return Err(ResultStoreError::Capacity("result_payload_limit"));
        }

        let snapshot = {
            let record = map
                .records
                .get_mut(&input.binding.result_ref)
                .ok_or(ResultStoreError::Unavailable("result_record"))?;
            record.continuation.apply_terminal_request(Some(request))?;
            record.pending_action = None;
            record.state = final_state;
            record.payload_bytes = final_payload_bytes;
            record.revision = next_revision;
            record.last_accessed_at = now;
            record.state_changed_at = now;
            record.snapshot()
        };
        map.total_payload_bytes = next_total_payload_bytes;
        Ok(snapshot)
    }
}

fn normalize_instruction(instruction: &str) -> Result<&str, ResultStoreError> {
    if instruction.chars().any(char::is_control) {
        return Err(ResultStoreError::InvalidInput("edit_instruction_control"));
    }
    let instruction = instruction.trim();
    if instruction.is_empty() || instruction.len() > MAX_EDIT_INSTRUCTION_BYTES {
        return Err(ResultStoreError::InvalidInput("edit_instruction"));
    }
    Ok(instruction)
}

/// The visible continuation in the card's locale: a short lead-in, then the
/// user's instruction verbatim. The message is complete, so no context is sent.
fn edit_continuation(
    instruction: &str,
    locale: crate::PresentationLocale,
) -> ResultContinuationRequest {
    let lead = match locale {
        crate::PresentationLocale::English => {
            "The user requested changes; the pending action was not executed. Requested changes:"
        }
        crate::PresentationLocale::SimplifiedChinese => "用户要求修改，原操作未执行。修改要求：",
    };
    ResultContinuationRequest {
        requires_model_context: false,
        message: format!("{lead}\n\n{instruction}"),
        context: ResultContinuationContext::EditRequested,
    }
}
