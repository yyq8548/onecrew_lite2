import { execFileSync } from "node:child_process";

import {
  parseWranglerTargetArgs,
  repositoryRoot,
  validateAccessWranglerConfig,
  WranglerTargetError,
  wranglerTargetArgs,
} from "./wrangler-target.mjs";

const rawArgs = process.argv.slice(2);
if (rawArgs.includes("--help") || rawArgs.includes("-h")) {
  console.log(
    "Usage: pnpm run db:migrate:<local|remote> -- [--config <path>] [--profile <name>]",
  );
  process.exit(0);
}

const modes = rawArgs.filter(
  (argument) => argument === "--local" || argument === "--remote",
);
if (modes.length !== 1) {
  console.error("Choose exactly one migration target: --local or --remote.");
  process.exit(2);
}
const mode = modes[0];

try {
  const target = parseWranglerTargetArgs(
    rawArgs.filter((argument) => argument !== mode),
  );
  await validateAccessWranglerConfig(target.configPath);
  const pnpm = process.platform === "win32" ? "pnpm.cmd" : "pnpm";
  execFileSync(pnpm, ["run", "check:migrations"], {
    cwd: repositoryRoot,
    stdio: "inherit",
  });
  execFileSync(
    pnpm,
    [
      "exec",
      "wrangler",
      "d1",
      "migrations",
      "apply",
      "DB",
      mode,
      ...wranglerTargetArgs(target),
    ],
    { cwd: repositoryRoot, stdio: "inherit" },
  );
} catch (error) {
  if (error instanceof WranglerTargetError) {
    console.error(error.message);
    process.exit(2);
  }
  throw error;
}
