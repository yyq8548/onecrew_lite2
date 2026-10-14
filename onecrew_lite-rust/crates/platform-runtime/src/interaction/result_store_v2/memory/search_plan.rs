use std::time::Instant;

use super::super::validation::{result_payload_bytes, validate_executed_result};
use super::super::{
    ExecutedResultInput, InitialResultState, OwnerActionPayload, PendingUserAction, ResultBinding,
    ResultSnapshot, ResultStoreError,
};
use super::{ResultMemoryStore, expire_target, require_user_action_available, validate_binding};

impl ResultMemoryStore {
    /// A single mutex protects validation, draft compilation and the revision CAS.
    /// Mint identity is stable; the pending payload digest identifies the current draft.
    pub(in super::super) fn revise_search_plan(
        &self,
        binding: &ResultBinding,
        patch: crate::SearchPlanPatch,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        let now = Instant::now();
        let mut map = self.lock()?;
        expire_target(&mut map, &binding.result_ref, self.limits, now)?;
        let record = map
            .records
            .get(&binding.result_ref)
            .ok_or(ResultStoreError::Absent)?;
        validate_binding(record, binding)?;
        require_user_action_available(record)?;
        let Some(PendingUserAction::SearchPlan { draft, .. }) = &record.pending_action else {
            return Err(ResultStoreError::InvalidState("search_plan_required"));
        };
        let draft = draft.apply_patch(patch).map_err(|error| {
            ResultStoreError::InvalidInput(if error == "search_plan_payload_too_large" {
                "search_plan_payload_too_large"
            } else {
                "search_plan_patch"
            })
        })?;
        let payload = OwnerActionPayload::from_search_plan(&draft)?;
        let candidate = ExecutedResultInput {
            origin: record.origin.clone(),
            view_kind: record.view.kind,
            batch: record.batch.clone(),
            presentation: super::super::search_plan_projection(&draft)?,
            pending_action: Some(PendingUserAction::SearchPlan { draft, payload }),
            initial_state: InitialResultState::Available,
        };
        validate_executed_result(&candidate)?;
        let bytes = result_payload_bytes(&candidate)?;
        let total = map
            .total_payload_bytes
            .checked_sub(record.payload_bytes)
            .and_then(|n| n.checked_add(bytes))
            .ok_or(ResultStoreError::Capacity("result_payload_limit"))?;
        if total > self.limits.max_total_payload_bytes {
            return Err(ResultStoreError::Capacity("result_payload_limit"));
        }
        let revision = record
            .revision
            .checked_add(1)
            .ok_or(ResultStoreError::Unavailable("result_revision"))?;
        let record = map
            .records
            .get_mut(&binding.result_ref)
            .ok_or(ResultStoreError::Absent)?;
        record.presentation = candidate.presentation;
        record.pending_action = candidate.pending_action;
        record.payload_bytes = bytes;
        record.revision = revision;
        record.last_accessed_at = now;
        // Retain the existing Available inactivity TTL semantics.
        let snapshot = record.snapshot();
        map.total_payload_bytes = total;
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::result_store_v2::{
        CallDisposition, ExecutedBatch, ExecutedCallOutcome, ResultOrigin, ResultViewKind,
    };

    fn input() -> ExecutedResultInput {
        let draft: crate::SearchPlanEditorDraft = serde_json::from_value(serde_json::json!({
            "description":"Acceptance criteria", "keywords":["rust"],
            "scopes":[{"operation_id":"youtube.channels.search","schema_revision":"test-revision",
                "connection":{"kind":"service_credential","toolkit":"youtube","credential_id":"test-credential"},
                "arguments":{"query":"rust","count":5}}]
        })).unwrap();
        ExecutedResultInput {
            origin: ResultOrigin::new("mcp:full").unwrap(),
            view_kind: ResultViewKind::Confirmation,
            batch: ExecutedBatch {
                outcomes: vec![ExecutedCallOutcome {
                    call_id: "plan".into(),
                    operation_id: "onecrew.search.plan.prepare".into(),
                    disposition: CallDisposition::NotRun {
                        reason: "confirmation_required".into(),
                    },
                    summary: "Review plan".into(),
                    resource_refs: vec![],
                }],
            },
            presentation: crate::search_plan_projection(&draft).unwrap(),
            pending_action: Some(PendingUserAction::SearchPlan {
                payload: OwnerActionPayload::new(serde_json::to_vec(&draft).unwrap()).unwrap(),
                draft,
            }),
            initial_state: InitialResultState::Available,
        }
    }

    #[test]
    fn unpresented_and_expired_drafts_cannot_be_revised() {
        let store = ResultMemoryStore::new();
        let result_ref = store.mint_from_execute(input()).unwrap();
        let binding = store.lock().unwrap().records[&result_ref].binding();
        let patch = crate::SearchPlanPatch {
            description: Some("Updated".into()),
            keywords: None,
        };
        assert_eq!(
            store
                .revise_search_plan(&binding, patch.clone())
                .unwrap_err(),
            ResultStoreError::InvalidState("result_not_presented")
        );
        assert_eq!(store.read(&binding).unwrap().binding.revision, 1);
        let snapshot = store.present(&result_ref, &binding.origin).unwrap();
        store
            .lock()
            .unwrap()
            .records
            .get_mut(&result_ref)
            .unwrap()
            .last_accessed_at =
            Instant::now() - store.limits.available_ttl - std::time::Duration::from_secs(1);
        assert_eq!(
            store
                .revise_search_plan(&snapshot.binding, patch)
                .unwrap_err(),
            ResultStoreError::Expired
        );
    }
}
