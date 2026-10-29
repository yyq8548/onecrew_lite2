import assert from "node:assert/strict";
import { cp, mkdir, mkdtemp, readFile, rename, rm, symlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { test, type TestContext } from "node:test";
import { fileURLToPath } from "node:url";
import { OPENCLI_ADDON_FILES, OpenCliAddons, type OpenCliAddonId } from "../src/platforms/opencli/addons.ts";

const addon = "onecrew-xiaohongshu";
const bundled = fileURLToPath(new URL("../../onecrew_lite-opencli/", import.meta.url));

async function fixture(context: TestContext, ids: readonly OpenCliAddonId[] = [addon]) {
  const scratch = fileURLToPath(new URL("../../.scratch/opencli-addon-snapshot/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "case-"));
  context.after(() => rm(root, { recursive: true, force: true }));
  const source = join(root, "bundled"); const home = join(root, "home");
  for (const id of ids) {
    const distribution = join(source, id); const installed = join(home, ".opencli", "clis", id);
    await Promise.all([mkdir(distribution, { recursive: true }), mkdir(installed, { recursive: true })]);
    for (const name of OPENCLI_ADDON_FILES[id]) {
      await Promise.all([cp(join(bundled, id, name), join(distribution, name)), cp(join(bundled, id, name), join(installed, name))]);
    }
  }
  const directory = join(home, ".opencli", "clis", addon);
  return { root, source, home, directory, file: join(directory, "read.mjs"), sourceFile: join(source, addon, "read.mjs"), addons: new OpenCliAddons(source, home) };
}

test("命令闭包复用42个真实分发文件快照，每次准备仍读取分发源，安装内容变化仍拒绝", async context => {
  const ids = [addon, "onecrew-browser-search"] as const;
  assert.equal(ids.reduce((count, id) => count + OPENCLI_ADDON_FILES[id].length, 0), 42);
  const files = await fixture(context, ids);
  const verify = await files.addons.prepare(ids);
  await verify();
  await rm(files.source, { recursive: true });
  await verify();
  await assert.rejects(files.addons.prepare(ids), { code: "ENOENT" });
  const bytes = await readFile(files.file); bytes[0] = bytes[0]! ^ 1;
  await writeFile(files.file, bytes);
  await assert.rejects(verify(), { code: "opencli_read_command_unavailable" });
});

test("首次准备验证分发文件类型和安装字节", async context => {
  const cases: { name: string; code: string; change: (files: Awaited<ReturnType<typeof fixture>>) => Promise<void> }[] = [
    { name: "分发源符号链接", code: "opencli_invalid_output", change: async files => {
      await rm(files.sourceFile); await symlink(join(bundled, addon, "read.mjs"), files.sourceFile);
    } },
    { name: "分发源目录", code: "opencli_invalid_output", change: async files => {
      await rm(files.sourceFile); await mkdir(files.sourceFile);
    } },
    { name: "安装副本等长内容变化", code: "opencli_read_command_unavailable", change: async files => {
      const bytes = await readFile(files.file); bytes[0] = bytes[0]! ^ 1; await writeFile(files.file, bytes);
    } },
  ];
  for (const scenario of cases) await context.test(scenario.name, async child => {
    const files = await fixture(child); await scenario.change(files);
    await assert.rejects(files.addons.prepare([addon]), { code: scenario.code });
  });
});

test("后置回调重新检查安装目录、祖先和全部文件约束", async context => {
  const cases: { name: string; change: (files: Awaited<ReturnType<typeof fixture>>) => Promise<void> }[] = [
    { name: "额外文件", change: files => cp(files.file, join(files.directory, "extra.mjs")) },
    { name: "缺少文件", change: files => rm(files.file) },
    { name: "文件大小变化", change: async files => {
      const bytes = await readFile(files.file); await writeFile(files.file, bytes.subarray(1));
    } },
    { name: "文件类型变化", change: async files => { await rm(files.file); await mkdir(files.file); } },
    { name: "文件符号链接", change: async files => {
      await rm(files.file); await symlink(files.sourceFile, files.file);
    } },
    { name: "安装目录符号链接", change: async files => {
      const moved = join(files.root, "moved-addon");
      await rename(files.directory, moved); await symlink(moved, files.directory, "dir");
    } },
    { name: "安装祖先符号链接", change: async files => {
      const config = join(files.home, ".opencli"); const moved = join(files.root, "moved-config");
      await rename(config, moved); await symlink(moved, config, "dir");
    } },
  ];
  for (const scenario of cases) await context.test(scenario.name, async child => {
    const files = await fixture(child); const verify = await files.addons.prepare([addon]);
    await scenario.change(files);
    await assert.rejects(verify(), { code: "opencli_read_command_unavailable" });
  });
});
