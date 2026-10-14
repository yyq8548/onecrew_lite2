#!/usr/bin/env node

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { lstat, mkdir, open, readFile, readlink, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptPath = fileURLToPath(import.meta.url);
const repoRoot = path.resolve(path.dirname(scriptPath), "../..");
const supportedProfiles = new Set(["skills", "changed", "commit", "full"]);
export const changedTrackedArgs = Object.freeze([
  "diff", "--relative", "--name-only", "--diff-filter=ACDMRTUXB", "-z", "HEAD", "--", ".",
]);

// Windows cannot spawn npm's .cmd shim without a shell. Invoke npm's actual
// JavaScript entrypoint instead, preserving each argument (including spaces).
export function commandInvocation(command, args, {
  platform = process.platform,
  execPath = process.execPath,
  env = process.env,
  pathExists = existsSync,
} = {}) {
  if (platform !== "win32" || command !== "npm") return [command, args];
  const searchPath = Object.entries(env).find(([key]) => key.toLowerCase() === "path")?.[1] ?? "";
  const directories = [path.win32.dirname(execPath), ...searchPath.split(";").filter(Boolean)];
  for (const directory of directories) {
    const entrypoint = path.win32.join(directory.replace(/^"|"$/g, ""), "node_modules/npm/bin/npm-cli.js");
    if (pathExists(entrypoint)) return [execPath, [entrypoint, ...args]];
  }
  // Preserve normal spawn-error reporting when npm is not installed.
  return [command, args];
}
const rustDocumentationSteps = Object.freeze([
  Object.freeze({
    name: "cargo documentation",
    command: "cargo",
    args: Object.freeze(["doc", "--workspace", "--no-deps"]),
  }),
  Object.freeze({
    name: "cargo doctest",
    command: "cargo",
    args: Object.freeze(["test", "--workspace", "--doc"]),
  }),
]);

function repoRelative(absolutePath) {
  return path.relative(repoRoot, absolutePath).split(path.sep).join("/") || ".";
}

function shellQuote(value) {
  if (/^[A-Za-z0-9_./:=+,-]+$/.test(value)) {
    return value;
  }
  return `'${value.replaceAll("'", `'"'"'`)}'`;
}

function displayCommand(command, args) {
  return [command, ...args].map(shellQuote).join(" ");
}

function safeStepName(name) {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

function formatProgressLine({
  index,
  status,
  name,
  durationMs = null,
  logPath = null,
  code = null,
  reusedFromRunId = null,
}) {
  const step = String(index).padStart(2, "0");
  const duration =
    durationMs === null
      ? null
      : durationMs < 1_000
        ? `${durationMs}ms`
        : `${(durationMs / 1_000).toFixed(1)}s`;
  return [
    `[agent-check ${step}]`,
    status,
    name,
    duration ? `(${duration})` : null,
    code ? `code=${code}` : null,
    reusedFromRunId ? `reused=${reusedFromRunId}` : null,
    logPath ? `log=${logPath}` : null,
  ]
    .filter(Boolean)
    .join(" ");
}

function reportProgress(event) {
  process.stderr.write(`${formatProgressLine(event)}\n`);
}

function parseStructuredFailure(contents) {
  for (const line of contents.split("\n")) {
    if (!line.startsWith("{")) {
      continue;
    }
    try {
      const parsed = JSON.parse(line);
      const failedCheck = parsed.checks?.find(
        (check) => check.status === "blocked" || check.status === "fail",
      );
      if (failedCheck?.code) {
        return {
          status: failedCheck.status === "blocked" ? "BLOCKED" : "FAIL",
          code: failedCheck.code,
        };
      }
      if (parsed.status === "blocked") {
        return { status: "BLOCKED", code: parsed.code ?? null };
      }
    } catch {
      // Command logs may contain arbitrary non-JSON lines beginning with `{`.
    }
  }
  return { status: "FAIL", code: null };
}

async function structuredFailureCode(logAbsolute) {
  return parseStructuredFailure(await readFile(logAbsolute, "utf8"));
}

function computeEvidenceKey({ argv, cwd, relevantDiffFingerprint, dependencyFingerprint }) {
  return createHash("sha256")
    .update(
      JSON.stringify({
        argv,
        cwd,
        relevantDiffFingerprint,
        dependencyFingerprint,
        contractVersion: 1,
      }),
    )
    .digest("hex");
}

function shouldReuseEvidence(priorStep, evidenceKey) {
  return Boolean(
    priorStep &&
      priorStep.evidenceKey === evidenceKey &&
      priorStep.status === "PASS",
  );
}

function parseArguments(argv) {
  let profile = "changed";
  let reuseEvidence = null;
  let help = false;
  const errors = [];

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--help" || argument === "-h") {
      help = true;
      continue;
    }
    if (argument === "--profile") {
      const value = argv[index + 1];
      if (!value || value.startsWith("-")) {
        errors.push("--profile requires a value.");
      } else {
        profile = value;
        index += 1;
      }
      continue;
    }
    if (argument.startsWith("--profile=")) {
      profile = argument.slice("--profile=".length);
      continue;
    }
    if (argument === "--reuse-evidence") {
      const value = argv[index + 1];
      if (!value || value.startsWith("-")) {
        errors.push("--reuse-evidence requires a result JSON path.");
      } else {
        reuseEvidence = value;
        index += 1;
      }
      continue;
    }
    if (argument.startsWith("--reuse-evidence=")) {
      reuseEvidence = argument.slice("--reuse-evidence=".length);
      continue;
    }
    errors.push(`Unknown argument: ${argument}`);
  }

  if (!supportedProfiles.has(profile)) {
    errors.push(
      `Unsupported profile ${JSON.stringify(profile)}; expected skills, changed, commit, or full.`,
    );
  }
  return { profile, reuseEvidence, help, errors };
}

