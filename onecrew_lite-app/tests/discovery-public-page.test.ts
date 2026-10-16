import assert from "node:assert/strict";
import { copyFile, mkdir, mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { DiscoveryPublicPageRepository, discoveryPublicPageClaim } from "../src/discovery/public-page-repository.ts";
import { discoveryPublicPageInput } from "../src/discovery/contact-native.ts";
import { discoveryPublicPageRead } from "../src/discovery/review-material.ts";
import { getDiscoverySavedPublicPage } from "../src/discovery/saved-read-repository.ts";
import { FileDiscoveryContactAudit } from "../src/discovery/public-page-audit.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

const directory = new URL("./fixtures/discovery-public-page/", import.meta.url);
const oracle = z.object({ input: discoveryPublicPageInput, planned: discoveryPublicPageInput, claim: discoveryPublicPageClaim, completed: discoveryPublicPageRead, old_claim: discoveryPublicPageClaim, resumed: discoveryPublicPageClaim, failed: discoveryPublicPageRead }).parse(parseApplicationJson(await readFile(new URL("oracle.json", directory))));
async function fixture(name = "before") {
  const scratch = fileURLToPath(new URL("../../.scratch/discovery-public-page-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true }); const root = await mkdtemp(join(scratch, "actual-")), path = join(root, "crm.sqlite");
  await copyFile(new URL(`${name}.sqlite`, directory), path);
  const database = new DatabaseSync(path), repository = new DiscoveryPublicPageRepository(database);
  return { database, repository, async close() { database.close(); await rm(root, { recursive: true }); } };
}
function withoutTime<T extends { captured_at: number | bigint }>(value: T) { const { captured_at, ...rest } = value; return rest; }
function completed(claim = oracle.claim) { return { hidden_email_available: false, claim, document_url: "https://example.com/tool", title: "Tool", markdown_excerpt: "An observed AI tool page.", truncated: false, stop_code: null }; }

test("original Rust SQLite public page plan, charged claim, full receipt and reopen match", async () => {
  const value = await fixture();
  try {
    assert.deepEqual(value.repository.next("pages"), oracle.planned);
    assert.equal(value.repository.next("pages", ["youtube"]), null);
    assert.equal(value.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 2);
    assert.deepEqual(value.repository.claim(oracle.input), oracle.claim);
    assert.equal(value.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 3);
    assert.throws(() => value.repository.claim(oracle.input), /active/);
    const result = value.repository.finish(completed());
    assert.deepEqual(withoutTime(result), withoutTime(oracle.completed));
    assert.deepEqual(getDiscoverySavedPublicPage(value.database, "pages", oracle.input.candidate_id, oracle.input.requested_url), result);
    assert.equal(value.database.prepare("SELECT verdict FROM discovery_candidates").get()!.verdict, "pending");
    assert.equal(value.repository.next("pages"), null);
    assert.throws(() => value.repository.claim(oracle.input), /attempted/);
  } finally { await value.close(); }
});

test("public page provenance and permanent author binding reject before spending or roll back completion", async () => {
  for (const corruption of ["DELETE FROM discovery_video_identity_bindings", "UPDATE discovery_video_identity_bindings SET native_author_id='UCotherchannel'", "UPDATE discovery_candidates SET platform_account_id='UCchangedcandidate'"]) {
    const value = await fixture();
    try {
      value.database.exec(corruption);
      assert.equal(value.repository.next("pages"), null);
      assert.throws(() => value.repository.claim(oracle.input));
      assert.equal(value.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 2);
      assert.equal(value.database.prepare("SELECT COUNT(*) AS n FROM discovery_public_page_reads").get()!.n, 0);
    } finally { await value.close(); }
    const claimed = await fixture("claimed");
    try {
      claimed.database.exec(corruption);
      assert.throws(() => claimed.repository.finish(completed()));
      assert.equal(claimed.database.prepare("SELECT status FROM discovery_public_page_reads").get()!.status, "reading");
      assert.equal(claimed.database.prepare("SELECT status FROM discovery_platform_runs").get()!.status, "searching");
    } finally { await claimed.close(); }
  }
  const value = await fixture();
  try {
    for (const requested_url of ["https://example.com/tool-extra", "https://unobserved.example.com/", "https://discord.gg/2abyjxr4v6"]) assert.throws(() => value.repository.claim({ ...oracle.input, requested_url }));
    assert.equal(value.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 2);
  } finally { await value.close(); }
});

test("original interrupted page recharges once and failed receipt prevents replay", async () => {
  const value = await fixture("interrupted");
  try {
    assert.deepEqual(value.repository.next("pages"), oracle.resumed.input);
    assert.deepEqual(value.repository.claim(oracle.resumed.input), oracle.resumed);
    assert.throws(() => value.repository.finish(completed(oracle.old_claim)), /stale/);
    const result = value.repository.finish({ hidden_email_available: false, claim: oracle.resumed, document_url: null, title: "", markdown_excerpt: "", truncated: false, stop_code: "opencli_risk_control" });
    assert.deepEqual(withoutTime(result), withoutTime(oracle.failed));
    assert.equal(value.database.prepare("SELECT status FROM discovery_platform_runs").get()!.status, "risk_control_stop");
    assert.equal(value.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 4);
    assert.throws(() => value.repository.claim(oracle.resumed.input));
  } finally { await value.close(); }
});

test("invalid provider evidence settles a durable failed receipt without releasing another claim", async () => {
  const value = await fixture();
  try {
    const claim = value.repository.claim(oracle.input);
    const result = value.repository.settle({ ...completed(claim), markdown_excerpt: "" });
    assert.equal(result.status, "failed"); assert.equal(result.stop_code, "opencli_invalid_output");
    assert.equal(value.database.prepare("SELECT status FROM discovery_platform_runs").get()!.status, "completed");
    assert.equal(value.database.prepare("SELECT used_calls FROM discovery_campaigns").get()!.used_calls, 3);
    assert.throws(() => value.repository.claim(oracle.input));
  } finally { await value.close(); }
});

test("actual append-only contact metrics hash campaign paths and reject page or contact text", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/discovery-public-page-tests/", import.meta.url)); await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "metrics-")), directory = await createPrivateDirectory(join(root, "private"));
  let owned = true;
  const audit = new FileDiscoveryContactAudit(directory, () => { if (!owned) throw new Error("account ownership changed"); });
  const event = { kind: "lookup" as const, candidate_id: "actual-candidate", call_number: 1, elapsed_ms: 12, method: "search" as const, raw_results: 5, kept_links: 1, provider_status: "completed" as const, stop_code: null };
  try {
    await Promise.all([audit.record("../../campaign", event), audit.record("../../campaign", { ...event, call_number: 2 })]);
    const files = await readdir(join(directory, "contact-enrichment-metrics")); assert.equal(files.length, 1); assert.match(files[0]!, /^[a-f0-9]{64}\.jsonl$/u);
    const rows = (await readFile(join(directory, "contact-enrichment-metrics", files[0]!), "utf8")).trim().split("\n").map(line => z.object({ call_number: z.number(), recorded_at: z.number().positive() }).parse(parseApplicationJson(line)));
    assert.deepEqual(rows.map(row => row.call_number), [1, 2]);
    assert.throws(() => audit.record("campaign", { ...event, page_text: "private page" } as typeof event));
    owned = false;
    await assert.rejects(audit.record("campaign", event), /ownership changed/);
  } finally { await rm(root, { recursive: true }); }
});
