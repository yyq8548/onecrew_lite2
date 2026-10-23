import { describe, expect, it } from "vitest";
import { readWorkspaceHistory, writeWorkspaceHistory } from "./workspace-history";
import type { WorkspaceActivity } from "@mcp-ui/lib/result-contract/workspace";

const first = "11111111-1111-4111-8111-111111111111";
const second = "22222222-2222-4222-8222-222222222222";
const activity: WorkspaceActivity = { campaign_id: "one", brief: "洗发水", stage: "searching",
  qualified: 1, pending: 0, target: 4, attempted: 2, total: 3, used: 5,
  login_required: 0, read_errors: 0, restricted: 0, stale: false };
describe("workspace display history", () => {
  it("restores the same card after remount, isolated from other host views", () => {
    const records = new Map<string, string>();
    const storage = { getItem: (key: string) => records.get(key) ?? null,
      setItem: (key: string, value: string) => { records.set(key, value); } };
    const history = [activity, { ...activity, campaign_id: "two", brief: "键盘" }];
    writeWorkspaceHistory(storage, first, history);
    expect(readWorkspaceHistory(storage, first)).toEqual(history);
    expect(readWorkspaceHistory(storage, second)).toEqual([]);
  });
  it("ignores corrupt, duplicate, unbounded or authority-bearing snapshots", () => {
    for (const raw of ["bad json", "{}", "x".repeat(2_000_001), JSON.stringify([activity, activity]),
      JSON.stringify([{ ...activity, approved: true }])]) {
      expect(readWorkspaceHistory({ getItem: () => raw, setItem() {} }, first)).toEqual([]);
    }
  });
  it("survives denied sandbox storage and quota exhaustion", () => {
    const storage = { getItem(): string { throw new Error("denied"); }, setItem() { throw new Error("quota"); } };
    expect(readWorkspaceHistory(storage, first)).toEqual([]);
    expect(() => writeWorkspaceHistory(storage, first, [activity])).not.toThrow();
  });
});
