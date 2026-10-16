import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";

test("冻结的原Rust实际SQLite/XLSX/JSON产物保持记录的SHA256", () => {
  const root = new URL("./fixtures/", import.meta.url);
  const entries = z.array(z.strictObject({ file: z.string(), sha256: z.string().regex(/^[a-f0-9]{64}$/u) })).parse(parseApplicationJson(readFileSync(new URL("rust-fixture-sha256.json", root))));
  assert.equal(entries.length, 26);
  for (const entry of entries) assert.equal(createHash("sha256").update(readFileSync(new URL(entry.file, root))).digest("hex"), entry.sha256, entry.file);
});
