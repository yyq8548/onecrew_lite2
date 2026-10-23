import { QueryClient } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { operatorBrowserStatus } from "@/lib/operator-account";
import { operatorBrowserStatusQueryOptions } from "./browser-environment";

vi.mock("@/lib/operator-account", () => ({
  operatorBrowserStatus: vi.fn(),
}));

const readyStatus = {
  installed: true,
  version: "151",
  path: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  os: "windows" as const,
  state: "ready" as const,
  channel: "stable" as const,
  install_scope: "machine" as const,
  cookie_provider: "windows_legacy_dpapi" as const,
  limitations: [],
};

describe("operatorBrowserStatusQueryOptions", () => {
  beforeEach(() => {
    vi.mocked(operatorBrowserStatus).mockReset();
    vi.mocked(operatorBrowserStatus).mockResolvedValue(readyStatus);
  });

  it("deduplicates detection and retains it for the application session", async () => {
    const client = new QueryClient();
    const options = operatorBrowserStatusQueryOptions(true);

    await Promise.all([client.fetchQuery(options), client.fetchQuery(options)]);
    await client.fetchQuery(options);

    expect(operatorBrowserStatus).toHaveBeenCalledTimes(1);
    expect(options.staleTime).toBe(Infinity);
    expect(options.gcTime).toBe(Infinity);
  });

  it("keeps explicit refresh available", async () => {
    const client = new QueryClient();
    const options = operatorBrowserStatusQueryOptions(true);
    await client.fetchQuery(options);

    await client.invalidateQueries({ queryKey: options.queryKey });
    await client.fetchQuery(options);

    expect(operatorBrowserStatus).toHaveBeenCalledTimes(2);
  });
});
