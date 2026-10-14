import { execFileSync } from "node:child_process";
import { relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const allowedFiles = new Set([
  "wrangler.toml",
  "site/wrangler.toml",
  "site/contact.json",
]);

let output = "";
try {
  output = execFileSync(
    "rg",
    [
      "-n",
      "--glob", "!**/dist/**",
      "--glob", "!site/build/**",
      "--glob", "!web/.react-router/**",
      "--glob", "!**/node_modules/**",
      "(?:https://)?(?:[A-Za-z0-9-]+\\.)*onecrewai\\.io",
      "README.md", "development", "package.json", "site", "src", "test", "web", "worker-configuration.d.ts", "wrangler.toml",
    ],
    { cwd: root, encoding: "utf8" },
  );
} catch (error) {
  if (error.status !== 1) throw error;
}

const violations = output.trim().split("\n").filter(Boolean).filter((line) => {
  const file = line.split(":", 1)[0];
  if (allowedFiles.has(file)) return false;
  if (file.startsWith("test/")) return false;
  return !/\.test\.[cm]?[jt]sx?:/.test(line);
});

if (violations.length > 0) {
  console.error("Production domain literals escaped the approved config/test boundary:");
  for (const violation of violations) console.error(violation);
  process.exitCode = 1;
} else {
  console.log(`Domain boundary clean under ${relative(process.cwd(), root) || "."}`);
}
