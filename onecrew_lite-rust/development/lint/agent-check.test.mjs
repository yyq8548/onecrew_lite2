import assert from "node:assert/strict";
import test from "node:test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

import {
  computeEvidenceKey,
  commandInvocation,
  changedTrackedArgs,
  formatProgressLine,
  parseArguments,
  parseStructuredFailure,
  rustDocumentationSteps,
  selectCheckGroups,
  shouldReuseEvidence,
} from "../scripts/agent-check.mjs";

test("Windows npm uses Node without shell interpolation or argument loss", () => {
  const args = ["run", "test", "--", "folder with spaces", "literal&value"];
  const entrypoint = "C:\\Program Files\\nodejs\\node_modules\\npm\\bin\\npm-cli.js";
  assert.deepEqual(commandInvocation("npm", args, {
    platform: "win32", execPath: "C:\\Program Files\\nodejs\\node.exe", env: {},
    pathExists: (candidate) => candidate === entrypoint,
  }), ["C:\\Program Files\\nodejs\\node.exe", [entrypoint, ...args]]);
  for (const [platform, command] of [["linux", "npm"], ["win32", "cargo"]]) {
    assert.deepEqual(commandInvocation(command, args, { platform }), [command, args]);
  }
});

test("Windows npm resolves PATH installations and retains missing-tool errors", () => {
  const options = {
    platform: "win32", execPath: "C:\\node\\node.exe", env: { Path: '"D:\\tools with spaces"' },
    pathExists: (candidate) => candidate === "D:\\tools with spaces\\node_modules\\npm\\bin\\npm-cli.js",
  };
  assert.deepEqual(commandInvocation("npm", ["--version"], options), [options.execPath,
    ["D:\\tools with spaces\\node_modules\\npm\\bin\\npm-cli.js", "--version"]]);
  assert.deepEqual(commandInvocation("npm", [], { ...options, pathExists: () => false }), ["npm", []]);
});

test("installed npm launches through the runner on Windows", { skip: process.platform !== "win32" }, () => {
  const result = spawnSync(...commandInvocation("npm", ["--version"]), { encoding: "utf8", shell: false });
  assert.equal(result.error, undefined);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout.trim(), /^\d+\.\d+\.\d+/);
});

