#!/usr/bin/env node

// Complete, reproducible transport inventory for a pinned OpenCLI manifest.
// A registered read is not proof of a connected profile or live site access.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

const manifestPath = process.argv[2] && resolve(process.argv[2]);
if (!manifestPath) {
  process.stderr.write("Usage: node audit-opencli-read-inventory.mjs /path/to/OpenCLI/cli-manifest.json\n");
  process.exit(2);
}

const root = dirname(manifestPath);
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
if (!Array.isArray(manifest)) throw new Error("OpenCLI manifest must be an array");
const version = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8")).version;
const commit = execFileSync("git", ["-C", root, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
const reads = manifest.filter(command => command.access === "read");
const allSites = new Set(manifest.map(command => command.site));
const sites = new Map();
const transportCounts = new Map();

for (const command of reads) {
  const transport = `${command.browser ? "Chrome" : "direct"} ${command.strategy}`;
  transportCounts.set(transport, (transportCounts.get(transport) ?? 0) + 1);
  if (!sites.has(command.site)) sites.set(command.site, []);
  sites.get(command.site).push({ name: command.name, transport });
}

const escapeCell = value => String(value).replaceAll("|", "\\|").replaceAll("\n", " ");
process.stdout.write(`# OpenCLI 所有站点只读连接清单\n\n`);
process.stdout.write(`来源：官方 OpenCLI ${version}，提交 \`${commit}\` 的 \`cli-manifest.json\`。`);
process.stdout.write(`共 ${manifest.length} 个命令、${allSites.size} 个站点；其中 ${reads.length} 个只读命令覆盖 ${sites.size} 个站点。`);
process.stdout.write(`逐站列出每个只读命令及注册连接方式；没有只读命令的站点单独标明。\n\n`);
process.stdout.write(`全量连接方式：${[...transportCounts].sort().map(([transport, count]) => `${transport} ${count}`).join("；")}。`);
process.stdout.write(`\`Chrome public\` 仍需浏览器扩展；\`direct local\` 可能依赖本机账号或客户端。`);
process.stdout.write(`注册清单不证明登录、现场读取成功、公开可访问或符合 Creator Campaign。\n\n`);
process.stdout.write(`| 站点 | 只读命令数 | 连接方式（命令数） | 命令（连接方式） |\n`);
process.stdout.write(`| --- | ---: | --- | --- |\n`);
for (const site of [...allSites].sort((a, b) => a.localeCompare(b, "en"))) {
  const commands = sites.get(site) ?? [];
  if (!commands.length) {
    const onlyWrites = manifest.filter(command => command.site === site)
      .map(command => `\`${command.name}\` (${command.browser ? "Chrome" : "direct"} ${command.strategy}, ${command.access})`).join("、");
    process.stdout.write(`| ${[site, 0, "无只读命令", onlyWrites].map(escapeCell).join(" | ")} |\n`);
    continue;
  }
  const counts = new Map();
  for (const command of commands) counts.set(command.transport, (counts.get(command.transport) ?? 0) + 1);
  const methods = [...counts].sort().map(([transport, count]) => `${transport} ${count}`).join("；");
  const names = commands.sort((a, b) => a.name.localeCompare(b.name, "en"))
    .map(command => `\`${command.name}\` (${command.transport})`).join("、");
  process.stdout.write(`| ${[site, commands.length, methods, names].map(escapeCell).join(" | ")} |\n`);
}
