import assert from "node:assert/strict";
import { cp, mkdir, mkdtemp, readdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { OpenCliAddons } from "../src/platforms/opencli/addons.ts";
import { discoveryExecutionFailureCode } from "../src/discovery/execution-failure.ts";

test("原bundle真实安装到隔离home，冲突必须显式修复且未知文件完整保留", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/opencli-addon-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true }); const home = await mkdtemp(join(scratch, "home-"));
  const source = fileURLToPath(new URL("../../onecrew_lite-opencli/", import.meta.url));
  const addons = new OpenCliAddons(source, home);
  await assert.rejects(addons.requireReady("onecrew-youtube"), error => {
    assert.equal(discoveryExecutionFailureCode(error), "opencli_command_unavailable");
    return true;
  });
  await assert.rejects(addons.install(false), { code: "opencli_invalid_input" });
  assert.equal((await addons.install(true)).every(item => item.state === "ready"), true);
  const unknown = join(home, ".opencli", "clis", "onecrew-youtube", "custom.js");
  await writeFile(unknown, "keep original custom file", { flag: "wx" });
  await assert.rejects(addons.install(true), { code: "opencli_read_command_unavailable" });
  assert.equal(await readFile(unknown, "utf8"), "keep original custom file");
  assert.equal((await addons.repair(true)).every(item => item.state === "ready"), true);
  const backups = join(home, ".opencli", "onecrew-backups");
  const roots = await readdir(backups); let preserved = false;
  for (const root of roots) {
    const directory = join(backups, root);
    if ((await readdir(directory)).includes("onecrew-youtube")) preserved = await readFile(join(directory, "onecrew-youtube", "custom.js"), "utf8") === "keep original custom file";
  }
  assert.equal(preserved, true);
  assert.equal((await addons.status()).every(item => item.state === "ready"), true);
  const retired = join(home, ".opencli", "clis", "onecrew-zhihu");
  await mkdir(retired);
  const retainedEvidence = fileURLToPath(new URL("./fixtures/zhihu-profile-contact-live-20261029.json", import.meta.url));
  await cp(retainedEvidence, join(retired, "saved-evidence.json"));
  assert.equal((await addons.repair(true)).every(item => item.state === "ready"), true);
  assert.equal((await readdir(join(home, ".opencli", "clis"))).includes("onecrew-zhihu"), false);
  const archived = await readdir(backups); let evidencePreserved = false;
  for (const root of archived) {
    const directory = join(backups, root, "onecrew-zhihu");
    if ((await readdir(join(backups, root))).includes("onecrew-zhihu"))
      evidencePreserved = (await readFile(join(directory, "saved-evidence.json"))).equals(await readFile(retainedEvidence));
  }
  assert.equal(evidencePreserved, true);
});
