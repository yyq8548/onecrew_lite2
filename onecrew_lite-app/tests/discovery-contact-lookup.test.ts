import assert from "node:assert/strict";
import { copyFile, mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { DiscoveryContactLookupRepository, discoveryContactLookup } from "../src/discovery/public-page-lookup.ts";
import { DiscoveryPublicPageRepository, discoveryPublicPageClaim } from "../src/discovery/public-page-repository.ts";
import { discoveryPublicPageRead } from "../src/discovery/review-material.ts";
const source = new URL("./fixtures/discovery-contact-lookup/", import.meta.url);
const oracle = z.object({ planned: discoveryContactLookup, claim: discoveryContactLookup, completed: discoveryContactLookup, page_claim: discoveryPublicPageClaim, page: discoveryPublicPageRead }).parse(parseApplicationJson(await readFile(new URL("oracle.json", source))));
async function fixture(name = "before") {
  const scratch = fileURLToPath(new URL("../../.scratch/discovery-contact-lookup-tests/", import.meta.url)); await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "actual-")), path = join(root, "crm.sqlite"); await copyFile(new URL(`${name}.sqlite`, source), path);
  const database = new DatabaseSync(path), repository = new DiscoveryContactLookupRepository(database), pages = new DiscoveryPublicPageRepository(database);
  return { database, repository, pages, async close() { database.close(); await rm(root, { recursive: true }); } };
}
test("original scored lookup plan, atomic charge, completion and author-bound contact page match Rust", async () => {
  const value = await fixture();
  try {
    assert.deepEqual(value.repository.next("bili-shoes"), oracle.planned);
    assert.equal(value.repository.next("bili-shoes", ["bilibili"]), null);
    assert.deepEqual(value.repository.claim(oracle.planned), oracle.claim);
    assert.equal(value.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 4);
    assert.throws(() => value.repository.claim(oracle.planned), /eligible/);
    assert.deepEqual(value.repository.finish(oracle.completed), oracle.completed);
    assert.deepEqual(value.repository.finish(oracle.completed), oracle.completed);
    assert.deepEqual(value.pages.next("bili-shoes"), oracle.page_claim.input);
    assert.deepEqual(value.pages.claim(oracle.page_claim.input), oracle.page_claim);
    const page = value.pages.finish({ hidden_email_available: false, claim: oracle.page_claim, document_url: oracle.page.document_url, title: oracle.page.title, markdown_excerpt: oracle.page.markdown_excerpt, truncated: oracle.page.truncated, stop_code: null });
    const { captured_at: originalTime, ...expected } = oracle.page, { captured_at: actualTime, ...actual } = page;
    assert.deepEqual(actual, expected);
    const golden = new DatabaseSync(fileURLToPath(new URL("contacted.sqlite", source)), { readOnly: true });
    try {
      const sql = "SELECT candidate_id,discovery_campaign_id,criteria_revision,native_author_id,kind,original_value,normalized_value,source_url,source_excerpt,attribution FROM discovery_contact_observations ORDER BY kind,normalized_value";
      assert.deepEqual(value.database.prepare(sql).all(), golden.prepare(sql).all());
      assert.ok(value.database.prepare(sql).all().some(row => row.normalized_value === "runner@example.com"));
    } finally { golden.close(); }
    assert.equal(value.repository.next("bili-shoes"), null);
  } finally { await value.close(); }
});
test("unscored or changed lookup inputs cannot spend and invalid completion is durably failed", async () => {
  const unscored = await fixture("unscored");
  try {
    assert.equal(unscored.repository.next("bili-shoes"), null);
    assert.throws(() => unscored.repository.claim(oracle.planned), /eligible/);
    assert.equal(unscored.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 3);
  } finally { await unscored.close(); }
  const value = await fixture();
  try {
    assert.throws(() => value.repository.claim({ ...oracle.planned, query: "untrusted email guess" }), /plan_changed/);
    assert.equal(value.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 3);
    value.repository.claim(oracle.planned);
    const failed = value.repository.settle({ ...oracle.completed, links: ["http://127.0.0.1/"] });
    assert.equal(failed.status, "failed"); assert.equal(failed.stop_code, "opencli_invalid_output");
    assert.deepEqual(failed.links, []);
    assert.equal(value.database.prepare("SELECT status FROM discovery_platform_runs").get()!.status, "completed");
    assert.equal(value.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 4);
    assert.equal(value.repository.next("bili-shoes"), null);
  } finally { await value.close(); }
});
test("paused and stale material retain charged lookup receipt and never invent contacts", async () => {
  const value = await fixture("claimed");
  try {
    value.database.prepare("UPDATE discovery_campaigns SET phase='paused'").run();
    const result = value.repository.finish(oracle.completed);
    assert.equal(result.status, "failed"); assert.equal(result.stop_code, "contact_lookup_stale"); assert.deepEqual(result.links, []);
    assert.equal(value.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 4);
    assert.equal(value.database.prepare("SELECT COUNT(*) AS n FROM discovery_contact_observations").get()!.n, 0);
    assert.equal(value.repository.next("bili-shoes"), null);
  } finally { await value.close(); }
});
