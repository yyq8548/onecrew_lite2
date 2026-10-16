import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { codexVersionDirectory, resolveCodexCli } from "../src/discovery/codex-installation.ts";

test("原Codex版本目录只接受16至64位十六进制，显式配置错误不转找其他程序或shell", async () => {
  assert.equal(codexVersionDirectory("9614f1d9b40d9c4f"), true); assert.equal(codexVersionDirectory("a".repeat(64)), true);
  for (const value of ["unrelated", "a".repeat(15), "a".repeat(65), "../9614f1d9b40d9c4f", "g".repeat(16)]) assert.equal(codexVersionDirectory(value), false);
  await assert.rejects(resolveCodexCli("codex"), /semantic_unavailable/);
  await assert.rejects(resolveCodexCli(fileURLToPath(new URL("../../.scratch/never-a-codex-cli.cmd", import.meta.url))), /semantic_unavailable/);
  await assert.rejects(resolveCodexCli(fileURLToPath(new URL("../../.scratch/nonexistent-codex-cli.exe", import.meta.url))), /semantic_unavailable/);
});