async function statEntry(absolutePath) {
  try {
    return await lstat(absolutePath);
  } catch (error) {
    if (error?.code === "ENOENT") {
      return null;
    }
    throw error;
  }
}

async function capture(command, args, cwd = repoRoot) {
  const stdoutChunks = [];
  const stderrChunks = [];
  const outcome = await new Promise((resolve, reject) => {
    const child = spawn(...commandInvocation(command, args), {
      cwd,
      env: commandEnvironment(),
      stdio: ["ignore", "pipe", "pipe"],
    });
    child.stdout.on("data", (chunk) => stdoutChunks.push(chunk));
    child.stderr.on("data", (chunk) => stderrChunks.push(chunk));
    child.once("error", reject);
    child.once("close", (exitCode) => resolve(exitCode ?? 1));
  });
  if (outcome !== 0) {
    throw new Error(
      `${displayCommand(command, args)} failed: ${Buffer.concat(stderrChunks).toString("utf8").trim()}`,
    );
  }
  return Buffer.concat(stdoutChunks);
}

async function candidatePathsForFingerprint(profile, touchedPaths) {
  if (profile === "changed" || profile === "commit") {
    return touchedPaths;
  }
  const tracked = await capture("git", changedTrackedArgs);
  const untracked = await capture("git", [
    "ls-files",
    "--others",
    "--exclude-standard",
    "-z",
    "--",
  ]);
  return [...parseNullSeparatedPaths(tracked), ...parseNullSeparatedPaths(untracked)];
}

async function fingerprintPaths(paths) {
  const hash = createHash("sha256");
  for (const relativePath of [...new Set(paths)].sort()) {
    const absolutePath = path.join(repoRoot, relativePath);
    const entry = await statEntry(absolutePath);
    hash.update(`${relativePath}\0`);
    if (!entry) {
      hash.update("deleted\0");
      continue;
    }
    hash.update(`${entry.mode}\0`);
    if (entry.isSymbolicLink()) {
      hash.update(`symlink:${await readlink(absolutePath)}\0`);
    } else if (entry.isFile()) {
      hash.update(await readFile(absolutePath));
      hash.update("\0");
    } else {
      hash.update("non-file\0");
    }
  }
  return hash.digest("hex");
}

async function dependencyFingerprint() {
  return fingerprintPaths([
    ".skillshare/config.yaml",
    "development/lint/agent-context.config.json",
    "development/lint/project-skill-ownership.json",
    "development/lint/project-skill-routing-cases.json",
    "development/scripts/agent-check.mjs",
  ]);
}

