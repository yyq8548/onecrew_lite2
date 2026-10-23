import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import JSZip from "jszip";

// 固定历史数据库只用于验证升级合同，不执行旧版本程序。
export async function extractCreatorHistory(versions: readonly number[] = Array.from({ length: 64 }, (_, index) => index + 27)): Promise<string> {
  const bytes = await readFile(new URL("./fixtures/creator-history/schemas.zip", import.meta.url));
  assert.equal(createHash("sha256").update(bytes).digest("hex"), "35030a84c52262ee7c48be13f11ef158e51fe61ed038548562a6cf4def69159b");
  const archive = await JSZip.loadAsync(bytes, { checkCRC32: true });
  const names = Array.from({ length: 64 }, (_, index) => `creator-schema${index + 27}-oracle.sqlite`);
  assert.deepEqual(Object.keys(archive.files).sort(), [...names].sort());
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/creator-history-", import.meta.url)));
  for (const version of versions) {
    assert.ok(Number.isInteger(version) && version >= 27 && version <= 90);
    const name = `creator-schema${version}-oracle.sqlite`;
    await writeFile(join(directory, name), await archive.file(name)!.async("nodebuffer"), { flag: "wx", mode: 0o600 });
  }
  return directory;
}
