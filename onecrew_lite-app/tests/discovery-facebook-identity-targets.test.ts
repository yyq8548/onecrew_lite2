import assert from "node:assert/strict";
import { copyFile, mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { DiscoveryFacebookIdentityRepository, discoveryFacebookIdentityClaim } from "../src/discovery/facebook-identity.ts";
import { DiscoveryFacebookTargetsRepository } from "../src/discovery/facebook-targets.ts";
import { discoveryProfileClaim } from "../src/discovery/profile-repository.ts";

const source = new URL("./fixtures/discovery-facebook-identity-targets/", import.meta.url);
const identityRead = z.object({ claim: discoveryFacebookIdentityClaim, status: z.string(), native_identity: z.record(z.string(), z.unknown()).nullable(), stop_code: z.string().nullable(), captured_at: z.number() });
const targetRead = z.object({ claim: discoveryProfileClaim, status: z.string(), source_urls: z.array(z.string()).nullable(), stop_code: z.string().nullable(), captured_at: z.number() });
const oracle = z.object({ campaign: z.string(), hit: z.string(), identity_claim: discoveryFacebookIdentityClaim, identity: identityRead, targets_claim: discoveryProfileClaim, targets: targetRead }).parse(parseApplicationJson(await readFile(new URL("oracle.json", source))));
async function fixture(name = "before") {
  const scratch = fileURLToPath(new URL("../../.scratch/discovery-facebook-identity-targets-tests/", import.meta.url)); await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "actual-")), path = join(root, "crm.sqlite"); await copyFile(new URL(`${name}.sqlite`, source), path);
  const database = new DatabaseSync(path), identities = new DiscoveryFacebookIdentityRepository(database), targets = new DiscoveryFacebookTargetsRepository(database);
  return { database, identities, targets, async close() { database.close(); await rm(root, { recursive: true }); } };
}
function withoutTime<T extends { captured_at: number | bigint }>(value: T) { const { captured_at, ...rest } = value; return rest; }