async function loadReuseEvidence(rawPath) {
  if (!rawPath) {
    return { runId: null, steps: new Map() };
  }
  const absolutePath = path.resolve(process.cwd(), rawPath);
  const parsed = JSON.parse(await readFile(absolutePath, "utf8"));
  if (parsed.schemaVersion !== 2 || !Array.isArray(parsed.steps)) {
    throw new Error("Reuse evidence must be an agent-check schemaVersion 2 result.");
  }
  return {
    runId: parsed.runId ?? null,
    steps: new Map(
      parsed.steps
        .filter((step) => step?.evidenceKey)
        .map((step) => [step.evidenceKey, step]),
    ),
  };
}

async function declaredReadinessSteps() {
  const manifest = JSON.parse(
    await readFile(
      path.join(repoRoot, "development/lint/project-skill-ownership.json"),
      "utf8",
    ),
  );
  const steps = [];
  for (const section of ["skills", "developmentSkills"]) {
    for (const [skillName, contract] of Object.entries(manifest[section] ?? {})) {
      if (!contract.featurePacket?.readiness) {
        continue;
      }
      const catalog = section === "skills" ? ".skillshare/skills" : "development/skills";
      steps.push({
        name: `${skillName} readiness`,
        command: "node",
        args: [path.posix.join(catalog, skillName, contract.featurePacket.readiness)],
      });
    }
  }
  return steps.sort((first, second) => first.name.localeCompare(second.name));
}

async function createLogDirectory() {
  const root = path.join(repoRoot, ".agent-check");
  const rootStat = await statEntry(root);
  if (rootStat && (!rootStat.isDirectory() || rootStat.isSymbolicLink())) {
    throw new Error(".agent-check exists but is not a real directory.");
  }
  if (!rootStat) {
    await mkdir(root);
  }

  const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
  const runId = `${timestamp}-${process.pid}`;
  const runDirectory = path.join(root, runId);
  await mkdir(runDirectory);
  return { runId, runDirectory };
}

function commandEnvironment() {
  return {
    ...process.env,
    CARGO_TERM_COLOR: "never",
    CARGO_INCREMENTAL: "0",
    CARGO_PROFILE_DEV_DEBUG: "0",
    CARGO_PROFILE_TEST_DEBUG: "0",
    NO_COLOR: "1",
    CI: process.env.CI || "1",
  };
}

async function runLoggedStep(context, definition) {
  const index = context.steps.length + 1;
  const logName = `${String(index).padStart(2, "0")}-${safeStepName(definition.name)}.log`;
  const logAbsolute = path.join(context.runDirectory, logName);
  const command = displayCommand(definition.command, definition.args);
  const cwd = definition.cwd ?? repoRoot;
  const evidenceKey = computeEvidenceKey({
    argv: [definition.command, ...definition.args],
    cwd: repoRelative(cwd),
    relevantDiffFingerprint: context.subjectFingerprint,
    dependencyFingerprint: context.dependencyFingerprint,
  });
  const priorStep = context.priorEvidence.steps.get(evidenceKey);
  if (shouldReuseEvidence(priorStep, evidenceKey)) {
    const reusedStep = {
      name: definition.name,
      command,
      argv: [definition.command, ...definition.args],
      cwd: repoRelative(cwd),
      exitCode: 0,
      signal: null,
      durationMs: 0,
      logPath: priorStep.logPath,
      ok: true,
      status: "PASS",
      evidenceKey,
      reusedFromRunId: context.priorEvidence.runId,
    };
    context.steps.push(reusedStep);
    reportProgress({
      index,
      status: "REUSE",
      name: definition.name,
      durationMs: 0,
      logPath: priorStep.logPath,
      reusedFromRunId: context.priorEvidence.runId,
    });
    return reusedStep;
  }
  const relativeLogPath = repoRelative(logAbsolute);
  reportProgress({ index, status: "START", name: definition.name, logPath: relativeLogPath });
  const header = [
    `name: ${definition.name}`,
    `command: ${command}`,
    `cwd: ${cwd}`,
    `startedAt: ${new Date().toISOString()}`,
    "",
  ].join("\n");
  await writeFile(logAbsolute, header, "utf8");
  const logHandle = await open(logAbsolute, "a");
  const started = process.hrtime.bigint();
  let spawnError = null;

  const outcome = await new Promise((resolve) => {
    const child = spawn(...commandInvocation(definition.command, definition.args), {
      cwd,
      env: commandEnvironment(),
      stdio: ["ignore", logHandle.fd, logHandle.fd],
    });
    child.once("error", (error) => {
      spawnError = error;
    });
    child.once("close", (exitCode, signal) => {
      resolve({
        exitCode: exitCode ?? (spawnError ? 127 : 1),
        signal: signal ?? null,
      });
    });
  });

  const durationMs = Number((process.hrtime.bigint() - started) / 1_000_000n);
  if (spawnError) {
    await logHandle.write(`\nspawnError: ${spawnError.message}\n`);
  }
  await logHandle.write(
    `\nfinishedAt: ${new Date().toISOString()}\nexitCode: ${outcome.exitCode}\nsignal: ${outcome.signal ?? ""}\ndurationMs: ${durationMs}\n`,
  );
  await logHandle.close();

  const failure =
    outcome.exitCode === 0
      ? { status: "PASS", code: null }
      : await structuredFailureCode(logAbsolute);

  const step = {
    name: definition.name,
    command,
    argv: [definition.command, ...definition.args],
    cwd: repoRelative(cwd),
    exitCode: outcome.exitCode,
    signal: outcome.signal,
    durationMs,
    logPath: relativeLogPath,
    ok: outcome.exitCode === 0,
    status: failure.status,
    evidenceKey,
    reusedFromRunId: null,
  };
  context.steps.push(step);
  reportProgress({
    index,
    status: failure.status,
    name: definition.name,
    durationMs,
    code: failure.code,
    logPath: relativeLogPath,
  });
  return step;
}

