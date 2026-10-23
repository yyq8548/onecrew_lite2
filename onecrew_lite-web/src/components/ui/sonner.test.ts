import { describe, expect, it } from "vitest";

const SOURCES = import.meta.glob("./sonner.tsx", {
  query: "?raw",
  import: "default",
  eager: true,
});
const source = String(SOURCES["./sonner.tsx"]);

describe("global toaster placement contract", () => {
  it("portals Sonner to document.body without requiring a browser during render", () => {
    expect(source).toContain("createPortal");
    expect(source).toContain("document.body");
    expect(source).toContain('typeof document === "undefined" ? null : document.body');
  });
});