test("original Facebook vanity identity charges once and binds native author without replacing observed source", async () => {
  const value = await fixture();
  try {
    assert.equal(value.identities.next(oracle.campaign), oracle.hit);
    assert.equal(value.identities.next(oracle.campaign, ["facebook"]), null);
    assert.throws(() => value.identities.claim(oracle.campaign, 2, oracle.hit));
    assert.equal(value.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 1);
    const claim = value.identities.claim(oracle.campaign, 1, oracle.hit);
    assert.deepEqual(claim, oracle.identity_claim);
    assert.equal(value.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 2);
    assert.throws(() => value.identities.claim(oracle.campaign, 1, oracle.hit));
    const read = value.identities.finish(claim, oracle.identity.native_identity, null);
    assert.deepEqual(withoutTime(read), withoutTime(oracle.identity));
    assert.deepEqual(value.identities.finish(claim, oracle.identity.native_identity, null), read);
    assert.equal(value.database.prepare("SELECT source_url FROM discovery_search_hits WHERE id=?").get(oracle.hit)!.source_url, claim.source_url);
    assert.equal(value.database.prepare("SELECT author_url FROM discovery_search_authors WHERE search_hit_id=?").get(oracle.hit)!.author_url, "https://www.facebook.com/profile.php?id=100044572203205");
    assert.equal(value.database.prepare("SELECT COUNT(*) AS n FROM discovery_candidates").get()!.n, 0);
    assert.throws(() => value.identities.finish(claim, { ...oracle.identity.native_identity, display_name: "Changed identity" }, null), /finished differently/);
  } finally { await value.close(); }
});
test("identity rejects source drift and private fields before binding, then settles invalid output", async () => {
  const value = await fixture("identity-claimed");
  try {
    for (const change of [{ source_url: "https://www.facebook.com/Other" }, { id: "NativeCreator" }, { profile_url: "https://www.facebook.com/Other" }, { cookie: "forbidden" }]) {
      assert.throws(() => value.identities.finish(oracle.identity_claim, { ...oracle.identity.native_identity, ...change }, null));
      assert.equal(value.identities.get(oracle.campaign, oracle.hit).status, "reading");
    }
    const read = value.identities.settle(oracle.identity_claim, { ...oracle.identity.native_identity, cookie: "forbidden" }, null);
    assert.equal(read.status, "failed"); assert.equal(read.stop_code, "opencli_invalid_output");
    assert.equal(value.database.prepare("SELECT COUNT(*) AS n FROM discovery_search_authors").get()!.n, 0);
    assert.notEqual(value.identities.next(oracle.campaign), oracle.hit);
    assert.equal(value.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 2);
  } finally { await value.close(); }
});
test("identity risk and auth stop platform while interruption releases only its exact paid claim", async () => {
  for (const [code, status] of [["opencli_risk_control", "risk_control_stop"], ["opencli_auth_required", "unauthorized"]]) {
    const value = await fixture("identity-claimed");
    try { value.identities.finish(oracle.identity_claim, null, code!); assert.equal(value.database.prepare("SELECT status FROM discovery_platform_runs").get()!.status, status); assert.equal(value.identities.next(oracle.campaign), null); }
    finally { await value.close(); }
  }
  const value = await fixture("identity-claimed");
  try {
    assert.equal(value.identities.interrupt(oracle.identity_claim).stop_code, "interrupted_facebook_identity_read");
    assert.equal(value.database.prepare("SELECT status FROM discovery_platform_runs").get()!.status, "completed");
    assert.notEqual(value.identities.next(oracle.campaign), oracle.hit);
    assert.throws(() => value.identities.claim(oracle.campaign, 1, oracle.hit));
    assert.equal(value.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 2);
  } finally { await value.close(); }
});
test("original Facebook profile target reservation preserves exact source and immutable bounded locators", async () => {
  const value = await fixture("profile-completed");
  try {
    assert.equal(value.targets.next(oracle.campaign), oracle.hit);
    const claim = value.targets.claim(oracle.campaign, 1, oracle.hit);
    assert.deepEqual(claim, oracle.targets_claim);
    assert.equal(value.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 4);
    for (const urls of [[claim.source_url], [oracle.targets.source_urls![0]!, oracle.targets.source_urls![0]!], [oracle.targets.source_urls![0]! + "?comment_id=6"], []]) assert.throws(() => value.targets.finish(claim, urls, null));
    const receipt = value.targets.finish(claim, oracle.targets.source_urls, null);
    assert.deepEqual(withoutTime(receipt), withoutTime(oracle.targets));
    assert.deepEqual(value.targets.finish(claim, oracle.targets.source_urls, null), receipt);
    assert.equal(value.targets.next(oracle.campaign), null);
    assert.throws(() => value.targets.claim(oracle.campaign, 1, oracle.hit));
    value.database.prepare("UPDATE discovery_campaigns SET phase='paused'").run();
    assert.throws(() => value.targets.finish(claim, oracle.targets.source_urls, null), /authorization changed/);
  } finally { await value.close(); }
});
test("Facebook targets bind the complete saved source identity and cancellation never reissues", async () => {
  const value = await fixture("profile-completed");
  try {
    value.database.prepare("DELETE FROM discovery_facebook_identity_reads WHERE search_hit_id=?").run(oracle.hit);
    assert.equal(value.targets.next(oracle.campaign), null);
    assert.throws(() => value.targets.claim(oracle.campaign, 1, oracle.hit));
    assert.equal(value.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 3);
  } finally { await value.close(); }
  const claimed = await fixture("targets-claimed");
  try {
    claimed.targets.interrupt(oracle.targets_claim);
    const read = claimed.targets.get(oracle.campaign, oracle.hit);
    assert.equal(read.status, "failed"); assert.equal(read.stop_code, "interrupted_read");
    assert.equal(claimed.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 4);
    assert.equal(claimed.targets.next(oracle.campaign), null);
    assert.throws(() => claimed.targets.finish(oracle.targets_claim, oracle.targets.source_urls, null));
  } finally { await claimed.close(); }
});
