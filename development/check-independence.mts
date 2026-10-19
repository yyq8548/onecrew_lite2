import { lstat, readFile, readdir } from "node:fs/promises";
import { extname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const retired = ["onecrew_lite", "rust"].join("-");
const previous = await lstat(join(root, retired)).then(() => true, error => {
  if (error.code === "ENOENT") return false;
  throw error;
});
if (previous) throw new Error("independence.previous_project_present");
const sourceExtensions = new Set([".ts", ".tsx", ".mts", ".cts", ".js", ".mjs", ".cjs", ".ps1", ".sh", ".cmd", ".bat", ".yml", ".yaml", ".json"]);
let checked = 0;
async function inspect(directory: string): Promise<void> {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (["node_modules", "dist", "vendor", ".git", ".scratch"].includes(entry.name) || entry.name.startsWith(".schema-codegen-")) continue;
    const path = join(directory, entry.name), name = relative(root, path);
    if (entry.isSymbolicLink()) throw new Error(`independence.source_alias:${name}`);
    if (entry.isDirectory()) { await inspect(path); continue; }
    if (entry.name === "Cargo.toml" || entry.name === "Cargo.lock" || extname(entry.name) === ".rs") throw new Error(`independence.previous_language_source:${name}`);
    if (!sourceExtensions.has(extname(entry.name)) || entry.name.endsWith("provenance.json")) continue;
    const text = await readFile(path, "utf8");
    if (text.includes(retired + "/") || text.includes(retired + "\\") || /["'`]cargo(?:\.exe)?["'`]/u.test(text)) throw new Error(`independence.previous_project_reference:${name}`);
    checked++;
  }
}
for (const directory of ["onecrew_lite-app", "onecrew_lite-contracts", "onecrew_lite-web", "onecrew_lite-mcp-ui", "onecrew_lite-opencli", "onecrew_lite-library", "development", "docs", ".github"]) await inspect(join(root, directory));
for (const name of ["package.json", "pnpm-workspace.yaml", "pnpm-lock.yaml"]) {
  if ((await readFile(join(root, name), "utf8")).includes(retired)) throw new Error(`independence.previous_workspace_reference:${name}`);
  checked++;
}
console.log(`Independent TypeScript project verified: ${checked} source and configuration files; previous project absent.`);
