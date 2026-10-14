import { describe, expect, it, vi } from "vitest";
import { createLocaleSynchronizer } from "./presentation-locale";

describe("MCP App language preference bridge", () => {
  it("resends the current language after the service reconnects", async () => {
    const send = vi.fn(async () => undefined);
    const sync = createLocaleSynchronizer(send);
    await sync("zh-CN");
    await sync("zh-CN", true);
    expect(send).toHaveBeenCalledTimes(2);
  });
  it("serializes changes and deduplicates the initial language event", async () => {
    const send = vi.fn(async (_locale: "en-US" | "zh-CN") => undefined);
    const sync = createLocaleSynchronizer(send);
    await Promise.all([sync("zh-CN"), sync("zh-CN"), sync("en-US"), sync("zh-CN")]);
    expect(send.mock.calls.map(([locale]) => locale)).toEqual(["zh-CN", "en-US", "zh-CN"]);
  });

  it("does not remember a failed update and permits retry", async () => {
    const send = vi.fn().mockRejectedValueOnce(new Error("unavailable")).mockResolvedValue(undefined);
    const sync = createLocaleSynchronizer(send);
    await expect(sync("zh-CN")).rejects.toThrow("unavailable");
    await sync("zh-CN");
    expect(send).toHaveBeenCalledTimes(2);
  });
});