test("nested workspaces fingerprint local changes without sibling paths", () => {
  const root = mkdtempSync(path.join(tmpdir(), "onecrew-runner-"));
  const nested = path.join(root, "0925", "onecrew-rust");
  const git = (args, cwd = root) => {
    const result = spawnSync("git", args, { cwd, encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout;
  };
  try {
    mkdirSync(nested, { recursive: true });
    git(["init"]);
    writeFileSync(path.join(nested, "local.txt"), "before\n");
    writeFileSync(path.join(root, "sibling.txt"), "before\n");
    git(["add", "."]);
    git(["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", "commit", "-m", "fixture"]);
    writeFileSync(path.join(nested, "local.txt"), "after\n");
    writeFileSync(path.join(root, "sibling.txt"), "after\n");
    assert.equal(git(changedTrackedArgs, nested), "local.txt\0");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});


test("evidence identity includes command, cwd, diff, and dependency fingerprints", () => {
  const base = {
    argv: ["uv", "run", "check.py"],
    cwd: ".",
    relevantDiffFingerprint: "diff-a",
    dependencyFingerprint: "deps-a",
  };
  const identity = computeEvidenceKey(base);

  assert.equal(identity, computeEvidenceKey({ ...base }));
  assert.notEqual(
    identity,
    computeEvidenceKey({ ...base, relevantDiffFingerprint: "diff-b" }),
  );
  assert.notEqual(
    identity,
    computeEvidenceKey({ ...base, dependencyFingerprint: "deps-b" }),
  );
  assert.notEqual(identity, computeEvidenceKey({ ...base, cwd: "apps/web" }));
  assert.notEqual(
    identity,
    computeEvidenceKey({ ...base, argv: ["uv", "run", "other.py"] }),
  );
});

test("only an identical PASS is reusable", () => {
  const evidenceKey = "same";
  assert.equal(
    shouldReuseEvidence({ evidenceKey, status: "PASS" }, evidenceKey),
    true,
  );
  for (const status of ["FAIL", "BLOCKED", undefined]) {
    assert.equal(shouldReuseEvidence({ evidenceKey, status }, evidenceKey), false);
  }
  assert.equal(
    shouldReuseEvidence({ evidenceKey: "different", status: "PASS" }, evidenceKey),
    false,
  );
});

test("progress lines expose the active step without corrupting final JSON stdout", () => {
  assert.equal(
    formatProgressLine({
      index: 7,
      status: "START",
      name: "cargo documentation",
      logPath: ".agent-check/run/07-cargo-documentation.log",
    }),
    "[agent-check 07] START cargo documentation log=.agent-check/run/07-cargo-documentation.log",
  );
  assert.equal(
    formatProgressLine({
      index: 7,
      status: "BLOCKED",
      name: "cargo documentation",
      durationMs: 1_250,
      code: "disk_full",
      logPath: ".agent-check/run/07-cargo-documentation.log",
    }),
    "[agent-check 07] BLOCKED cargo documentation (1.3s) code=disk_full log=.agent-check/run/07-cargo-documentation.log",
  );
});

test("structured readiness blockers retain their code", () => {
  const payload = JSON.stringify({
    status: "blocked",
    checks: [
      { status: "pass", code: null },
      { status: "blocked", code: "product_domain_worktree_dirty" },
    ],
  });
  assert.deepEqual(parseStructuredFailure(`header\n${payload}\ntrailer`), {
    status: "BLOCKED",
    code: "product_domain_worktree_dirty",
  });
  assert.deepEqual(parseStructuredFailure("plain compiler failure"), {
    status: "FAIL",
    code: null,
  });
});

test("reuse evidence is explicit and optional", () => {
  assert.deepEqual(parseArguments(["--profile", "skills"]), {
    profile: "skills",
    reuseEvidence: null,
    help: false,
    errors: [],
  });
  assert.deepEqual(
    parseArguments(["--profile=skills", "--reuse-evidence", "result.json"]),
    {
      profile: "skills",
      reuseEvidence: "result.json",
      help: false,
      errors: [],
    },
  );
});

test("Rust documentation and doctests are separate commit evidence", () => {
  assert.deepEqual(
    rustDocumentationSteps.map(({ name, command, args }) => ({
      name,
      command,
      args: [...args],
    })),
    [
      {
        name: "cargo documentation",
        command: "cargo",
        args: ["doc", "--workspace", "--no-deps"],
      },
      {
        name: "cargo doctest",
        command: "cargo",
        args: ["test", "--workspace", "--doc"],
      },
    ],
  );
});

test("schema generator changes select unit tests and consumer drift checks", () => {
  assert.deepEqual(
    selectCheckGroups("commit", ["development/schema-codegen/generate.mjs"]),
    {
      cargo: false,
      npm: true,
      schemaCodegen: true,
      chatXdk: false,
      juiceboxSdk: false,
    },
  );
  assert.deepEqual(
    selectCheckGroups("commit", [
      "apps/mcp-ui/src/lib/generated/contracts/index.ts",
    ]),
    {
      cargo: false,
      npm: true,
      schemaCodegen: true,
      chatXdk: false,
      juiceboxSdk: false,
    },
  );
  assert.equal(selectCheckGroups("full", []).schemaCodegen, true);
  assert.equal(selectCheckGroups("skills", []).schemaCodegen, false);
});

test("Router drift inputs select the cargo drift test and the npm validators", () => {
  for (const touchedPath of [
    "integrations/codex-marketplace/plugins/onecrew/skills/onecrew/SKILL.md",
    "integrations/codex-marketplace/plugins/onecrew/.codex-plugin/plugin.json",
    "apps/mcp-ui/src/test-support/fixtures.ts",
    "apps/mcp-ui/preview/20-scenarios.json",
    "apps/web/src/lib/runtime-change.ts",
  ]) {
    const selection = selectCheckGroups("commit", [touchedPath]);
    assert.equal(selection.cargo, true, touchedPath);
    assert.equal(selection.npm, true, touchedPath);
  }
  for (const touchedPath of [
    "apps/mcp-ui/preview/main.tsx",
    "apps/web/src/lib/runtime-change.test.ts",
  ]) {
    assert.equal(selectCheckGroups("commit", [touchedPath]).cargo, false, touchedPath);
  }
});
