#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const skillRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = path.resolve(skillRoot, "../../..");
const skill = "onecrew-local-data-evolution";
const anchors = [
  { path: "crates/creator-runtime/src/repository/sqlite/schema.rs", symbol: "CURRENT_SCHEMA_VERSION" },
  { path: "crates/creator-runtime/src/repository/sqlite/schema/migrations/mod.rs", symbol: "FORWARD_MIGRATIONS" },
  { path: "crates/creator-runtime/src/repository/sqlite/schema/upgrade/orchestrator.rs", symbol: "fn upgrade" },
  { path: "crates/platform-runtime/src/data_upgrade.rs", symbol: "LocalDataUpgradeStatus" },
  { path: "crates/local-service/src/startup.rs", symbol: "enum StartupPhase" },
];
const evalCases = [
  ["evals/cases/golden-adjacent-upgrade.yaml", "golden"],
  ["evals/cases/negative-current-query.yaml", "negative"],
  ["evals/cases/verification-recovery.yaml", "verification"],
];
const domainScopes = [
  "crates/creator-runtime/src/repository/sqlite/schema",
  "crates/local-vault/src/store/schema.rs",
  "crates/platform-runtime/src/data_upgrade.rs",
  "crates/local-service/src/startup.rs",
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

const manifestPath = path.join(skillRoot, "evals/manifest.yaml");
const rubricPath = path.join(skillRoot, "evals/rubric.md");
const manifest = existsSync(manifestPath) ? readFileSync(manifestPath, "utf8") : "";
const manifestOk = manifest.includes(`skill: ${skill}`) && manifest.includes("pass_threshold: 90") && manifest.includes("critical_fail_policy: zero");
record("eval-asset:manifest", "eval_asset", manifestOk ? "pass" : "fail", {
  code: manifestOk ? null : "eval_schema_invalid",
  path: "evals/manifest.yaml",
  proof: manifestOk ? "schema_present" : "schema_missing_or_invalid",
});

for (const [relative, type] of evalCases) {
  const absolute = path.join(skillRoot, relative);
  const source = existsSync(absolute) ? readFileSync(absolute, "utf8") : "";
  const valid = manifest.includes(`- ${relative.replace("evals/", "")}`) &&
    source.includes(`type: ${type}`) && source.includes("must_include:") &&
    source.includes("must_not_include:") && source.includes("critical_fail_if:") &&
    source.includes("pass: 90");
  record(`eval-asset:${type}`, "eval_asset", valid ? "pass" : "fail", {
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
// A migration delivery necessarily changes these paths. Preserve the overlap
// evidence; authorization and migration correctness require separate review.
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
