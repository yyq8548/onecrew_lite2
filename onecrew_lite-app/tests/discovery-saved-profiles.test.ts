import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { validateDiscoveryRetainedProfile } from "../src/discovery/native-material.ts";

test("7个冻结FB v1/v2样本保留身份约束", () => {
  const cases = z.array(z.object({ author: z.string(), platform: z.string(), profile: z.unknown(), source: z.string(), valid: z.boolean() })).parse(parseApplicationJson(readFileSync(new URL("./fixtures/discovery-saved-profiles-rust.json", import.meta.url)))).filter(value => value.platform === "facebook");
  assert.equal(cases.length, 7);
  for (const value of cases) {
    const url = `https://www.facebook.com/profile.php?id=${value.author}`;
    const validate = () => validateDiscoveryRetainedProfile(value.platform, value.author, url, value.source, value.profile);
    if (value.valid) assert.deepEqual(validate(), value.profile);
    else assert.throws(validate);
  }
});
