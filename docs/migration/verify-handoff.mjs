import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { access, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
execFileSync(process.execPath, ["development/check-independence.mts"], { cwd: root, stdio: "inherit" });
for (const path of [
  "pnpm-lock.yaml", "development/README.md", "onecrew_lite-app/src/cli.ts",
  "onecrew_lite-web/package.json", "onecrew_lite-mcp-ui/package.json",
  "onecrew_lite-library/package.json", "onecrew_lite-app/tests/fixtures/creator-history/schemas.zip",
  ".github/workflows/workspace-candidates.yml",
]) await access(new URL("../../" + path, import.meta.url));
const packageJson = JSON.parse(await readFile(new URL("../../package.json", import.meta.url), "utf8"));
assert.equal(packageJson.engines.node, "26.7.0");
assert.equal(packageJson.packageManager, "pnpm@10.32.1");
for (const [config, source] of [
  ["onecrew_lite-web/creator-sync-contracts.config.json", "onecrew_lite-contracts/schemas/web-creator-sync.json"],
  ["onecrew_lite-web/discovery-contracts.config.json", "onecrew_lite-contracts/schemas/web-discovery.json"],
  ["onecrew_lite-mcp-ui/contracts.config.json", "onecrew_lite-contracts/schemas/mcp-app.json"],
]) {
  const path = new URL("../../" + config, import.meta.url);
  const value = JSON.parse(await readFile(path, "utf8"));
  assert.equal(fileURLToPath(new URL(value.manifest, path)), fileURLToPath(new URL("../../" + source, import.meta.url)));
  const manifest = JSON.parse(await readFile(new URL(value.manifest, path), "utf8"));
  assert.equal(manifest.schema_version, 1);
  assert.equal(manifest.target, value.target);
  assert.ok(manifest.roots.length > 0);
}
console.log(JSON.stringify({ sourceLayoutVerified: true, runtimeAcceptance: "not_checked_by_this_command", macAcceptance: "incomplete" }));
