import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const checkOnly = process.argv.slice(2).includes("--check");
const unknownArgs = process.argv.slice(2).filter((arg) => arg !== "--check");
if (unknownArgs.length > 0) throw new Error(`unknown arguments: ${unknownArgs.join(" ")}`);

const versionSourcePath = resolve(root, "development/release/version.json");
const versionSource = JSON.parse(readFileSync(versionSourcePath, "utf8"));
const version = versionSource.cliPluginBase;
if (typeof version !== "string" || !/^\d+\.\d+\.\d+$/.test(version)) {
  throw new Error("cliPluginBase must be an exact semantic version");
}
const creatorSchemaRelease = versionSource.creatorCurrentSchemaIntroducedInRelease;
if (typeof creatorSchemaRelease !== "string" || !/^\d+\.\d+\.\d+$/.test(creatorSchemaRelease)) {
  throw new Error("creatorCurrentSchemaIntroducedInRelease must be an exact semantic version");
}
const vaultSchemaRelease = versionSource.vaultCurrentSchemaIntroducedInRelease;
if (typeof vaultSchemaRelease !== "string" || !/^\d+\.\d+\.\d+$/.test(vaultSchemaRelease)) {
  throw new Error("vaultCurrentSchemaIntroducedInRelease must be an exact semantic version");
}

const updates = new Map();
function updateJson(relativePath, mutate) {
  const path = resolve(root, relativePath);
  const value = JSON.parse(readFileSync(path, "utf8"));
  mutate(value);
  updates.set(path, `${JSON.stringify(value, null, 2)}\n`);
}

updateJson("apps/web/package.json", (value) => {
  value.version = version;
});
updateJson("apps/web/package-lock.json", (value) => {
  value.version = version;
  value.packages[""].version = version;
});
updateJson("integrations/codex-marketplace/plugins/onecrew/.codex-plugin/plugin.json", (value) => {
  value.version = version;
});

function replacePackageVersion(relativePath, packageName) {
  const path = resolve(root, relativePath);
  const source = updates.get(path) ?? readFileSync(path, "utf8");
  const pattern = new RegExp(`(name = "${packageName}"\\r?\\nversion = ")[^"]+("\\r?\\n)`);
  if (!pattern.test(source)) throw new Error(`${relativePath} has no ${packageName} version`);
  updates.set(path, source.replace(pattern, `$1${version}$2`));
}

replacePackageVersion("crates/onecrew/Cargo.toml", "onecrew");
replacePackageVersion("Cargo.lock", "onecrew");
replacePackageVersion("crates/local-service/Cargo.toml", "onecrew-local-service");
replacePackageVersion("Cargo.lock", "onecrew-local-service");

const drift = [];
for (const [path, expected] of updates) {
  if (readFileSync(path, "utf8") === expected) continue;
  drift.push(path.slice(root.length + 1));
  if (!checkOnly) writeFileSync(path, expected);
}

if (drift.length > 0 && checkOnly) {
  throw new Error(`release version mirrors are stale: ${drift.join(", ")}`);
}
console.log(`${checkOnly ? "checked" : "synced"} CLI/web/plugin base ${version}`);
