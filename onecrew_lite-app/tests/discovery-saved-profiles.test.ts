import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { validateDiscoveryRetainedProfile } from "../src/discovery/native-material.ts";

test("11个原Rust FB v1/v2与Reddit保存profile合同结果逐项一致", () => {
  const cases = z.array(z.object({ author: z.string(), platform: z.enum(["facebook", "reddit"]), profile: z.unknown(), source: z.string(), valid: z.boolean() })).parse(parseApplicationJson(readFileSync(new URL("./fixtures/discovery-saved-profiles-rust.json", import.meta.url))));
  assert.equal(cases.length, 11);
  for (const value of cases) {
    const url = value.platform === "facebook" ? `https://www.facebook.com/profile.php?id=${value.author}` : value.source;
    const validate = () => validateDiscoveryRetainedProfile(value.platform, value.author, url, value.source, value.profile);
    if (value.valid) assert.deepEqual(validate(), value.profile); else assert.throws(validate);
  }
});
