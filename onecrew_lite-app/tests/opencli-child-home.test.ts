import assert from "node:assert/strict";
import test from "node:test";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { runTrustedOpenCliProcess } from "../src/platforms/opencli/process.ts";

test("actual child runtime uses the account home without changing the parent environment", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const home = await mkdtemp(join(scratch, "opencli-child-home-"));
  const configurationDirectory = join(home, ".opencli");
  await mkdir(configurationDirectory, { mode: 0o700 });
  const before = { HOME: process.env.HOME, USERPROFILE: process.env.USERPROFILE };
  try {
    const result = await runTrustedOpenCliProcess({ executable: process.execPath }, ["--input-type=module", "--eval", "import {homedir} from 'node:os'; console.log(JSON.stringify({home:homedir(),config:process.env.OPENCLI_CONFIG_DIR}))"], { deadlineMs: 5000, configurationDirectory });
    assert.deepEqual(JSON.parse(result.stdout), { home, config: configurationDirectory });
    assert.deepEqual({ HOME: process.env.HOME, USERPROFILE: process.env.USERPROFILE }, before);
  } finally { await rm(home, { recursive: true }); }
});