async function runCapturedStep(context, definition) {
  const index = context.steps.length + 1;
  const logName = `${String(index).padStart(2, "0")}-${safeStepName(definition.name)}.log`;
  const logAbsolute = path.join(context.runDirectory, logName);
  const command = displayCommand(definition.command, definition.args);
  const cwd = definition.cwd ?? repoRoot;
  const startedAt = new Date().toISOString();
  const started = process.hrtime.bigint();
  const stdoutChunks = [];
  const stderrChunks = [];
  let spawnError = null;
  const relativeLogPath = repoRelative(logAbsolute);
  reportProgress({ index, status: "START", name: definition.name, logPath: relativeLogPath });

  const outcome = await new Promise((resolve) => {
    const child = spawn(...commandInvocation(definition.command, definition.args), {
      cwd,
      env: commandEnvironment(),
      stdio: ["ignore", "pipe", "pipe"],
    });
    child.stdout.on("data", (chunk) => stdoutChunks.push(chunk));
    child.stderr.on("data", (chunk) => stderrChunks.push(chunk));
    child.once("error", (error) => {
      spawnError = error;
    });
    child.once("close", (exitCode, signal) => {
      resolve({
        exitCode: exitCode ?? (spawnError ? 127 : 1),
        signal: signal ?? null,
      });
    });
  });

  const stdout = Buffer.concat(stdoutChunks);
  const stderr = Buffer.concat(stderrChunks);
  const durationMs = Number((process.hrtime.bigint() - started) / 1_000_000n);
  const log = Buffer.concat([
    Buffer.from(
      `name: ${definition.name}\ncommand: ${command}\ncwd: ${cwd}\nstartedAt: ${startedAt}\n\n[stdout]\n`,
    ),
    stdout,
    Buffer.from("\n[stderr]\n"),
    stderr,
    Buffer.from(
      `${spawnError ? `\nspawnError: ${spawnError.message}\n` : ""}\nexitCode: ${outcome.exitCode}\nsignal: ${outcome.signal ?? ""}\ndurationMs: ${durationMs}\n`,
    ),
  ]);
  await writeFile(logAbsolute, log);

  const step = {
    name: definition.name,
    command,
    argv: [definition.command, ...definition.args],
    cwd: repoRelative(cwd),
    exitCode: outcome.exitCode,
    signal: outcome.signal,
    durationMs,
    logPath: relativeLogPath,
    ok: outcome.exitCode === 0,
  };
  context.steps.push(step);
  reportProgress({
    index,
    status: step.ok ? "PASS" : "FAIL",
    name: definition.name,
    durationMs,
    logPath: relativeLogPath,
  });
  return { step, stdout };
}

function parseNullSeparatedPaths(buffer) {
  return buffer
    .toString("utf8")
    .split("\0")
    .filter(Boolean)
    .map((entry) => entry.split(path.sep).join("/"));
}

