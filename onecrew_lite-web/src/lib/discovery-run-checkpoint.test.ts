import { expect, it } from "vitest";
import { checkpointExecution, loadRunCheckpoint, loadRunHistory, saveRunCheckpoint } from "./discovery-run-checkpoint";

const campaign = { id: "phone", revision: 1, used_calls: 11 };
function storage() {
  const data = new Map<string, string>();
  return { data, getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => { data.set(key, value); } };
}
it("retains the exact stop checkpoint on reload without restoring execution or authority", () => {
  const store = storage();
  saveRunCheckpoint(store, campaign, "page_closed", 100);
  expect(loadRunCheckpoint(store, "phone")).toEqual({ campaign_id: "phone", revision: 1, used: 11, reason: "page_closed", recorded_at: 100 });
  expect(checkpointExecution(loadRunCheckpoint(store, "phone")?.reason)).toBe("paused");
  expect(loadRunCheckpoint(store, "water")).toBeNull();
  saveRunCheckpoint(store, campaign, null, 101);
  expect(loadRunCheckpoint(store, "phone")?.reason).toBe("execution_interrupted");
  expect(loadRunHistory(store, "phone").map(item => item.reason)).toEqual(["page_closed"]);
  saveRunCheckpoint(store, { ...campaign, used_calls: 12 }, null, 102);
  saveRunCheckpoint(store, { ...campaign, used_calls: 13 }, "user_paused", 103);
  expect(loadRunHistory(store, "phone").map(item => [item.reason, item.used])).toEqual([["user_paused", 13], ["page_closed", 11]]);
});
it("keeps uncertain execution separate from source failures and does not save error text", () => {
  const store = storage();
  saveRunCheckpoint(store, campaign, "request_failed", 100);
  expect(checkpointExecution(loadRunCheckpoint(store, "phone")?.reason)).toBe("error");
  expect(JSON.stringify([...store.data.values()])).not.toContain("credential");
  for (let n = 0; n < 25; n++) saveRunCheckpoint(store, { ...campaign, id: `campaign-${n}` }, "user_paused", 101 + n);
  expect(loadRunCheckpoint(store, "phone")).toBeNull();
  expect(loadRunCheckpoint(store, "campaign-24")?.reason).toBe("user_paused");
});
it("retains unlimited reads and explicit pause across reload", () => {
  const store = storage();
  saveRunCheckpoint(store, { ...campaign, used_calls: 10001 }, "user_paused", 100);
  expect(loadRunCheckpoint(store, "phone")).not.toHaveProperty("max");
  expect(loadRunCheckpoint(store, "phone")?.used).toBe(10001);
  expect(checkpointExecution(loadRunCheckpoint(store, "phone")?.reason)).toBe("paused");
});
it("loads historical checkpoints without restoring the retired limit", () => {
  const store = storage();
  store.setItem("onecrew-discovery-run-checkpoints-v1", JSON.stringify({ format: 1, entries: [
    { campaign_id: "phone", revision: 1, used: 11, max: 11, reason: "budget_exhausted", recorded_at: 100 },
  ] }));
  expect(loadRunCheckpoint(store, "phone")).toEqual({ campaign_id: "phone", revision: 1, used: 11, reason: "execution_interrupted", recorded_at: 100 });
  saveRunCheckpoint(store, { ...campaign, used_calls: 12 }, "user_paused", 101);
  expect(loadRunHistory(store, "phone")).toHaveLength(2);
  expect(store.getItem("onecrew-discovery-run-checkpoints-v1")).not.toContain('"max"');
});
it("refuses corrupt or future journals and surfaces unavailable storage", () => {
  const store = storage();
  store.setItem("onecrew-discovery-run-checkpoints-v1", '{"format":2,"entries":[]}');
  expect(saveRunCheckpoint(store, campaign, "user_paused")).toBeNull();
  expect([...store.data.values()]).toEqual(['{"format":2,"entries":[]}']);
  expect(loadRunCheckpoint(store, "phone")).toBeNull();
  expect(saveRunCheckpoint({ getItem: () => null, setItem: () => { throw new Error("unavailable"); } }, campaign, "user_paused")).toBeNull();
});
