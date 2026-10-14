import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

import { ACCESS_ORIGIN, BLOG_ORIGIN, SITE_ORIGIN } from "./site-deployment-config.mjs";

const [action, target = "access", ...extraArgs] = process.argv.slice(2);
const devArgs = extraArgs.filter((arg) => arg !== "--");
const root = fileURLToPath(new URL("../", import.meta.url));
const pnpm = process.platform === "win32" ? "pnpm.cmd" : "pnpm";
const environment = {
  ...process.env,
  NODE_OPTIONS: `${process.env.NODE_OPTIONS ?? ""} --dns-result-order=ipv4first`.trim(),
  VITE_ACCESS_URL: ACCESS_ORIGIN,
  VITE_BLOG_URL: BLOG_ORIGIN,
  VITE_SITE_URL: SITE_ORIGIN,
};

function runVite(command, buildTarget) {
  if (buildTarget === "site") {
    const args = ["exec", "react-router", command, "web", "--config", "web/vite.site.config.ts"];
    if (command === "dev") args.push(...devArgs);
    execFileSync(pnpm, args, { cwd: root, env: environment, stdio: "inherit" });
    if (command === "build") {
      execFileSync(process.execPath, ["development/generate-site-static.mjs"], {
        cwd: root,
        env: environment,
        stdio: "inherit",
      });
      execFileSync(process.execPath, ["development/generate-site-csp.mjs"], {
        cwd: root,
        env: environment,
        stdio: "inherit",
      });
      execFileSync(process.execPath, ["development/check-site-seo.mjs"], {
        cwd: root,
        env: environment,
        stdio: "inherit",
      });
    }
    return;
  }
  const config = `web/vite.${buildTarget}.config.ts`;
  const args = ["exec", "vite", ...(command === "build" ? ["build"] : []), "--config", config];
  if (command === "dev") args.push(...devArgs);
  execFileSync(pnpm, args, { cwd: root, env: environment, stdio: "inherit" });
}

if (action === "dev" && (target === "access" || target === "site")) {
  runVite("dev", target);
} else if (action === "build" && target === "all") {
  runVite("build", "access");
  runVite("build", "site");
} else if (action === "build" && (target === "access" || target === "site")) {
  runVite("build", target);
} else if (action === "test") {
  execFileSync(pnpm, ["exec", "vitest", "run", "--config", "web/vitest.config.ts"], {
    cwd: root,
    env: environment,
    stdio: "inherit",
  });
} else {
  console.error("Usage: node development/web.mjs dev <access|site> | build <access|site|all> | test");
  process.exitCode = 2;
}