async function runCandidateWhitespaceStep(context, touchedPaths) {
  const index = context.steps.length + 1;
  const logName = `${String(index).padStart(2, "0")}-candidate-whitespace.log`;
  const logAbsolute = path.join(context.runDirectory, logName);
  const started = process.hrtime.bigint();
  const violations = [];
  let checkedFiles = 0;
  const relativeLogPath = repoRelative(logAbsolute);
  reportProgress({ index, status: "START", name: "candidate whitespace", logPath: relativeLogPath });

  for (const touchedPath of touchedPaths) {
    const absolutePath = path.resolve(repoRoot, touchedPath);
    if (
      absolutePath !== repoRoot &&
      !absolutePath.startsWith(`${repoRoot}${path.sep}`)
    ) {
      violations.push({ path: touchedPath, line: null, kind: "outside-repository" });
      continue;
    }

    const entryStat = await statEntry(absolutePath);
    if (!entryStat || !entryStat.isFile() || entryStat.isSymbolicLink()) {
      continue;
    }

    const contents = await readFile(absolutePath);
    if (contents.includes(0)) {
      continue;
    }
    checkedFiles += 1;
    const lines = contents.toString("utf8").split("\n");
    for (let lineIndex = 0; lineIndex < lines.length; lineIndex += 1) {
      const line = lines[lineIndex].endsWith("\r")
        ? lines[lineIndex].slice(0, -1)
        : lines[lineIndex];
      if (/[ \t]+$/.test(line)) {
        violations.push({
          path: touchedPath,
          line: lineIndex + 1,
          kind: "trailing-whitespace",
        });
      }
      if (/^[ \t]* \t/.test(line)) {
        violations.push({
          path: touchedPath,
          line: lineIndex + 1,
          kind: "space-before-tab-in-indent",
        });
      }
    }
  }

  const durationMs = Number((process.hrtime.bigint() - started) / 1_000_000n);
  await writeFile(
    logAbsolute,
    `${JSON.stringify({ checkedFiles, violations }, null, 2)}\n`,
    "utf8",
  );
  const step = {
    name: "candidate whitespace",
    command: "internal:candidate-whitespace",
    argv: ["internal:candidate-whitespace"],
    cwd: ".",
    exitCode: violations.length === 0 ? 0 : 1,
    signal: null,
    durationMs,
    logPath: relativeLogPath,
    ok: violations.length === 0,
  };
  context.steps.push(step);
  reportProgress({
    index,
    status: step.ok ? "PASS" : "FAIL",
    name: step.name,
    durationMs,
    code: step.ok ? null : "candidate_whitespace",
    logPath: relativeLogPath,
  });
  return step;
}

async function detectTouchedPaths(context, profile) {
  if (profile === "changed" || profile === "commit") {
    const tracked = await runCapturedStep(context, {
      name:
        profile === "changed"
          ? "detect changed tracked paths"
          : "detect commit candidate tracked paths",
      command: "git",
      args: changedTrackedArgs,
    });
    const untracked = await runCapturedStep(context, {
      name:
        profile === "changed"
          ? "detect untracked paths"
          : "detect commit candidate untracked paths",
      command: "git",
      args: ["ls-files", "--others", "--exclude-standard", "-z", "--"],
    });
    return [...parseNullSeparatedPaths(tracked.stdout), ...parseNullSeparatedPaths(untracked.stdout)];
  }

  return [];
}

