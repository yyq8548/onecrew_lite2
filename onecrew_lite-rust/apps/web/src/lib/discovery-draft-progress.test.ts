import { expect, it } from "vitest";
import { parseDraftProgress } from "./discovery-draft-progress";

it("binds planning diagnostics to their request and validates completed phase timings", () => {
  const progress = { id: "request", phase: "planning", elapsed_ms: 3500, phase_elapsed_ms: 2000,
    timings: [{ phase: "checking_connection", elapsed_ms: 1500 }], error_code: null, unsupported_conditions: [] };
  expect(parseDraftProgress(progress, "request")).toEqual(progress);
  expect(parseDraftProgress(null, "request")).toBeNull();
  const clarification = { ...progress, phase: "failed", error_code: "discovery_draft_conditions_unsupported", unsupported_conditions: ["男明星", "按照粉丝排名"] };
  expect(parseDraftProgress(clarification, "request")?.unsupported_conditions).toEqual(["男明星", "按照粉丝排名"]);
  expect(() => parseDraftProgress({ ...clarification, unsupported_conditions: [42] }, "request")).toThrow();
  for (const invalid of [{ id: "other" }, { phase: "invented" }, { phase_elapsed_ms: -1 }, { extra: "approved" }])
    expect(() => parseDraftProgress({ ...progress, ...invalid }, "request")).toThrow();
});
