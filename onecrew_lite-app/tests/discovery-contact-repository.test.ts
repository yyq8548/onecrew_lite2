import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../src/contracts/application-json.ts";
import { DiscoveryReviewRepository } from "../src/discovery/review-repository.ts";
import { DiscoveryRepository } from "../src/discovery/repository.ts";
import { loadDiscoveryReviewBasis } from "../src/discovery/review-basis.ts";
import { buildDiscoveryReviewMaterial } from "../src/discovery/review-material.ts";
import { discoveryProfileContactFields } from "../src/discovery/contact-profile.ts";
import { loadDiscoveryContactObservations } from "../src/discovery/contact-repository.ts";
import { discoveryEligibleContactAuthor, loadDiscoveryNativeContactMaterial, nextDiscoveryNativeContactLink } from "../src/discovery/contact-native.ts";
import { discoveryReviewRequestHash, submitDiscoveryReviewInput } from "../src/discovery/review-submission.ts";
import { upgradeCreatorStagingCopy } from "../src/storage/creator-upgrade.ts";
import { backupLegacyDatabase } from "../src/storage/legacy-sqlite.ts";

const root = fileURLToPath(new URL("./fixtures/discovery-contacts/", import.meta.url));
async function database(name: string) {
  mkdirSync(fileURLToPath(new URL("../../.scratch/", import.meta.url)), { recursive: true });
  const target = join(mkdtempSync(fileURLToPath(new URL("../../.scratch/discovery-contacts-", import.meta.url))), "staging", "creator.sqlite");
  const sourcePath = join(root, `${name}-before.sqlite`);
  await backupLegacyDatabase(sourcePath, target, "creator");
  await upgradeCreatorStagingCopy({ sourcePath, stagingPath: target });
  const db = new DatabaseSync(target); db.exec("PRAGMA foreign_keys=ON"); return db;
}
const contacts = (db: DatabaseSync) => db.prepare("SELECT * FROM discovery_contact_observations ORDER BY normalized_value,source_url").all();
const originalContacts = (db: DatabaseSync) => contacts(db).filter(row => row.kind !== "website").map(({ source_excerpt, ...row }) => row);
function currentInput(db: DatabaseSync, raw: unknown) {
  const input = submitDiscoveryReviewInput.parse(raw);
  db.exec("BEGIN DEFERRED");
  try {
    const material = buildDiscoveryReviewMaterial(loadDiscoveryReviewBasis(db, input.campaign_id, input.expected_revision, input.search_hit_id));
    return { ...input, material_hash: material.material_hash };
  } finally { db.exec("COMMIT"); }
}
test("原Rust native与DOM真实SQLite升级后保存联系人及来源", async () => {
  for (const name of ["native", "web"]) {
    const source = z.object({ input: z.unknown(), material: z.object({ material_hash: z.string() }) }).parse(parseApplicationJson(readFileSync(join(root, `${name}.json`)))), db = await database(name), original = new DatabaseSync(join(root, `${name}-after.sqlite`), { readOnly: true });
    try {
      const input = currentInput(db, source.input), repository = new DiscoveryReviewRepository(db), receipt = repository.submit("contact-oracle", input);
      assert.equal(receipt.verdict, "qualified");
      assert.deepEqual(originalContacts(db), originalContacts(original));
      assert.ok(contacts(db).some(contact => contact.kind === "website" && contact.original_value === "https://creator.example/contact"));
      const savedContacts = contacts(db);
      assert.ok(contacts(db).some(contact => contact.normalized_value === "public@example.com"));
      assert.ok(contacts(db).some(contact => contact.normalized_value === "keyboard@example.com"));
      assert.ok(contacts(db).some(contact => contact.kind === "social_profile"));
      const ledger = db.prepare("SELECT request_hash FROM discovery_evidence_reviews WHERE request_id=?").get(receipt.request_id);
      assert.equal(ledger!.request_hash, discoveryReviewRequestHash("contact-oracle", input));
      assert.deepEqual(repository.submit("contact-oracle", input), receipt);
      assert.deepEqual(contacts(db), savedContacts);
      db.exec("BEGIN DEFERRED");
      assert.equal(discoveryEligibleContactAuthor(db, receipt.campaign_id, receipt.criteria_revision, receipt.candidate_id), receipt.native_author_id);
      assert.ok(loadDiscoveryNativeContactMaterial(db, { campaign_id: receipt.campaign_id, expected_revision: receipt.criteria_revision, candidate_id: receipt.candidate_id, profile_ref: receipt.profile_ref, requested_url: "https://creator.example/contact" }));
      assert.ok(nextDiscoveryNativeContactLink(db, receipt.campaign_id, []));
      assert.equal(loadDiscoveryContactObservations(db, receipt.campaign_id, receipt.criteria_revision, receipt.candidate_id, "another-author").length, 0);
      assert.equal(loadDiscoveryContactObservations(db, receipt.campaign_id, receipt.criteria_revision, receipt.candidate_id, receipt.native_author_id).length, savedContacts.length);
      db.exec("COMMIT");
      db.prepare("UPDATE discovery_campaigns SET target_count=1 WHERE id=?").run(receipt.campaign_id);
      const discovery = new DiscoveryRepository(db), before = discovery.getPlan(receipt.campaign_id).used_calls;
      assert.equal(discovery.planStep(receipt.campaign_id, receipt.criteria_revision).kind, "read_public_page");
      assert.equal(discovery.getPlan(receipt.campaign_id).used_calls, before);
    } finally { original.close(); db.close(); }
  }
});
test("真实联系人写失败共同回滚审核ledger和candidate，旧profile归属变化明确失败", async () => {
  const source = z.object({ input: z.unknown() }).parse(parseApplicationJson(readFileSync(join(root, "native.json")))), db = await database("native");
  try {
    const repository = new DiscoveryReviewRepository(db), before = db.prepare("SELECT id,verdict,evidence_json,updated_at FROM discovery_candidates ORDER BY id").all();
    db.exec("BEGIN DEFERRED");
    for (const row of before) assert.equal(discoveryEligibleContactAuthor(db, "instagram-flow", 1, z.string().parse(row.id)), null);
    db.exec("COMMIT");
    db.exec("CREATE TRIGGER reject_contact_write BEFORE INSERT ON discovery_contact_observations BEGIN SELECT RAISE(FAIL,'contact_write_rejected'); END");
    const reviewedInput = currentInput(db, source.input);
    assert.throws(() => repository.submit("contact-oracle", reviewedInput), /contact_write_rejected/u);
    assert.deepEqual(db.prepare("SELECT id,verdict,evidence_json,updated_at FROM discovery_candidates ORDER BY id").all(), before);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM discovery_evidence_reviews").get()!.n, 0);
    assert.equal(contacts(db).length, 0); assert.equal(db.isTransaction, false);
    db.exec("DROP TRIGGER reject_contact_write");
    const input = z.object({ search_hit_id: z.string() }).parse(source.input);
    db.exec("BEGIN IMMEDIATE");
    const material = buildDiscoveryReviewMaterial(loadDiscoveryReviewBasis(db, "instagram-flow", 1, input.search_hit_id));
    assert.ok(discoveryProfileContactFields(db, material).includes("public@example.com"));
    db.prepare("UPDATE discovery_profile_reads SET expected_revision=2 WHERE search_hit_id=?").run(input.search_hit_id);
    assert.throws(() => discoveryProfileContactFields(db, material), /contact_native_profile_binding_unavailable/u);
    db.exec("ROLLBACK");
    assert.equal(stringifyApplicationJson(db.prepare("SELECT id,verdict,evidence_json,updated_at FROM discovery_candidates ORDER BY id").all()), stringifyApplicationJson(before));
  } finally { db.close(); }
});
