import { expect, it } from "vitest";
import { executionRunning, parseExecution } from "./discovery-execution";

const status = { campaign_id: "keyboard", revision: 1, stage: "searching", updated_at: 1000,
  used: 7, reason: null, failure_phase: null, failure_code: null, sync_error: null, synced_at: null, deferred_scores: 0, sync_pending: false,
  excluded_platforms: [], deferred_hits: [], library: null };
it("retains host execution and rejects malformed or broadened status", () => {
  const parsed = parseExecution(status);
  expect(executionRunning(parsed)).toBe(true);
  expect(executionRunning(parseExecution({ ...status, stage: "paused" }))).toBe(false);
  expect(() => parseExecution({ ...status, stage: "invented" })).toThrow();
  expect(() => parseExecution({ ...status, authority: "approved" })).toThrow();
});
it("binds the background Library projection to its campaign and local table", () => {
  const library = { campaign_id: "keyboard", table_id: "table", web_url: "http://127.0.0.1:8091/library/table", total_hits: 10, inserted_rows: 2 };
  expect(parseExecution({ ...status, library })?.library?.table_id).toBe("table");
  for (const invalid of [{ ...library, campaign_id: "other" },
    { ...library, web_url: "https://example.com/library/table" },
    { ...library, web_url: "http://127.0.0.1:8091/library/other" }])
    expect(() => parseExecution({ ...status, library: invalid })).toThrow();
});
