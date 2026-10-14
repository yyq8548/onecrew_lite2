import { afterEach, describe, expect, it, vi } from "vitest";

import { applyHostContext } from "./host-theme";

describe("MCP App Host theme projection", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("uses Host theme as the document authority and applies partial style variables", () => {
    const attributes = new Map<string, string>();
    const properties = new Map<string, string>();
    const style = {
      colorScheme: "",
      setProperty: (name: string, value: string) => properties.set(name, value),
    };
    vi.stubGlobal("document", {
      documentElement: {
        setAttribute: (name: string, value: string) => attributes.set(name, value),
        style,
      },
    });

    applyHostContext({
      theme: "dark",
      styles: { variables: { "--font-sans": "Host Sans" } },
    } as Parameters<typeof applyHostContext>[0]);
    expect(attributes.get("data-theme")).toBe("dark");
    expect(style.colorScheme).toBe("dark");
    expect(properties.get("--font-sans")).toBe("Host Sans");

    applyHostContext({ theme: "light" } as Parameters<typeof applyHostContext>[0]);
    expect(attributes.get("data-theme")).toBe("light");
    expect(style.colorScheme).toBe("light");
  });
});
