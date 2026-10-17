import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { instagramProfilePlan } from "../src/platforms/opencli/profiles.ts";
import { validateSavedDiscoveryInstagramProfile } from "../src/discovery/contact-profile-contracts.ts";

for (const [username, id] of [["tssitlidze_sofia", "65478081998"], ["eliirax", "58840919784"]]) {
  test(`Instagram真实网页资料 ${username} 在读取与保存校验之间保留帖子预览`, () => {
    const raw = JSON.parse(readFileSync(new URL(`./fixtures/instagram-visible-profile/${username}.json`, import.meta.url), "utf8"));
    const decoded = instagramProfilePlan(username!, id!).decode(raw);
    assert.ok(decoded.visible_posts?.length);
    const saved = validateSavedDiscoveryInstagramProfile(id!, decoded);
    assert.deepEqual(saved.visible_posts, decoded.visible_posts);
    assert.deepEqual(saved.biography, decoded.biography);
  });
}
