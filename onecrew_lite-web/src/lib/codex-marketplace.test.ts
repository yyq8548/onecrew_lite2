import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const root = join(__dirname, "..", "..", "..");
const plugin = join(root, "onecrew_lite-app/src/client-adapters/plugin-template");
const manifest = JSON.parse(readFileSync(join(plugin, ".codex-plugin/plugin.json"), "utf8"));

describe("TypeScript plugin distribution contract", () => {
  it("pairs the webpage release with the plugin and its interface metadata", () => {
    expect(manifest.version).toBe(JSON.parse(readFileSync(join(root, "onecrew_lite-web/package.json"), "utf8")).version);
    expect(manifest.name).toBe("onecrew_lite");
    expect(manifest.mcpServers).toBe("./.mcp.json");
    expect(manifest.skills).toBe("./skills/");
    expect(manifest.homepage).toBe("https://onecrewai.io");
    expect(manifest.author.name).toBe("onecrew_lite");
    expect(manifest.interface.displayName).toBe("OneCrew Lite");
    expect(manifest.interface.capabilities).toContain("Read");
    expect(manifest.interface.defaultPrompt.length).toBeGreaterThan(0);
    expect(manifest.hooks).toBeUndefined();
  });
  it("contains the declared skill resources without credentials or local connection state", () => {
    const files: string[] = [];
    const walk = (directory: string) => {
      for (const entry of readdirSync(directory, { withFileTypes: true })) {
        const path = join(directory, entry.name);
        if (entry.isDirectory()) walk(path); else files.push(path);
      }
    };
    walk(plugin);
    expect(files.some(path => path.endsWith("SKILL.md"))).toBe(true);
    for (const file of files) {
      const text = readFileSync(file, "utf8");
      expect(text).not.toMatch(/\bauthorization["']?\s*[:=]|\bbearer\s+[a-z0-9._~+/-]{12,}/i);
      expect(text).not.toContain("onecrew_ticket=");
      expect(text).not.toContain("control_token");
    }
  });
});
