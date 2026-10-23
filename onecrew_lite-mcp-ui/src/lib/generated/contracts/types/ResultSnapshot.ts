/* Generated. Do not edit. */

export interface ResultSnapshot {
  batch: {
    /**
     * @minItems 1
     * @maxItems 1000
     */
    outcomes: {
      call_id: string;
      disposition:
        | {
            kind: "succeeded";
          }
        | {
            code: string;
            kind: "failed";
          }
        | {
            code: string;
            kind: "unknown";
          }
        | {
            kind: "not_run";
            reason: string;
          };
      operation_id: string;
      /**
       * @maxItems 64
       */
      resource_refs: {
        id: string;
        kind: "creator" | "conversation" | "connection" | "provider_object";
        label: string | null;
      }[];
      summary: string;
    }[];
  };
  binding: {
    fingerprint: string;
    origin: string;
    result_ref: string;
    revision: number;
    view: {
      identity: string;
      kind: "read_only" | "confirmation" | "choice";
    };
  };
  /**
   * Exact completed prefix inside presentation.sections; the current review
   * follows it. This is display state and never grants execution authority.
   */
  completed_sections_count: number;
  continuation: {
    continuation_revision: number;
    dispatch_not_after_ms: number | null;
    dispatch_started_at_ms: number | null;
    state: "not_requested" | "pending" | "dispatching" | "delivered" | "paused" | "delivery_unknown";
  };
  /**
   * Display-only preference; excluded from result identity and user authority.
   */
  locale: "en-US" | "zh-CN";
  pending_action:
    | (
        | {
            kind: "creator_save_batch";
            review: {
              /**
               * @maxItems 50
               */
              failures: {
                display_name: string | null;
                handle: string | null;
                platform: "x" | "instagram" | "telegram" | "youtube" | "tiktok";
                profile_url: string | null;
                /**
                 * Safe explanation for a prepared row that is not directly selectable.
                 * A safe explanation paired with a non-selectable prepared row.
                 */
                reason:
                  | "existing_creator"
                  | "duplicate_selection"
                  | "identity_ambiguous"
                  | "conflicting_update"
                  | "required_contact_route_conflict"
                  | "preparation_failed";
                row_id: string;
              }[];
              /**
               * The caller's explicit save policy.  The policy is held with the prepared
               * rows so confirmation cannot silently turn an "add only" request into an
               * update after the review has been shown.
               */
              intent: "add_new" | "save_or_update";
              /**
               * @minItems 1
               * @maxItems 50
               */
              rows: {
                preview: {
                  assessment: string | null;
                  avatar_url: string | null;
                  display_name: string | null;
                  expires_at: number;
                  follower_counts: {
                    [k: string]: number;
                  };
                  handle: string | null;
                  identity_creator_ids: string[];
                  identity_status: string;
                  /**
                   * The caller's explicit save policy.  The policy is held with the prepared
                   * rows so confirmation cannot silently turn an "add only" request into an
                   * update after the review has been shown.
                   */
                  intent: "add_new" | "save_or_update";
                  notes: string | null;
                  platform: "x" | "instagram" | "telegram" | "youtube" | "tiktok";
                  profile_url: string | null;
                  reason:
                    | (
                        | "existing_creator"
                        | "duplicate_selection"
                        | "identity_ambiguous"
                        | "conflicting_update"
                        | "required_contact_route_conflict"
                        | "preparation_failed"
                      )
                    | null;
                  /**
                   * @minItems 1
                   * @maxItems 128
                   */
                  routes: {
                    channel: string;
                    collision: {
                      creator_ids: string[];
                      status: "none" | "possible" | "conflict";
                    };
                    display_value: string;
                    kind: string;
                    normalized_value: string;
                    required: boolean;
                    route_id: string;
                    sources: string[];
                  }[];
                  score: number | null;
                  /**
                   * Public content evidence carried by the prepared selection, not authority.
                   */
                  source_url: string | null;
                  /**
                   * Owner-derived actionability for one prepared row.  Adapters may display
                   * this value but must not infer it from identity or route fields.
                   */
                  status:
                    | "eligible_new"
                    | "eligible_update"
                    | "existing_skipped"
                    | "duplicate_skipped"
                    | "identity_pending"
                    | "contact_conflict"
                    | "failed";
                };
                row_id: string;
              }[];
              /**
               * Mutually exclusive, owner-derived counts for one immutable preparation.
               */
              summary: {
                blocked: number;
                duplicate_skipped: number;
                eligible_new: number;
                eligible_update: number;
                existing_skipped: number;
                failed: number;
                total: number;
              };
            };
          }
        | {
            draft: {
              description: string;
              /**
               * Editable acceptance keywords. Changing these does not rewrite a selected
               * operation's arguments; changed provider criteria require a new plan.
               */
              keywords: string[];
              /**
               * Items: One search the plan runs after the user confirms it.
               */
              scopes: {
                /**
                 * Search criteria for the selected operation, valid under its input_schema. Omit search_plan_policy, confirmed_plan_ref and cursor; they belong to the later search call.
                 */
                arguments: {
                  [k: string]: unknown;
                };
                /**
                 * The tagged connection returned by onecrew_connection_list for the operation's toolkit.
                 */
                connection:
                  | {
                      kind: "operator_account";
                      operator_account_id: string;
                      toolkit: string;
                    }
                  | {
                      credential_id: string;
                      kind: "service_credential";
                      toolkit: string;
                    };
                operation_id: string;
                /**
                 * Copy verbatim from onecrew_tool_get for operation_id.
                 */
                schema_revision: string;
              }[];
            };
            draft_digest: string;
            kind: "search_plan";
          }
        | {
            kind: "confirm";
            label: string;
            prompt: string;
          }
        | {
            kind: "choice";
            /**
             * @minItems 1
             * @maxItems 32
             */
            questions: {
              header: string;
              id: string;
              /**
               * @minItems 1
               * @maxItems 32
               */
              options: {
                description: string | null;
                id: string;
                label: string;
              }[];
              prompt: string;
              selection_mode: "single" | "multiple";
            }[];
          }
      )
    | null;
  presentation: {
    kind:
      | "creator_save_batch_confirmation"
      | "creator_save_batch_outcome"
      | "creator_batch_confirmation"
      | "creator_batch_outcome"
      | "outreach_confirmation"
      | "outreach_outcome"
      | "email_confirmation"
      | "email_outcome"
      | "search_plan_editor"
      | "outreach_draft"
      | "record_preview"
      | "simple_operation"
      | "read_only"
      | "choice"
      | "batch_result";
    /**
     * @maxItems 656
     */
    sections: (
      | {
          kind: "creator_save_rows";
          rows: {
            avatar_url: string | null;
            confirmed_route_ids: string[];
            creator_id: string | null;
            display_name: string | null;
            failure_code: string | null;
            handle: string | null;
            /**
             * The durable effect classification for one confirmed Creator row.
             */
            operation:
              "added" | "updated" | "existing_skipped" | "duplicate_skipped" | "not_selected" | "failed" | "unknown";
            platform: "x" | "instagram" | "telegram" | "youtube" | "tiktok";
            profile_url: string | null;
            reason:
              | (
                  | "existing_creator"
                  | "duplicate_selection"
                  | "identity_ambiguous"
                  | "required_contact_route_conflict"
                  | "conflicting_update"
                  | "preparation_failed"
                  | "user_not_selected"
                  | "row_failed"
                  | "commit_unknown"
                )
              | null;
            row_id: string;
            status: "saved" | "failed" | "unknown" | "not_run" | "not_selected";
          }[];
        }
      | {
          /**
           * @minItems 1
           * @maxItems 128
           */
          fields: {
            label: string;
            value: string;
          }[];
          heading: string | null;
          kind: "fields";
        }
      | {
          heading: string | null;
          /**
           * @minItems 1
           * @maxItems 128
           */
          items: {
            description: string | null;
            title: string;
          }[];
          kind: "items";
        }
      | {
          body: string;
          heading: string | null;
          kind: "notice";
          tone: "neutral" | "success" | "warning" | "critical";
        }
    )[];
    summary: string | null;
    title: string;
  };
  presented: boolean;
  state:
    | {
        kind: "available";
      }
    | {
        action:
          | {
              kind: "confirmed";
            }
          | {
              /**
               * @minItems 1
               * @maxItems 32
               */
              answers: (
                | {
                    /**
                     * @minItems 1
                     * @maxItems 32
                     */
                    option_ids: string[];
                    question_id: string;
                  }
                | {
                    option_id: string;
                    question_id: string;
                  }
                | {
                    question_id: string;
                    text: string;
                  }
              )[];
              kind: "selected";
            };
        kind: "submitted";
      }
    | {
        code: string;
        kind: "resolved";
        summary: string;
      }
    | {
        code: string;
        kind: "failed";
        retryable: boolean;
        summary: string;
      }
    | {
        code: string;
        kind: "unknown";
        summary: string;
      };
}