export function selectCheckGroups(profile, touchedPaths) {
  if (profile === "skills") {
    return { cargo: false, npm: false, schemaCodegen: false, chatXdk: false, juiceboxSdk: false };
  }
  if (profile === "full") {
    return { cargo: true, npm: true, schemaCodegen: true, chatXdk: true, juiceboxSdk: true };
  }

  let cargo = false;
  let npm = false;
  let schemaCodegen = false;
  let chatXdk = false;
  let juiceboxSdk = false;
  for (const touchedPath of touchedPaths) {
    if (
      touchedPath.startsWith("development/schema-codegen/") &&
      !touchedPath.startsWith("development/schema-codegen/node_modules/")
    ) {
      schemaCodegen = true;
    }
    if (touchedPath.startsWith("libs/chat-xdk/")) {
      chatXdk = true;
      cargo = true;
    }
    if (touchedPath.startsWith("libs/juicebox-sdk/")) {
      juiceboxSdk = true;
      cargo = true;
    }
    if (
      touchedPath === "Cargo.toml" ||
      touchedPath === "Cargo.lock" ||
      touchedPath === "rust-toolchain" ||
      touchedPath === "rust-toolchain.toml" ||
      touchedPath.startsWith(".cargo/") ||
      touchedPath.startsWith("crates/") ||
      (!touchedPath.startsWith("libs/") && touchedPath.endsWith(".rs"))
    ) {
      cargo = true;
    }
    if (
      (touchedPath.startsWith("apps/web/") ||
        touchedPath.startsWith("apps/mcp-ui/") ||
        touchedPath.startsWith("development/schema-codegen/")) &&
      !touchedPath.startsWith("apps/web/node_modules/") &&
      !touchedPath.startsWith("apps/web/dist/") &&
      !touchedPath.startsWith("apps/mcp-ui/node_modules/") &&
      !touchedPath.startsWith("apps/mcp-ui/dist/") &&
      !touchedPath.startsWith("development/schema-codegen/node_modules/")
    ) {
      npm = true;
      schemaCodegen = true;
    }
    // The bundled plugin Router and manifests are validated by the web tests.
    if (touchedPath.startsWith("integrations/codex-marketplace/")) {
      npm = true;
    }
    if (isRustCompileInput(touchedPath)) {
      cargo = true;
    }
  }
  return { cargo, npm, schemaCodegen, chatXdk, juiceboxSdk };
}

// Files outside Rust sources that crates compile in: the staged Codex plugin
// assets (`crates/codex`) and the inputs of the `onecrew-mcp-adapter` Router
// drift test (`operation_registry/tests/production_inventory/router_references.rs`),
// which also lists the MCP App fixture and preview directories.
function isRustCompileInput(touchedPath) {
  return (
    touchedPath.startsWith("integrations/codex-marketplace/plugins/onecrew/") ||
    touchedPath.startsWith("apps/mcp-ui/src/test-support/") ||
    /^apps\/mcp-ui\/preview\/[^/]+\.json$/.test(touchedPath) ||
    touchedPath === "apps/web/src/lib/runtime-change.ts"
  );
}

