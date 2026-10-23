import assert from "node:assert/strict";
import { stat } from "node:fs/promises";
import { isAbsolute } from "node:path";
import test from "node:test";
import { getDesktopDirectory } from "../src/native-capabilities/desktop-directory.ts";

test("native operating system resolves the existing Desktop without creating or exporting files", { skip: process.platform !== "win32" && process.platform !== "darwin" }, async () => {
  const path = await getDesktopDirectory();
  assert.ok(isAbsolute(path));
  assert.ok((await stat(path)).isDirectory());
});
