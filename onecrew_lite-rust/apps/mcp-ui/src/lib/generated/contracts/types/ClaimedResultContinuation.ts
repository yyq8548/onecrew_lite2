/* Generated. Do not edit. */

export interface ClaimedResultContinuation {
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
  continuation_revision: number;
  dispatch_not_after_ms: number;
  dispatch_started_at_ms: number;
  lease_id: string;
  request: {
    /**
     * Model context carries only what the model cannot otherwise know; the Host
     * may show it to the user, so it never repeats the model's own request.
     */
    context:
      | {
          kind: "creator_save_batch";
          /**
           * @minItems 1
           * @maxItems 50
           *
           * Items: One reviewed Creator row as a follow-up needs it: the saved Creator, or why
           * the row was not saved. Display fields stay on the card.
           */
          outcomes: {
            creator_id: string | null;
            failure_code: string | null;
            /**
             * The durable effect classification for one confirmed Creator row.
             */
            operation:
              "added" | "updated" | "existing_skipped" | "duplicate_skipped" | "not_selected" | "failed" | "unknown";
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
          /**
           * Counts for all rows represented by one Creator save completion.
           */
          summary: {
            added: number;
            duplicate_skipped: number;
            existing_skipped: number;
            failed: number;
            not_selected: number;
            total: number;
            unknown: number;
            updated: number;
          };
        }
      | {
          confirmed_plan_ref: string;
          description: string;
          keywords: string[];
          kind: "search_plan_confirmed";
        }
      | {
          kind: "execution";
          /**
           * @minItems 1
           * @maxItems 8
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
          kind: "choice";
        }
      | {
          kind: "cancelled";
        }
      | {
          kind: "edit_requested";
        };
    message: string;
    /**
     * True exactly when the context carries content the model needs and the
     * concise message omits; only then is context sent, and the Host must
     * accept it before the message.
     */
    requires_model_context?: boolean;
  };
}
