/* Generated. Do not edit. */

export type ResultStateFailure =
  | {
      code: "absent";
    }
  | {
      code: "expired";
    }
  | {
      code: "view_mismatch";
    }
  | {
      code: "origin_mismatch";
    }
  | {
      code: "fingerprint_mismatch";
    }
  | {
      code: "revision_conflict";
      current_revision: number;
    }
  | {
      code: "continuation_revision_conflict";
      current_revision: number;
    }
  | {
      code: "continuation_lease_mismatch";
    }
  | {
      code: "search_plan_payload_too_large";
    }
  | {
      code: "invalid_input";
    }
  | {
      code: "invalid_state";
    }
  | {
      code: "replay";
    }
  | {
      code: "capacity";
    }
  | {
      code: "unavailable";
    };
