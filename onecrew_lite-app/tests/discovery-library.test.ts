import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { buildDiscoveryLibrarySnapshot } from "../src/discovery/library-material.ts";

test("原Rust真实仓储Library材料的未验证、证据和旧确认投影及全部hash逐项相同", () => {
  const cases = z.array(z.object({ basis: z.unknown(), expected: z.unknown() })).parse(parseApplicationJson(readFileSync(new URL("./fixtures/discovery-library-rust.json", import.meta.url))));
  assert.equal(cases.length, 3);
  for (const value of cases) assert.deepEqual(buildDiscoveryLibrarySnapshot(value.basis), value.expected);
});
