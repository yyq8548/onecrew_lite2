import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { instagramProfilePlan } from "../src/platforms/opencli/profiles.ts";
import { validateSavedDiscoveryInstagramProfile } from "../src/discovery/contact-profile-contracts.ts";
import { instagramProfileIdentityPlan } from "../src/platforms/opencli/profile-identity.ts";
import { decodeCreatorSearch } from "../src/platforms/opencli/search-results.ts";

for (const [username, id] of [["tssitlidze_sofia", "65478081998"], ["eliirax", "58840919784"]]) {
  test(`Instagram真实网页资料 ${username} 在读取与保存校验之间保留帖子预览`, () => {
    const raw = JSON.parse(readFileSync(new URL(`./fixtures/instagram-visible-profile/${username}.json`, import.meta.url), "utf8"));
    const decoded = instagramProfilePlan(username!, id!).decode(raw);
    assert.ok(decoded.visible_posts?.length);
    const saved = validateSavedDiscoveryInstagramProfile(id!, decoded);
    assert.deepEqual(saved.visible_posts, decoded.visible_posts);
    assert.deepEqual(saved.biography, decoded.biography);
  });
  test(`Instagram OpenCLI DOM ${username} 保留主页身份、资料和已观察帖子来源`, () => {
    const raw = JSON.parse(readFileSync(new URL(`./fixtures/instagram-visible-profile/${username}.json`, import.meta.url), "utf8"));
    const pageId = raw[0].profile_url as string;
    const material = raw.map((row: Record<string, unknown>) => ({ ...row, id: pageId, identity_kind: "profile_url", evidence_source: "public_profile_dom_profile_bound", execution_mode: "opencli_dom" }));
    const decoded = instagramProfileIdentityPlan(username!).decode(material);
    const saved = validateSavedDiscoveryInstagramProfile(pageId, decoded);
    assert.equal(saved.follower_count, raw[0].follower_count);
    assert.equal(saved.biography, raw[0].biography);
    assert.deepEqual(saved.visible_posts, raw[0].visible_posts);
    assert.equal(saved.execution_mode, "opencli_dom");
    const search = { provider: "onecrew-instagram-visible-v1", execution_mode: "opencli_dom", username: username!, name: raw[0].display_name, url: pageId };
    assert.equal(decodeCreatorSearch("instagram", [search], 1)[0]!.author_id, null);
    assert.throws(() => validateSavedDiscoveryInstagramProfile(pageId, { ...saved, identity_kind: "native_id" }));
    assert.throws(() => validateSavedDiscoveryInstagramProfile(pageId, { ...saved, execution_mode: "api" }));
  });
}
