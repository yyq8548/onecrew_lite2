#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const skillRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = path.resolve(skillRoot, "../../..");
const skill = "onecrew-peer-adapters";
const anchors = [
  { path: "crates/mcp-adapter/src/tools.rs", symbol: "surface_tool_router" },
  { path: "crates/mcp-adapter/src/operation_registry/mod.rs", symbol: "build_registry" },
  { path: "crates/mcp-adapter/src/operation_registry/registration/registry.rs", symbol: "struct OperationRegistry" },
  { path: "crates/mcp-adapter/src/operation_registry/descriptor/identity.rs", symbol: "enum SurfaceId" },
  { path: "crates/mcp-adapter/src/operation_registry/handle.rs", symbol: "struct OperationRegistryHandle" },
  { path: "crates/schema-manifest/src/lib.rs", symbol: "pub fn contract_schema_root" },
  { path: "development/schema-codegen/generate.mjs", symbol: "function validateManifest" },
  { path: "development/schema-codegen/generate.mjs", symbol: "async function compareDirectories" },
  { path: "development/schema-codegen/generate.mjs", symbol: "async function replaceDirectory" },
  { path: "apps/web/creator-sync-contracts.config.json", symbol: '"target": "web-creator-sync"' },
  { path: "apps/mcp-ui/contracts.config.json", symbol: '"target": "mcp-app"' },
];
const evalCases = [
  ["evals/cases/golden-projection.yaml", "golden", "golden"],
  ["evals/cases/negative-widget-layout.yaml", "negative", "negative_widget_layout"],
  ["evals/cases/negative-mcp-app-host-contract.yaml", "negative", "negative_mcp_app_host_contract"],
  ["evals/cases/golden-contract-codegen.yaml", "golden", "golden_contract_codegen"],
  ["evals/cases/negative-generated-contract-edit.yaml", "negative", "negative_generated_contract_edit"],
  ["evals/cases/verification-contract-drift.yaml", "verification", "verification_contract_drift"],
  ["evals/cases/verification-write-safety.yaml", "verification", "verification"],
];
const domainScopes = [
  "crates/mcp-adapter/Cargo.toml",
  "crates/mcp-adapter/src/",
  "crates/local-service/src/",
  "crates/schema-manifest/",
  "development/schema-codegen/",
  "apps/web/creator-sync-contracts.config.json",
  "apps/web/src/lib/generated/creator-sync-contracts/",
  "apps/mcp-ui/contracts.config.json",
  "apps/mcp-ui/src/lib/generated/contracts/",
];
const checks = [];

function record(id, kind, status, extra = {}) {
  checks.push({ id, kind, status, ...extra });
}

for (const anchor of anchors) {
  const absolute = path.join(repoRoot, anchor.path);
  if (!existsSync(absolute)) {
    record(`anchor:${anchor.path}`, "anchor", "fail", { code: "anchor_missing", path: anchor.path });
    continue;
  }
  const source = readFileSync(absolute, "utf8");
  record(`anchor:${anchor.path}#${anchor.symbol}`, "anchor", source.includes(anchor.symbol) ? "pass" : "fail", {
    code: source.includes(anchor.symbol) ? null : "symbol_missing",
    path: anchor.path,
    symbol: anchor.symbol,
  });
}

function readJson(relativePath) {
  return JSON.parse(readFileSync(path.join(repoRoot, relativePath), "utf8"));
}

const generatorPackage = readJson("development/schema-codegen/package.json");
const dependencies = Object.values(generatorPackage.dependencies ?? {});
const dependenciesPinned = dependencies.length > 0 && dependencies.every(
  (version) => typeof version === "string" && /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version),
);
record("codegen:pinned-dependencies", "codegen", dependenciesPinned ? "pass" : "fail", {
  code: dependenciesPinned ? null : "generator_dependency_not_pinned",
  path: "development/schema-codegen/package.json",
});

