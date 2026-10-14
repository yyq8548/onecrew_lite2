// @vitest-environment node

import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

describe("Desktop pairing source contract", () => {
  it("contains no browser persistence or logging path for pairing material", async () => {
    const panel = await readFile(
      fileURLToPath(new URL("./device-access-panel.tsx", import.meta.url)),
      "utf8",
    );
    const state = await readFile(
      fileURLToPath(new URL("../../lib/use-device-access-state.ts", import.meta.url)),
      "utf8",
    );
    const pairing = await readFile(
      fileURLToPath(new URL("../pairing/pairing-authorization-page.tsx", import.meta.url)),
      "utf8",
    );
    const api = await readFile(
      fileURLToPath(new URL("../../lib/access-api.ts", import.meta.url)),
      "utf8",
    );
    const workerPairing = await Promise.all(
      [
        "approval-token.ts",
        "crypto.ts",
        "desktop-pairing-admission-contract.ts",
        "service.ts",
        "../routes/pairing.ts",
      ].map((path) =>
        readFile(
          fileURLToPath(new URL(`../../../../src/pairing/${path}`, import.meta.url)),
          "utf8",
        ),
      ),
    );
    const source = `${panel}\n${state}\n${pairing}`;
    const sensitiveSource = `${source}\n${api}\n${workerPairing.join("\n")}`;

    expect(sensitiveSource).not.toMatch(/localStorage|sessionStorage|indexedDB/);
    expect(sensitiveSource).not.toMatch(/console\.(log|info|warn|error)/);
    expect(sensitiveSource).not.toMatch(/analytics|toast|clipboard\.writeText/);
    expect(source).not.toMatch(/URLSearchParams|history\./);
    expect(pairing).not.toMatch(/<a\s|href=/);
    expect(pairing).toContain('action="/api/pairing/authorize"');
    expect(panel).toContain("flex-col gap-2 sm:w-auto sm:flex-row");
    expect(panel).toContain("min-w-0");
    expect(panel).toContain("w-full sm:w-auto");
  });
});