async function runSelectedChecks(context, profile, selection) {
  const commitLevel = profile === "commit" || profile === "full";

  if (selection.schemaCodegen) {
    await runLoggedStep(context, {
      name: "schema codegen unit tests",
      command: "npm",
      args: ["test"],
      cwd: path.join(repoRoot, "development/schema-codegen"),
    });
  }

  if (selection.cargo) {
    // The CLI embeds both asset trees at compile time, including on a fresh checkout.
    for (const app of ["web", "mcp-ui"]) {
      await runLoggedStep(context, {
        name: `${app} embedded assets`, command: "npm", args: ["run", "build"],
        cwd: path.join(repoRoot, "apps", app),
      });
    }
    await runLoggedStep(context, {
      name: "rust workspace policy",
      command: "uv",
      args: ["run", "development/lint/check_rust_workspace_policy.py"],
    });
    await runLoggedStep(context, {
      name: "cargo format",
      command: "cargo",
      args: ["fmt", "--all", "--", "--check"],
    });
    if (commitLevel) {
      await runLoggedStep(context, {
        name: "cargo clippy",
        command: "cargo",
        args: ["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
      });
      for (const step of rustDocumentationSteps) {
        await runLoggedStep(context, step);
      }
      await runLoggedStep(context, {
        name: "cargo test",
        command: "cargo",
        args: ["test", "--workspace"],
      });
    } else {
      await runLoggedStep(context, {
        name: "cargo check",
        command: "cargo",
        args: ["check", "--workspace", "--all-targets"],
      });
    }
  }

  if (selection.npm) {
    const webDirectory = path.join(repoRoot, "apps/web");
    const mcpUiDirectory = path.join(repoRoot, "apps/mcp-ui");
    for (const script of ["check", "test"]) {
      await runLoggedStep(context, {
        name: `MCP App ${script}`,
        command: "npm",
        args: ["run", script],
        cwd: mcpUiDirectory,
      });
    }
    if (commitLevel && !selection.cargo) {
      await runLoggedStep(context, {
        name: "MCP App build", command: "npm", args: ["run", "build"], cwd: mcpUiDirectory,
      });
    }
    if (commitLevel) {
      await runLoggedStep(context, {
        name: "MCP App contract drift",
        command: "npm",
        args: ["run", "contracts:check"],
        cwd: mcpUiDirectory,
      });
      await runLoggedStep(context, {
        name: "Web contract drift",
        command: "npm",
        args: ["run", "contracts:check"],
        cwd: webDirectory,
      });
    }
    await runLoggedStep(context, {
      name: "npm typecheck",
      command: "npm",
      args: ["run", "typecheck"],
      cwd: webDirectory,
    });
    await runLoggedStep(context, {
      name: "npm test",
      command: "npm",
      args: ["test"],
      cwd: webDirectory,
    });
    if (commitLevel && !selection.cargo) {
      await runLoggedStep(context, {
        name: "npm build",
        command: "npm",
        args: ["run", "build"],
        cwd: webDirectory,
      });
    }
  }

  for (const vendor of [
    {
      selected: selection.chatXdk,
      label: "chat-xdk",
      cwd: path.join(repoRoot, "libs/chat-xdk"),
    },
    {
      selected: selection.juiceboxSdk,
      label: "juicebox-sdk",
      cwd: path.join(repoRoot, "libs/juicebox-sdk"),
    },
  ]) {
    if (!vendor.selected) {
      continue;
    }
    if (vendor.label === "chat-xdk" && commitLevel) {
      await runLoggedStep(context, {
        name: "chat-xdk ci",
        command: "make",
        args: ["ci"],
        cwd: vendor.cwd,
      });
      continue;
    }
    await runLoggedStep(context, {
      name: `${vendor.label} cargo format`,
      command: "cargo",
      args: ["fmt", "--all", "--", "--check"],
      cwd: vendor.cwd,
    });
    if (commitLevel) {
      await runLoggedStep(context, {
        name: `${vendor.label} cargo clippy`,
        command: "cargo",
        args: ["clippy", "--workspace", "--tests", "--", "-D", "warnings"],
        cwd: vendor.cwd,
      });
      await runLoggedStep(context, {
        name: `${vendor.label} cargo test`,
        command: "cargo",
        args: ["test", "--workspace"],
        cwd: vendor.cwd,
      });
    } else {
      await runLoggedStep(context, {
        name: `${vendor.label} cargo check`,
        command: "cargo",
        args: ["check", "--workspace", "--all-targets"],
        cwd: vendor.cwd,
      });
    }
  }
}

function buildSummary(result, started) {
  const passedSteps = result.steps.filter((step) => step.ok).length;
  const failedSteps = result.steps.length - passedSteps;
  result.summary = {
    stepCount: result.steps.length,
    passedSteps,
    failedSteps,
    durationMs: Number((process.hrtime.bigint() - started) / 1_000_000n),
  };
  result.ok = result.errors.length === 0 && failedSteps === 0;
  return result;
}

async function main(profile, reuseEvidencePath) {
  const started = process.hrtime.bigint();
  const { runId, runDirectory } = await createLogDirectory();
  const priorEvidence = await loadReuseEvidence(reuseEvidencePath);
  const context = {
    runDirectory,
    steps: [],
    priorEvidence,
    subjectFingerprint: "pending",
    dependencyFingerprint: "pending",
  };
  const result = {
    schemaVersion: 2,
    tool: "agent-check",
    ok: false,
    profile,
    repoRoot,
    runId,
    logDirectory: repoRelative(runDirectory),
    touchedPaths: [],
    fingerprintedPaths: [],
    fingerprintScope: "conservative-final-candidate",
    subjectFingerprint: null,
    dependencyFingerprint: null,
    reusedEvidenceFromRunId: priorEvidence.runId,
    selection: null,
    steps: context.steps,
    summary: null,
    errors: [],
  };

  result.touchedPaths = [...new Set(await detectTouchedPaths(context, profile))].sort();
  result.fingerprintedPaths = [
    ...new Set(await candidatePathsForFingerprint(profile, result.touchedPaths)),
  ].sort();
  const baseCommit = (await capture("git", ["rev-parse", "HEAD"]))
    .toString("utf8")
    .trim();
  const diffFingerprint = await fingerprintPaths(result.fingerprintedPaths);
  context.subjectFingerprint = createHash("sha256")
    .update(`${baseCommit}\0${diffFingerprint}\0`)
    .digest("hex");
  context.dependencyFingerprint = await dependencyFingerprint();
  result.subjectFingerprint = context.subjectFingerprint;
  result.dependencyFingerprint = context.dependencyFingerprint;
  const skillsTouched =
    profile === "skills" ||
    profile === "full" ||
    result.touchedPaths.some(
      (touchedPath) =>
        touchedPath.startsWith(".skillshare/") ||
        touchedPath.startsWith("development/") ||
        touchedPath === "CLAUDE.md" ||
        touchedPath === "AGENTS.md",
    );
  if (skillsTouched) {
    await runLoggedStep(context, {
      name: "skill contract tests",
      command: "uv",
      args: [
        "run",
        "-m",
        "unittest",
        "discover",
        "-s",
        "development/lint",
        "-p",
        "test_*.py",
      ],
    });
    await runLoggedStep(context, {
      name: "agent check evidence tests",
      command: "node",
      args: ["--test", "development/lint/agent-check.test.mjs"],
    });
    for (const readinessStep of await declaredReadinessSteps()) {
      await runLoggedStep(context, readinessStep);
    }
    await runLoggedStep(context, {
      name: "skillshare audit",
      command: "skillshare",
      args: ["audit", "-p"],
    });
    await runLoggedStep(context, {
      name: "skillshare sync preview",
      command: "skillshare",
      args: ["sync", "-p", "--dry-run"],
    });
    await runLoggedStep(context, {
      name: "agent context lint",
      command: "uv",
      args: ["run", "development/lint/agent-context.py"],
    });
    await runLoggedStep(context, {
      name: "skill token budget",
      command: "uv",
      args: [
        "run",
        "development/skills-analysis/analyze-skills-tokens.py",
        "--sort-by-size",
        "--check",
      ],
    });
  }
  if (profile !== "skills") {
    await runLoggedStep(context, {
      name: "git diff check",
      command: "git",
      args: ["diff", "--check", "HEAD", "--", "."],
    });
    await runCandidateWhitespaceStep(context, result.touchedPaths);
  }
  result.selection = selectCheckGroups(profile, result.touchedPaths);
  await runSelectedChecks(context, profile, result.selection);
  const finalized = buildSummary(result, started);
  await writeFile(
    path.join(runDirectory, "result.json"),
    `${JSON.stringify(finalized, null, 2)}\n`,
    "utf8",
  );
  return finalized;
}

async function runCli(argv) {
  const parsed = parseArguments(argv);
  if (parsed.help) {
    process.stdout.write(
      `${JSON.stringify(
        {
          schemaVersion: 2,
          tool: "agent-check",
          ok: parsed.errors.length === 0,
          usage:
            "node development/scripts/agent-check.mjs --profile skills|changed|commit|full [--reuse-evidence result.json]",
          defaultProfile: "changed",
          errors: parsed.errors,
        },
        null,
        2,
      )}\n`,
    );
    if (parsed.errors.length > 0) {
      process.exitCode = 1;
    }
    return;
  }
  if (parsed.errors.length > 0) {
    process.stdout.write(
      `${JSON.stringify(
        {
          schemaVersion: 2,
          tool: "agent-check",
          ok: false,
          profile: parsed.profile,
          repoRoot,
          errors: parsed.errors.map((message) => ({ code: "INVALID_ARGUMENT", message })),
        },
        null,
        2,
      )}\n`,
    );
    process.exitCode = 1;
    return;
  }
  try {
    const result = await main(parsed.profile, parsed.reuseEvidence);
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
    if (!result.ok) {
      process.exitCode = 1;
    }
  } catch (error) {
    process.stdout.write(
      `${JSON.stringify(
        {
          schemaVersion: 2,
          tool: "agent-check",
          ok: false,
          profile: parsed.profile,
          repoRoot,
          errors: [
            {
              code: "UNEXPECTED_ERROR",
              message: error instanceof Error ? error.message : String(error),
            },
          ],
        },
        null,
        2,
      )}\n`,
    );
    process.exitCode = 1;
  }
}

export {
  computeEvidenceKey,
  formatProgressLine,
  parseArguments,
  parseStructuredFailure,
  rustDocumentationSteps,
  shouldReuseEvidence,
};

const isMain = process.argv[1] && path.resolve(process.argv[1]) === scriptPath;
if (isMain) {
  await runCli(process.argv.slice(2));
}
