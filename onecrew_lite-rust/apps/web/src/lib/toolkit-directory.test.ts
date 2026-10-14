import { describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@/lib/local-runtime", () => ({ invoke: invokeMock }));

import {
  listToolkitDirectory,
  TOOLKIT_DIRECTORY_QUERY_KEY,
  toolkitDirectoryQueryOptions,
  type ToolkitDirectory,
} from "@/lib/toolkit-directory";

describe("toolkit directory Tauri client", () => {
  it("keeps command spelling and the immutable query family in one owner", async () => {
    const result: ToolkitDirectory[] = [{
      id: "toolkit.x",
      toolkit: "x",
      title: "X",
      summary: "A public social network for real-time conversations.",
      purpose: "Find and inspect public X resources.",
      connection_model: "Uses an X operator account.",
      identifier_semantics: ["Use provider-returned handles."],
      capability_categories: ["profiles"],
      gotchas: ["Keep identifier domains separate."],
      operations: [{
        id: "x.profile.get",
        title: "X profile by handle",
        summary: "Read one X profile by handle.",
      }],
    }];
    invokeMock.mockResolvedValueOnce(result);

    await expect(listToolkitDirectory()).resolves.toBe(result);
    expect(invokeMock).toHaveBeenCalledWith("toolkit_directory");

    const options = toolkitDirectoryQueryOptions(true);
    expect(options.queryKey).toEqual(TOOLKIT_DIRECTORY_QUERY_KEY);
    expect(options.queryFn).toBe(listToolkitDirectory);
    expect(options.enabled).toBe(true);
    expect(options.retry).toBe(false);
    expect(options.staleTime).toBe(Infinity);
    expect(options.refetchOnWindowFocus).toBe(false);
  });
});