for (const [consumer, packagePath] of [
  ["desktop", "apps/web/package.json"],
  ["mcp_app", "apps/mcp-ui/package.json"],
]) {
  const scripts = readJson(packagePath).scripts ?? {};
  const generateCommand = scripts["contracts:generate"] ?? "";
  const checkCommand = scripts["contracts:check"] ?? "";
  const ordinaryCommands = Object.entries(scripts)
    .filter(([name]) => name !== "contracts:generate" && name !== "contracts:check")
    .map(([, command]) => command);
  const valid = generateCommand.includes("--mode generate") &&
    checkCommand.includes("--mode check") &&
    ordinaryCommands.every((command) =>
      typeof command === "string" &&
      !command.includes("contracts:generate") &&
      !command.includes("--mode generate"));
  record(`codegen:${consumer}-scripts`, "codegen", valid ? "pass" : "fail", {
    code: valid ? null : "unsafe_consumer_codegen_scripts",
    path: packagePath,
  });
}

const agentCheckPath = "development/scripts/agent-check.mjs";
const agentCheckSource = readFileSync(path.join(repoRoot, agentCheckPath), "utf8");
const gateCoverage = agentCheckSource.includes('name: "schema codegen unit tests"') &&
  agentCheckSource.includes('name: "MCP App contract drift"') &&
  agentCheckSource.includes('name: "Web contract drift"');
record("codegen:commit-gates", "codegen", gateCoverage ? "pass" : "fail", {
  code: gateCoverage ? null : "codegen_gate_missing",
  path: agentCheckPath,
});

const manifestPath = path.join(skillRoot, "evals/manifest.yaml");
const rubricPath = path.join(skillRoot, "evals/rubric.md");
const manifest = existsSync(manifestPath) ? readFileSync(manifestPath, "utf8") : "";
const manifestOk = manifest.includes(`skill: ${skill}`) && manifest.includes("pass_threshold: 90") && manifest.includes("critical_fail_policy: zero");
record("eval-asset:manifest", "eval_asset", manifestOk ? "pass" : "fail", {
  code: manifestOk ? null : "eval_schema_invalid",
  path: "evals/manifest.yaml",
  proof: manifestOk ? "schema_present" : "schema_missing_or_invalid",
});

for (const [relative, type, id] of evalCases) {
  const absolute = path.join(skillRoot, relative);
  const source = existsSync(absolute) ? readFileSync(absolute, "utf8") : "";
  const valid = manifest.includes(`- ${relative.replace("evals/", "")}`) &&
    source.includes(`type: ${type}`) && source.includes("must_include:") &&
    source.includes("must_not_include:") && source.includes("critical_fail_if:") &&
    source.includes("pass: 90");
  record(`eval-asset:${id}`, "eval_asset", valid ? "pass" : "fail", {
    code: valid ? null : "eval_schema_invalid",
    path: relative,
    proof: valid ? "schema_present" : "schema_missing_or_invalid",
  });
}

const rubric = existsSync(rubricPath) ? readFileSync(rubricPath, "utf8") : "";
const rubricOk = rubric.includes("## Pass Criteria") && rubric.includes("## Critical Fail Criteria");
record("eval-asset:rubric", "eval_asset", rubricOk ? "pass" : "fail", {
  code: rubricOk ? null : "eval_schema_invalid",
  path: "evals/rubric.md",
  proof: rubricOk ? "schema_present" : "schema_missing_or_invalid",
});

function gitPaths(args) {
  return execFileSync("git", args, { cwd: repoRoot, encoding: "utf8" })
    .split("\n").map((value) => value.trim()).filter(Boolean);
}

const dirty = [...new Set([
  ...gitPaths(["diff", "--name-only", "--"]),
  ...gitPaths(["diff", "--cached", "--name-only", "--"]),
  ...gitPaths(["ls-files", "--others", "--exclude-standard", "--"]),
])].filter((candidate) => domainScopes.some((scope) => candidate.startsWith(scope)));
// These paths may be the authorized adapter change itself. Record overlap;
// task scope and semantic owner review decide whether it is a conflict.
record("domain:worktree", "worktree", "pass", {
  code: dirty.length ? "domain_worktree_dirty" : null,
  paths: dirty,
  proof: "paths_observed_not_intent_classified",
});

const failed = checks.some((check) => check.status === "fail");
const blocked = !failed && checks.some((check) => check.status === "blocked");
const status = failed ? "fail" : blocked ? "blocked" : "pass";
process.stdout.write(`${JSON.stringify({
  schemaVersion: 1,
  skill,
  status,
  summary: { anchorCount: anchors.length, evalCaseCount: evalCases.length },
  checks,
  note: "Eval asset readiness passed only when schema fields are present; no Agent output or grader ran.",
})}\n`);
process.exitCode = status === "pass" ? 0 : status === "blocked" ? 2 : 1;
