use std::collections::BTreeSet;

use onecrew_local_runtime::{OwnerActionPayload, ResultChoiceQuestion};
use serde::{Deserialize, Serialize};

use crate::operation_registry::descriptor::EntryId;

use super::contract::{ToolExecutionCall, UserQuestionId, valid_choice_questions};

/// MCP-owned continuation of a sealed business review. The runtime stores and
/// hashes these bytes but never interprets calls or grants them authority.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnerResumeEnvelope {
    pub(super) surface: crate::operation_registry::descriptor::SurfaceId,
    pub(super) pending: ResumeCall,
    pub(super) remaining: Vec<ResumeCall>,
}

pub(super) fn owner_call_digest(
    surface: crate::operation_registry::descriptor::SurfaceId,
    call: &ResumeCall,
    operation: &crate::operation_registry::descriptor::OperationEntry,
) -> Result<onecrew_local_runtime::OwnerCallDigest, ResumeEnvelopeFailure> {
    let bytes = serde_json::to_vec(&(
        surface,
        call,
        operation.output.revision(),
        operation.effect,
        operation.approval,
        operation.secret,
        operation.binding.output_class(),
        operation.view_kind,
        &operation.metadata.title,
        &operation.metadata.summary,
        &operation.metadata.aliases,
        operation.metadata.toolkit,
        &operation.metadata.domain_id,
        &operation.safety,
        operation.metadata.surfaces.iter().collect::<Vec<_>>(),
        &operation.connection,
    ))
    .map_err(|_| ResumeEnvelopeFailure::Invalid)?;
    Ok(onecrew_local_runtime::OwnerCallDigest::from_bytes(&bytes))
}

pub(super) fn encode_owner_resume_envelope(
    surface: crate::operation_registry::descriptor::SurfaceId,
    pending: ResumeCall,
    remaining: Vec<ResumeCall>,
) -> Result<onecrew_local_runtime::PeerResumePayload, ResumeEnvelopeFailure> {
    validate_resume_envelope(&ResumeEnvelope::V1 {
        pending: pending.clone(),
        remaining: remaining.clone(),
    })?;
    let bytes = serde_json::to_vec(&OwnerResumeEnvelope {
        surface,
        pending,
        remaining,
    })
    .map_err(|_| ResumeEnvelopeFailure::Invalid)?;
    onecrew_local_runtime::PeerResumePayload::new(bytes).map_err(|_| ResumeEnvelopeFailure::Invalid)
}

pub(super) fn decode_owner_resume_envelope(
    bytes: &[u8],
) -> Result<OwnerResumeEnvelope, ResumeEnvelopeFailure> {
    let envelope: OwnerResumeEnvelope =
        serde_json::from_slice(bytes).map_err(|_| ResumeEnvelopeFailure::Invalid)?;
    validate_resume_envelope(&ResumeEnvelope::V1 {
        pending: envelope.pending.clone(),
        remaining: envelope.remaining.clone(),
    })?;
    Ok(envelope)
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "version", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ResumeEnvelope {
    V1 {
        pending: ResumeCall,
        remaining: Vec<ResumeCall>,
    },
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResumeCall {
    pub(super) request_id: String,
    pub(super) call: ToolExecutionCall,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ResumeEnvelopeFailure {
    Invalid,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "version", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ChoiceResumeEnvelope {
    V2 {
        call_id: EntryId,
        operation_id: EntryId,
        interaction_id: UserQuestionId,
        questions: Vec<ResultChoiceQuestion>,
    },
}

pub(super) fn encode_resume_envelope(
    pending: ResumeCall,
    remaining: Vec<ResumeCall>,
) -> Result<OwnerActionPayload, ResumeEnvelopeFailure> {
    let envelope = ResumeEnvelope::V1 { pending, remaining };
    validate_resume_envelope(&envelope)?;
    let bytes = serde_json::to_vec(&envelope).map_err(|_| ResumeEnvelopeFailure::Invalid)?;
    let peer = onecrew_local_runtime::PeerResumePayload::new(bytes)
        .map_err(|_| ResumeEnvelopeFailure::Invalid)?;
    Ok(OwnerActionPayload::from_peer_resume(peer))
}

pub(super) fn decode_resume_envelope(
    payload: Vec<u8>,
) -> Result<ResumeEnvelope, ResumeEnvelopeFailure> {
    let envelope = serde_json::from_slice::<ResumeEnvelope>(&payload)
        .map_err(|_| ResumeEnvelopeFailure::Invalid)?;
    validate_resume_envelope(&envelope)?;
    Ok(envelope)
}

pub(super) fn encode_choice_resume_envelope(
    call_id: EntryId,
    operation_id: EntryId,
    interaction_id: UserQuestionId,
    questions: Vec<ResultChoiceQuestion>,
) -> Result<OwnerActionPayload, ResumeEnvelopeFailure> {
    let envelope = ChoiceResumeEnvelope::V2 {
        call_id,
        operation_id,
        interaction_id,
        questions,
    };
    validate_choice_resume_envelope(&envelope)?;
    let bytes = serde_json::to_vec(&envelope).map_err(|_| ResumeEnvelopeFailure::Invalid)?;
    OwnerActionPayload::new(bytes).map_err(|_| ResumeEnvelopeFailure::Invalid)
}

pub(super) fn decode_choice_resume_envelope(
    payload: Vec<u8>,
) -> Result<ChoiceResumeEnvelope, ResumeEnvelopeFailure> {
    let envelope = serde_json::from_slice::<ChoiceResumeEnvelope>(&payload)
        .map_err(|_| ResumeEnvelopeFailure::Invalid)?;
    validate_choice_resume_envelope(&envelope)?;
    Ok(envelope)
}

fn validate_resume_envelope(envelope: &ResumeEnvelope) -> Result<(), ResumeEnvelopeFailure> {
    let ResumeEnvelope::V1 { pending, remaining } = envelope;
    if remaining.len() > 7 {
        return Err(ResumeEnvelopeFailure::Invalid);
    }
    let calls = std::iter::once(pending).chain(remaining);
    let mut call_ids = BTreeSet::new();
    let mut request_ids = BTreeSet::new();
    for call in calls {
        if !valid_request_id(&call.request_id)
            || !call_ids.insert(call.call.call_id.as_str())
            || !request_ids.insert(call.request_id.as_str())
        {
            return Err(ResumeEnvelopeFailure::Invalid);
        }
    }
    Ok(())
}

fn validate_choice_resume_envelope(
    envelope: &ChoiceResumeEnvelope,
) -> Result<(), ResumeEnvelopeFailure> {
    let ChoiceResumeEnvelope::V2 {
        call_id: _,
        operation_id,
        interaction_id,
        questions,
    } = envelope;
    (operation_id.as_str() == super::projection::ASK_USER_QUESTION_OPERATION_ID
        && !interaction_id.as_str().is_empty()
        && valid_choice_questions(questions))
    .then_some(())
    .ok_or(ResumeEnvelopeFailure::Invalid)
}

fn valid_request_id(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("mcp-v2-") else {
        return false;
    };
    rest.len() == 27
        && rest.as_bytes()[2] == b'-'
        && rest[..2].bytes().all(|byte| byte.is_ascii_digit())
        && rest[3..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
