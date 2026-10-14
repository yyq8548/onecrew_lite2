import { describe, expect, it } from "vitest";

const BRAND_SOURCES = import.meta.glob([
  "/src/app/inspector-app.tsx",
  "/src/widgets/operation-inspector/index.html",
  "/src/widgets/retired-operation-inspector/index.html",
  "/src/widgets/retired-operation-inspector/main.ts",
], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

describe("onecrew_lite MCP App branding", () => {
  it("brands both inspector entries as onecrew_lite", () => {
    const app = BRAND_SOURCES["/src/app/inspector-app.tsx"];
    const active = BRAND_SOURCES["/src/widgets/operation-inspector/index.html"];
    const retired = BRAND_SOURCES["/src/widgets/retired-operation-inspector/index.html"];

    expect(app).toContain('title: "onecrew_lite"');
    expect(app).toContain('label={t("loading")}');
    expect(active).toContain("<title>onecrew_lite action card</title>");
    expect(active).toContain("<strong>onecrew_lite</strong>");
    expect(retired).toContain("<title>onecrew_lite card version notice</title>");
    expect(`${app}\n${active}\n${retired}`).not.toContain("Reacher");
  });

  it("keeps the existing SDK client machine names", () => {
    const app = BRAND_SOURCES["/src/app/inspector-app.tsx"];
    const retiredClient = BRAND_SOURCES["/src/widgets/retired-operation-inspector/main.ts"];

    expect(app).toContain('name: "onecrew-result-card"');
    expect(retiredClient).toContain('name: "onecrew-retired-card"');
  });
});
