import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { facebookProfilePlan } from "../src/platforms/opencli/facebook-profile.ts";
import { validateDiscoveryRetainedProfile } from "../src/discovery/native-material.ts";

test("真实Facebook主页没有联系入口时仍可通过保存前的身份校验", () => {
  const rows = JSON.parse(readFileSync(new URL("./fixtures/facebook-native-empty-contacts/profile.json", import.meta.url), "utf8"));
  const actual = rows[0];
  const decoded = facebookProfilePlan(actual.id, actual.observed_profile_url).decode(rows);
  assert.deepEqual(decoded.contact_links, []);
  assert.doesNotThrow(() => validateDiscoveryRetainedProfile("facebook", actual.id, actual.profile_url, actual.observed_profile_url, decoded));
});
