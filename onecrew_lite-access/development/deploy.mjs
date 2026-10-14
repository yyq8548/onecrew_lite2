import { execFileSync } from "node:child_process";
import { access } from "node:fs/promises";
import { resolve } from "node:path";

import {
  assertSafeDeployArgs,
  parseWranglerTargetArgs,
  repositoryRoot,
  validateAccessWranglerConfig,
  WranglerTargetError,
  wranglerTargetArgs,
} from "./wrangler-target.mjs";

const rawArgs = process.argv.slice(2);
if (rawArgs.includes("--help") || rawArgs.includes("-h")) {
  console.log(
    "Usage: pnpm run deploy -- [--config <path>] [--profile <name>] [safe Wrangler deploy options]",
  );
  process.exit(0);
}

const secretBindingNames = [
  "THIRD_PARTY_TOKEN_ENCRYPTION_KEY",
  "BETTER_AUTH_SECRET",
  "PAIRING_HMAC_SECRET",
  "RESEND_API_KEY",
  "GOOGLE_CLIENT_SECRET",
  "RELEASE_LINK_SECRET",
  "RELEASE_PUBLISHER_TOKEN",
];

try {
  const target = parseWranglerTargetArgs(rawArgs, { allowPassthrough: true });
  assertSafeDeployArgs(target.passthrough);
  if (
    target.passthrough.some((argument) =>
      secretBindingNames.some((name) => argument.includes(name))
    )
  ) {
    throw new WranglerTargetError(
      "Do not pass secret bindings to deploy; use `wrangler secret put <NAME>` with the same config and profile.",
    );
  }
  await validateAccessWranglerConfig(target.configPath);

  const pnpm = process.platform === "win32" ? "pnpm.cmd" : "pnpm";
  execFileSync(pnpm, ["run", "check:migrations"], {
    cwd: repositoryRoot,
    stdio: "inherit",
  });
  execFileSync(pnpm, ["run", "build:access"], {
    cwd: repositoryRoot,
    stdio: "inherit",
  });
  try {
    await access(resolve(repositoryRoot, "web/dist/access/index.html"));
  } catch {
    throw new WranglerTargetError(
      "The Access build is missing web/dist/access/index.html; deployment stopped before Wrangler.",
    );
  }
  execFileSync(
    pnpm,
    [
      "exec",
      "wrangler",
      "deploy",
      ...wranglerTargetArgs(target),
      ...target.passthrough,
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
