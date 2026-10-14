const errors = {
  missing_structured_content: "The server did not return structuredContent.",
  structured_content_not_object: "structuredContent is not an object.",
  error_without_typed_failure: "The server marked an error without a recognized failure structure.",
  success_marked_error: "The response contains a success structure but is also marked as an error.",
  snapshot_root_rejected: "The snapshot root keys, locale, or presented flag do not match the contract.",
  snapshot_binding_rejected: "The snapshot result binding failed format validation.",
  snapshot_batch_rejected: "The snapshot execution batch failed format validation.",
  snapshot_presentation_rejected: "The snapshot presentation failed format validation.",
  snapshot_pending_action_rejected: "The snapshot pending action failed format validation.",
  snapshot_state_rejected: "The snapshot state failed format validation.",
  snapshot_continuation_rejected: "The snapshot continuation state failed format validation.",
  snapshot_consistency_rejected: "The snapshot view, state, and pending action are inconsistent.",
  claimed_contract_rejected: "The continuation claim contains an invalid lease structure.",
  completed_contract_rejected: "The continuation completion failed format validation.",
  failure_contract_rejected: "The failure structure does not match the current contract.",
  unrecognized_root: "The structuredContent root is not part of the current card protocol.",
} as const;
export default errors;
