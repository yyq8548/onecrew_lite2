#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const skillRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = path.resolve(skillRoot, "../../..");
const skill = "onecrew-rust-quality";
const anchors = [
  { path: "Cargo.toml", symbol: "[workspace]" },
  { path: "development/lint/agent-context.config.json", symbol: '"maxSkillBodyLines"' },
  { path: "development/lint/project-skill-ownership.json", symbol: '"onecrew-rust-quality"' },
  { path: ".skillshare/skills/onecrew-rust-architecture/SKILL.md", symbol: "name: onecrew-rust-architecture" },
  { path: ".skillshare/skills/onecrew-verify/references/command-matrix.md", symbol: "### Root Rust crate" },
];
const evalCases = [
  ["evals/cases/golden-error-boundary.yaml", "golden", "golden_error_boundary"],
  ["evals/cases/golden-unsafe-wrapper.yaml", "golden", "golden_unsafe_wrapper"],
  ["evals/cases/golden-workspace-policy.yaml", "golden", "golden_workspace_policy"],
  ["evals/cases/negative-structure-only.yaml", "negative", "negative_structure_only"],
  ["evals/cases/negative-verification-only.yaml", "negative", "negative_verification_only"],
  ["evals/cases/verification-final-quality.yaml", "verification", "verification_final_quality"],
];
const referenceFiles = [
  "references/error-panic-and-boundaries.md",
  "references/unsafe-proof-and-safe-abstractions.md",
  "references/api-rustdoc-and-traits.md",
  "references/cargo-lints-features-and-msrv.md",
];
const domainScopes = [
  "Cargo.toml",
  "Cargo.lock",
  ".cargo/",
  "clippy.toml",
  ".clippy.toml",
  "rust-toolchain",
  "rust-toolchain.toml",
  "rustfmt.toml",
  ".rustfmt.toml",
  "crates/",
  "crates/local-service/",
  "libs/",
];
const checks = [];

function record(id, kind, status, extra = {}) {
  checks.push({ id, kind, status, ...extra });
}

for (const anchor of anchors) {
  const absolute = path.join(repoRoot, anchor.path);
  if (!existsSync(absolute)) {
    record(`anchor:${anchor.path}`, "anchor", "fail", {
      code: "anchor_missing",
      path: anchor.path,
    });
    continue;
  }
  const source = readFileSync(absolute, "utf8");
  const present = source.includes(anchor.symbol);
  record(`anchor:${anchor.path}#${anchor.symbol}`, "anchor", present ? "pass" : "fail", {
    code: present ? null : "symbol_missing",
    path: anchor.path,
    symbol: anchor.symbol,
  });
}

for (const relative of referenceFiles) {
  const absolute = path.join(skillRoot, relative);
  const source = existsSync(absolute) ? readFileSync(absolute, "utf8") : "";
  const valid = source.includes("## Critical Rules") &&
    source.includes("## Quick Reference") &&
    source.includes("## Common Patterns") &&
    source.includes("## Checklist");
  record(`reference:${relative}`, "reference", valid ? "pass" : "fail", {
    code: valid ? null : "reference_pattern_missing",
    path: relative,
  });
}

const manifestPath = path.join(skillRoot, "evals/manifest.yaml");
const rubricPath = path.join(skillRoot, "evals/rubric.md");
const manifest = existsSync(manifestPath) ? readFileSync(manifestPath, "utf8") : "";
const manifestOk = manifest.includes(`skill: ${skill}`) &&
  manifest.includes("pass_threshold: 90") &&
  manifest.includes("critical_fail_policy: zero");
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

const rootManifest = readFileSync(path.join(repoRoot, "Cargo.toml"), "utf8");
record("policy:workspace-lints", "observation", "pass", {
  code: rootManifest.includes("[workspace.lints") ? "declared" : "unresolved",
  proof: "declaration_observed_not_quality_graded",
});
record("policy:workspace-rust-version", "observation", "pass", {
  code: /rust-version\s*=/.test(rootManifest) ? "declared" : "unresolved",
  proof: "declaration_observed_not_msrv_verified",
});

function gitPaths(args) {
  return execFileSync("git", args, { cwd: repoRoot, encoding: "utf8" })
    .split("\n").map((value) => value.trim()).filter(Boolean);
}

const dirty = [...new Set([
  ...gitPaths(["diff", "--name-only", "--"]),
  ...gitPaths(["diff", "--cached", "--name-only", "--"]),
  ...gitPaths(["ls-files", "--others", "--exclude-standard", "--"]),
])].filter((candidate) => domainScopes.some((scope) =>
  candidate === scope || candidate.startsWith(scope)));
// A dirty path may be the authorized quality change itself. Report evidence;
// only the task owner can decide whether overlapping work conflicts with intent.
record("domain:worktree", "worktree", "pass", {
  code: dirty.length ? "product_domain_worktree_dirty" : null,
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
  summary: {
    anchorCount: anchors.length,
    referenceCount: referenceFiles.length,
    evalCaseCount: evalCases.length,
  },
  checks,
  note: "Readiness checks stable anchors, reference/eval assets, and product-domain drift only. Existing Rust debt and code quality are not graded or proved.",
})}\n`);
process.exitCode = status === "pass" ? 0 : status === "blocked" ? 2 : 1;
