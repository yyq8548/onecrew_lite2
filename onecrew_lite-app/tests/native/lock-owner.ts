import assert from "node:assert/strict";
import { once } from "node:events";
import { open } from "node:fs/promises";
import { createRequire } from "node:module";

const native = createRequire(import.meta.url)("fs-native-extensions") as { tryLock(fd: number): boolean };
assert.equal(process.argv.length, 3);
const file = await open(process.argv[2]!, "r+");
try {
  assert.equal(native.tryLock(file.fd), true);
  process.stdout.write("locked\n");
  await once(process.stdin, "data");
} finally { await file.close(); }
