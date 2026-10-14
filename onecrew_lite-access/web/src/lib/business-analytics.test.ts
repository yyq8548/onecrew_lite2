import { afterEach, beforeEach, expect, it, vi } from "vitest";

beforeEach(() => { vi.resetModules(); });
afterEach(async () => {
  delete window.onecrewAnalytics;
  window.dispatchEvent(new Event("onecrew:analytics-ready"));
  await Promise.resolve();
  vi.unstubAllGlobals();
});
function tag(ready = true) {
  const analytics = { surface: "access" as const, pageView: vi.fn(), business: vi.fn(), ready: vi.fn().mockResolvedValue(ready) };
  window.onecrewAnalytics = analytics;
  return analytics;
}

it("waits for the standard tag, coalesces calls, and only dispatches server-confirmed names", async () => {
  const analytics = tag();
  const fetch = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ events: ["sign_up", "generate_lead", "invalid"] }) });
  vi.stubGlobal("fetch", fetch);
  const { flushBusinessAnalytics } = await import("./business-analytics");
  const first = flushBusinessAnalytics(); expect(flushBusinessAnalytics()).toBe(first);
  await first;
  expect(fetch).toHaveBeenCalledOnce();
  expect(analytics.business.mock.calls).toEqual([["sign_up"], ["generate_lead"]]);
  fetch.mockResolvedValue({ ok: true, json: async () => ({ events: [] }) });
  await flushBusinessAnalytics();
  expect(analytics.business).toHaveBeenCalledTimes(2);
});

it("does not claim receipts if analytics is unavailable and never rejects business callers", async () => {
  const analytics = tag(false); const fetch = vi.fn(); vi.stubGlobal("fetch", fetch);
  const { flushBusinessAnalytics } = await import("./business-analytics");
  await expect(flushBusinessAnalytics()).resolves.toBeUndefined();
  expect(fetch).not.toHaveBeenCalled(); expect(analytics.business).not.toHaveBeenCalled();
  analytics.ready.mockResolvedValue(true); fetch.mockRejectedValue(new Error("offline"));
  await expect(flushBusinessAnalytics()).resolves.toBeUndefined();
  expect(analytics.business).not.toHaveBeenCalled();
});

it("drains a new business success arriving while the prior request is pending", async () => {
  const analytics = tag(); let resolve!: (value: unknown) => void;
  const fetch = vi.fn().mockImplementationOnce(() => new Promise(done => { resolve = done; }))
    .mockResolvedValueOnce({ ok: true, json: async () => ({ events: ["generate_lead"] }) });
  vi.stubGlobal("fetch", fetch);
  const { flushBusinessAnalytics } = await import("./business-analytics");
  const first = flushBusinessAnalytics(); await Promise.resolve(); await Promise.resolve();
  flushBusinessAnalytics(); resolve({ ok: true, json: async () => ({ events: ["sign_up"] }) });
  await first;
  expect(analytics.business.mock.calls).toEqual([["sign_up"], ["generate_lead"]]);
});

it("resumes once when the tag becomes ready after timing out", async () => {
  const analytics = tag(false);
  const fetch = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ events: ["sign_up"] }) });
  vi.stubGlobal("fetch", fetch);
  const { flushBusinessAnalytics } = await import("./business-analytics");
  await flushBusinessAnalytics(); expect(fetch).not.toHaveBeenCalled();
  analytics.ready.mockResolvedValue(true);
  window.dispatchEvent(new Event("onecrew:analytics-ready"));
  await vi.waitFor(() => expect(analytics.business).toHaveBeenCalledOnce());
  window.dispatchEvent(new Event("onecrew:analytics-ready"));
  expect(fetch).toHaveBeenCalledOnce();
});
