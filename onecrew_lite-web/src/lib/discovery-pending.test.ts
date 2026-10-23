import { describe, expect, it } from "vitest";
import { pendingEvidenceReasons } from "./discovery-pending";
describe("pending evidence reasons", () => {
  it("keeps all missing hard evidence alongside a transient score failure", () => {
    expect(pendingEvidenceReasons(["subscriber_count_unverified", "language_unverified"], false, true))
      .toEqual(["subscriber_count_unverified", "language_unverified", "score_temporarily_failed"]);
  });
  it("does not fabricate missing scoring for an existing rating", () => {
    expect(pendingEvidenceReasons(["language_unverified"], true, false)).toEqual(["language_unverified"]);
    expect(pendingEvidenceReasons(undefined, false, false)).toEqual(["initial_match_unrated"]);
  });
});
