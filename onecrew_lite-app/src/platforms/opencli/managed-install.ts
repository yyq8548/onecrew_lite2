import { createHash } from "node:crypto";
import { lstat, mkdir, mkdtemp, open, rename, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { homedir } from "node:os";
import { dirname, isAbsolute, join } from "node:path";
import { extractNodeZip } from "./node-archive.ts";
import { runTrustedOpenCliProcess } from "./process.ts";
import { OpenCliRegistry, parseOpenCliJson, parseOpenCliVersion } from "./registry.ts";
import { VERIFIED_OPENCLI_VERSION } from "./installation.ts";

export const MANAGED_NODE_VERSION = "24.21.0";
const nativeLock = createRequire(import.meta.url)("fs-native-extensions") as { tryLock(fd: number): boolean };
export const NODE_DISTRIBUTIONS = Object.freeze({
  "win-x64": Object.freeze({ suffix: "zip", sha256: "158f7685b44de51f6c0df1d153526cbcd3e1bc739a8dfc607721cef75de9e541" }),
  "darwin-x64": Object.freeze({ suffix: "tar.gz", sha256: "1462cb3b3046b815cf8ea436d3da450ec1a9f11dac7e5a46b0ada5305d7e8097" }),
  "darwin-arm64": Object.freeze({ suffix: "tar.gz", sha256: "bed7eea5325e1108f32ce5228ddd6a5f0f08a499ee42aa7442aea583702f6057" }),
});
type Distribution = keyof typeof NODE_DISTRIBUTIONS;
function platform(): Distribution { const name = `${process.platform === "win32" ? "win" : process.platform}-${process.arch}`; if (!Object.hasOwn(NODE_DISTRIBUTIONS, name)) throw new Error("opencli_repair_unsupported_platform"); return name as Distribution; }
function root() { const value = process.platform === "win32" ? process.env.LOCALAPPDATA ? join(process.env.LOCALAPPDATA, "onecrew_lite", "opencli") : "" : join(homedir(), ".local", "share", "onecrew_lite", "opencli"); if (!isAbsolute(value)) throw new Error("opencli_repair_home_unavailable"); return value; }
async function safeDirectory(path: string) {
  const chain = []; for (let current = path; ; current = dirname(current)) { chain.push(current); if (dirname(current) === current) break; }
  for (const current of chain.reverse()) {
    const info = await lstat(current).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return null; throw error; });
    if (!info) await mkdir(current, { mode: 0o700 });
    else if (!info.isDirectory() || info.isSymbolicLink()) throw new Error("opencli_repair_unsafe_directory");
  }
}
function entrypoint(path: string, distribution: Distribution) { const node = join(path, `node-v${MANAGED_NODE_VERSION}-${distribution}`); return { executable: distribution.startsWith("win-") ? join(node, "node.exe") : join(node, "bin", "node"), script: join(path, "cli", "node_modules", "@jackwener", "opencli", "dist", "src", "main.js") }; }
async function validate(path: string, distribution: Distribution) {
  const installation = entrypoint(path, distribution);
  for (const file of [installation.executable, installation.script]) { const info = await lstat(file); if (!info.isFile() || info.isSymbolicLink()) throw new Error("opencli_repair_readback_failed"); }
  const nodeVersion = await runTrustedOpenCliProcess({ executable: installation.executable }, ["--version"], { deadlineMs: 15000 });
  if (nodeVersion.stdout.trim() !== `v${MANAGED_NODE_VERSION}`) throw new Error("opencli_repair_version_mismatch");
  const version = await runTrustedOpenCliProcess(installation, ["--version"], { deadlineMs: 15000 });
  if (parseOpenCliVersion(version.stdout) !== VERIFIED_OPENCLI_VERSION) throw new Error("opencli_repair_version_mismatch");
  const registry = await runTrustedOpenCliProcess(installation, ["list", "-f", "json"], { deadlineMs: 15000 });
  new OpenCliRegistry(await parseOpenCliJson(registry.stdout));
}
export function verifyNodeArchiveChecksum(bytes: Uint8Array, distribution: Distribution): void {
  if (bytes.byteLength > 96 * 1024 * 1024 || createHash("sha256").update(bytes).digest("hex") !== NODE_DISTRIBUTIONS[distribution].sha256) throw new Error("opencli_repair_checksum_failed");
}
async function download(distribution: Distribution, destination: string) {
  const spec = NODE_DISTRIBUTIONS[distribution];
  const response = await fetch(`https://nodejs.org/dist/v${MANAGED_NODE_VERSION}/node-v${MANAGED_NODE_VERSION}-${distribution}.${spec.suffix}`, { redirect: "error", signal: AbortSignal.timeout(180000) });
  if (!response.ok || !response.body) throw new Error("opencli_repair_download_failed");
  const chunks: Uint8Array[] = []; let total = 0;
  const reader = response.body.getReader();
  try {
    while (true) { const next = await reader.read(); if (next.done) break; total += next.value.byteLength; if (total > 96 * 1024 * 1024) throw new Error("opencli_repair_download_too_large"); chunks.push(next.value); }
  } finally { await reader.cancel(); reader.releaseLock(); }
  const bytes = Buffer.concat(chunks); verifyNodeArchiveChecksum(bytes, distribution);
  const file = await open(destination, "wx", 0o600); try { await file.writeFile(bytes); await file.sync(); } finally { await file.close(); }
}
// 仅显式安装流程调用；固定根、版本、下载源与摘要，不接受 HTTP 请求中的路径或包名。
export async function repairManagedOpenCli(approved: boolean): Promise<void> {
  if (approved !== true) throw new Error("opencli_repair_approval_required");
  const distribution = platform(); const directory = root(); await safeDirectory(directory);
  const lockPath = join(directory, "install.lock");
  const prior = await lstat(lockPath).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return null; throw error; });
  if (prior && (!prior.isFile() || prior.isSymbolicLink())) throw new Error("opencli_repair_write_failed");
  const lease = await open(lockPath, "a+", 0o600);
  try {
    if (!nativeLock.tryLock(lease.fd)) throw new Error("opencli_repair_busy");
    const actual = await lstat(lockPath); const opened = await lease.stat();
    if (actual.isSymbolicLink() || actual.dev !== opened.dev || actual.ino !== opened.ino) throw new Error("opencli_repair_write_failed");
    const installed = join(directory, "managed-v1");
    const valid = await validate(installed, distribution).then(() => true, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT" || error instanceof Error && /opencli_repair_|opencli_/u.test(error.message)) return false; throw error; });
    if (valid) return;
    const staging = await mkdtemp(join(directory, "staging-")); const archive = join(staging, `node.${NODE_DISTRIBUTIONS[distribution].suffix}`);
    await download(distribution, archive);
    if (distribution === "win-x64") await extractNodeZip(archive, staging, `node-v${MANAGED_NODE_VERSION}-${distribution}`);
    else await runTrustedOpenCliProcess({ executable: "/usr/bin/tar" }, ["-xzf", archive, "-C", staging], { deadlineMs: 60000, installation: true });
    const node = entrypoint(staging, distribution).executable;
    const npm = distribution === "win-x64" ? join(dirname(node), "node_modules", "npm", "bin", "npm-cli.js") : join(dirname(node), "..", "lib", "node_modules", "npm", "bin", "npm-cli.js");
    const config = join(staging, "empty.npmrc"); const global = join(staging, "global.npmrc");
    await writeFile(config, "", { flag: "wx", mode: 0o600 }); await writeFile(global, "", { flag: "wx", mode: 0o600 });
    await runTrustedOpenCliProcess({ executable: node, script: npm }, ["install", "--prefix", join(staging, "cli"), "--cache", join(staging, "npm-cache"), "--userconfig", config, "--globalconfig", global, "--registry=https://registry.npmjs.org", "--ignore-scripts", "--no-audit", "--no-fund", "--save-exact", `@jackwener/opencli@${VERIFIED_OPENCLI_VERSION}`], { deadlineMs: 240000, installation: true });
    await validate(staging, distribution);
    const backup = join(directory, "previous-v1");
    const retained = await lstat(backup).then(() => true, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return false; throw error; });
    if (retained) throw new Error("opencli_repair_backup_retained");
    const existing = await lstat(installed).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return null; throw error; });
    if (existing && (!existing.isDirectory() || existing.isSymbolicLink())) throw new Error("opencli_repair_unsafe_directory");
    if (existing) await rename(installed, backup);
    const publication = await rename(staging, installed).then(() => ({ ok: true as const }), error => ({ ok: false as const, error }));
    if (!publication.ok) { if (existing) await rename(backup, installed); throw publication.error; }
    const checked = await validate(installed, distribution).then(() => ({ ok: true as const }), error => ({ ok: false as const, error }));
    if (!checked.ok) { await rename(installed, staging); if (existing) await rename(backup, installed); throw checked.error; }
    // 保留原安装与暂存诊断资料；独立清理流程只处理经过归属核对的安装资源。
  } finally { await lease.close(); }
}
