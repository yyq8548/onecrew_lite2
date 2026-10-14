import { execFileSync } from "node:child_process";
import { access } from "node:fs/promises";
import { resolve } from "node:path";

import { repositoryRoot } from "./wrangler-target.mjs";
import "./site-deployment-config.mjs";

const args = process.argv.slice(2).filter((argument) => argument !== "--");

if (args.includes("--help") || args.includes("-h")) {
  console.log("Usage: pnpm run deploy:site -- [--dry-run]");
  process.exit(0);
}

if (args.some((argument) => argument !== "--dry-run") || args.length > 1) {
  console.error("Only --dry-run is supported for the pinned OneCrew Site deployment.");
  process.exit(2);
}

const dryRun = args[0] === "--dry-run";
const pnpm = process.platform === "win32" ? "pnpm.cmd" : "pnpm";

execFileSync(pnpm, ["run", "check:domains"], {
  cwd: repositoryRoot,
  stdio: "inherit",
});
execFileSync(pnpm, ["run", "build:site"], {
  cwd: repositoryRoot,
  stdio: "inherit",
});
execFileSync(process.execPath, ["development/check-site-csp.mjs"], {
  cwd: repositoryRoot,
  stdio: "inherit",
});

for (const file of ["ga4-init.js", "index.html", "zh/index.html", "robots.txt", "sitemap.xml", "llms.txt"]) {
  try {
    await access(resolve(repositoryRoot, "site/build/client", file));
  } catch {
    console.error(`Site artifact is missing site/build/client/${file}; deployment stopped.`);
    process.exit(2);
  }
}

const wranglerArgs = [
  "exec",
  "wrangler",
  "deploy",
  "--config",
  "site/wrangler.toml",
  "--profile",
  "onecrew",
];
if (dryRun) wranglerArgs.push("--dry-run", "--outdir", "dist/site-worker");

execFileSync(pnpm, wranglerArgs, {
  cwd: repositoryRoot,
  stdio: "inherit",
});
