import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const SOURCES = import.meta.glob([
  "/src/components/ui/result-status.tsx",
  "/src/components/ui/platform-mark.tsx",
  "/src/components/ui/onecrew-brand.tsx",
], { query: "?raw", import: "default", eager: true }) as Record<string, string>;

// Stylesheets are read from disk: the bundler rewrites CSS it imports, and this
// suite asserts the single colour source the bundle is built from.
const readCss = (path: string) => readFileSync(fileURLToPath(new URL(path, import.meta.url)), "utf8");
const theme = readCss("../styles/theme.css");
const filterSelect = readCss("../components/ui/filter-select.css");
const rootBlock = /:root \{([\s\S]*?)\n\}/.exec(theme)?.[1] ?? "";
const darkBlock = /:root\[data-theme="dark"\] \{([\s\S]*?)\n\}/.exec(theme)?.[1] ?? "";

const SEMANTIC_TOKENS = [
  "background", "foreground", "card", "card-foreground", "primary", "primary-foreground",
  "secondary", "secondary-foreground", "muted", "muted-foreground", "accent", "accent-foreground",
  "destructive", "success-background", "success-foreground", "success-border",
  "warning-background", "warning-foreground", "warning-border", "border", "input",
];

describe("one colour source for the MCP App", () => {
  it("resolves every semantic token from the Host palette before the package fallback", () => {
    expect(rootBlock).not.toBe("");
    expect(darkBlock).not.toBe("");
    for (const token of SEMANTIC_TOKENS) {
      const light = new RegExp(`^\\s*--${token}:\\s*(.+);$`, "m").exec(rootBlock)?.[1];
      const dark = new RegExp(`^\\s*--${token}:\\s*(.+);$`, "m").exec(darkBlock)?.[1];
      expect(light, `light --${token}`).toMatch(/^var\(--(color|border|font)-[a-z-]+, .+\)$/);
      expect(dark, `dark --${token}`).toMatch(/^var\(--(color|border|font)-[a-z-]+, .+\)$/);
    }
  });

  it("declares the brand accent, ring and icon colour exactly once each", () => {
    expect(theme.match(/--brand-hue:/g)).toHaveLength(1);
    expect(theme.match(/--ring:/g)).toHaveLength(1);
    expect(theme.match(/--icon-color:/g)).toHaveLength(1);
    expect(theme).toContain("--ring: var(--color-ring-primary, var(--brand-hue));");
    expect(theme).toContain("--icon-color: var(--foreground);");
  });

  it("leaves channel identity as the only literal palette in the stylesheet", () => {
    const literals = theme
      .split("\n")
      .filter((line) => /#[0-9a-fA-F]{3,8}\b/.test(line))
      .map((line) => line.trim());
    expect(literals.length).toBeGreaterThan(0);
    for (const line of literals) {
      expect(line.startsWith(".channel-mark") || line.includes("--brand-hue"), line).toBe(true);
    }
    // Brand and icon colour follow the text token rather than restating black
    // and white per theme.
    expect(theme).toContain(".onecrew-brand { color: var(--foreground); }");
    expect(theme).not.toContain('[data-theme="dark"] .onecrew-brand');
  });

  it("gives result status one tone authority instead of a stylesheet override", () => {
    const status = SOURCES["/src/components/ui/result-status.tsx"]!;
    expect(status).toContain("text-success-foreground");
    expect(status).toContain("text-destructive");
    expect(status).toContain("text-warning-foreground");
    expect(status).toContain("text-muted-foreground");
    expect(status).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
    // The stylesheet sizes the glyph and must not repaint what the component chose.
    expect(theme).toContain("[data-result-status] svg { width:20px; height:20px; }");
    expect(theme).not.toMatch(/\[data-result-status="[a-z]+"\]\s*\{[^}]*color:/);
  });

  it("keeps a single colour vocabulary outside the stylesheet", () => {
    for (const [path, source] of Object.entries(SOURCES)) {
      expect(source, path).not.toMatch(/\b(?:text|bg|border)-(?:red|green|blue|amber|sky|fuchsia|emerald|yellow|orange|slate|zinc|gray)-[0-9]{3}\b/);
      expect(source, path).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
    }
    expect(filterSelect).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
  });

  it("retains the Host theme binding and the selectors the bundle contract requires", () => {
    expect(theme).toContain(':root[data-theme="dark"]');
    expect(theme).toContain('@custom-variant dark (&:where([data-theme="dark"], [data-theme="dark"] *));');
    expect(theme).not.toMatch(/prefers-color-scheme/);
    expect(theme).toContain(".channel-mark");
    expect(SOURCES["/src/components/ui/platform-mark.tsx"]).toContain("channel-mark");
  });
});
