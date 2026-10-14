import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

import {
  parseWranglerTargetArgs,
  repositoryRoot,
  validateAccessWranglerConfig,
  WranglerTargetError,
  wranglerTargetArgs,
} from "./wrangler-target.mjs";

const rawArgs = process.argv.slice(2);
if (rawArgs.includes("--help") || rawArgs.includes("-h")) {
  console.log("Usage: pnpm run types -- [--config <path>] [--profile <name>]");
  process.exit(0);
}

try {
  const target = parseWranglerTargetArgs(rawArgs);
  await validateAccessWranglerConfig(target.configPath);
  const pnpm = process.platform === "win32" ? "pnpm.cmd" : "pnpm";
  execFileSync(
    pnpm,
    [
      "exec",
      "wrangler",
      "types",
      "worker-configuration.d.ts",
      "--env-interface",
      "CloudflareBindings",
      "--strict-vars=false",
      ...wranglerTargetArgs(target),
    ],
    { cwd: repositoryRoot, stdio: "inherit" },
  );
  const generatedTypes = new URL("../worker-configuration.d.ts", import.meta.url);
  writeFileSync(
    generatedTypes,
    readFileSync(generatedTypes, "utf8").replace(/[ \t]+$/gm, ""),
  );
} catch (error) {
  if (error instanceof WranglerTargetError) {
    console.error(error.message);
    process.exit(2);
  }
  throw error;
}
