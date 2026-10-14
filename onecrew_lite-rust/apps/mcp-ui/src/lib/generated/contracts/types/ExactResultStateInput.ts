/* Generated. Do not edit. */

export type ExactResultStateInput =
  | {
      action: "confirm_creator_save_batch";
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
       * @minItems 1
       * @maxItems 50
       */
      selections: {
        /**
         * @minItems 1
         * @maxItems 128
         */
        confirmed_route_ids: string[];
        row_id: string;
      }[];
    }
  | {
      action: "revise_search_plan";
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
      patch: {
        description?: string | null;
        keywords?: string[] | null;
      };
    }
  | {
      action: "read_current";
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
    }
  | {
      action: "confirm";
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
    }
  | {
      action: "select";
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
    }
  | {
      action: "cancel";
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
    }
  | {
      action: "request_edit";
      input: {
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
        instruction: string;
      };
    }
  | {
      action: "claim_continuation";
      input: {
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
        expected_continuation_revision: number;
        mode: "pending" | "retry_paused";
      };
    }
  | {
      action: "complete_continuation";
      input: {
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
        expected_continuation_revision: number;
        lease_id: string;
        outcome: "delivered" | "paused" | "delivery_unknown";
      };
    };
