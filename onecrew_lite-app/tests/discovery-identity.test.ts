import assert from "node:assert/strict";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { z } from "zod";
import { parseApplicationJson, stringifyApplicationJson } from "../src/contracts/application-json.ts";
import { identityPairInput, identitySubmitInput, identityRevokeInput, identityReviewReceipt } from "../src/discovery/identity-contracts.ts";
import { DiscoveryIdentityRepository } from "../src/discovery/identity-repository.ts";
import { identitySubmissionHash } from "../src/discovery/identity-review.ts";
import { validateIdentityAddition } from "../src/discovery/identity-relationships.ts";
import { DiscoveryIdentityPublicationRepository, buildIdentityLibraryMaterial } from "../src/discovery/identity-library-publication.ts";

const directory = process.env.ONECREW_IDENTITY_ORACLE_DIRECTORY ?? fileURLToPath(new URL("./fixtures/discovery-identity/", import.meta.url));
const scratch = fileURLToPath(new URL("../../.scratch/", import.meta.url));
async function database(name = "baseline.sqlite") {
  mkdirSync(scratch, { recursive: true });
  assert.ok(directory, "Run the original Rust identity oracle before this integration test");
  const destination = join(mkdtempSync(join(scratch, "discovery-identity-")), "business.sqlite");
  // 原 Rust VACUUM INTO 产物是关闭后的独立数据库，不依赖 WAL。
  copyFileSync(join(directory, name), destination);
  const database = new DatabaseSync(destination); database.exec("PRAGMA foreign_keys=ON");
  return database;
}
function oracle() {
  assert.ok(directory);
  const raw = parseApplicationJson(readFileSync(resolve(directory, "oracle.json")));
  assert.ok(raw !== null && typeof raw === "object" && !Array.isArray(raw));
  return raw as Record<string, unknown>;
}
test("原Rust双账号材料和请求hash与真实SQLite完全一致", { skip: !directory }, async () => {
  const source = oracle(), db = await database();
  try {
    const repository = new DiscoveryIdentityRepository(db);
    const input = identitySubmitInput.parse(source.input), pair = identityPairInput.parse(source.pair);
    assert.deepEqual(parseApplicationJson(stringifyApplicationJson(repository.material(pair))), source.material);
    assert.deepEqual(parseApplicationJson(stringifyApplicationJson(repository.material({ ...pair, left_search_hit_id: pair.right_search_hit_id, right_search_hit_id: pair.left_search_hit_id }))), source.material);
    assert.equal(identitySubmissionHash("operator", input), source.request_hash);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM discovery_identity_reviews").get()!.n, 0);
  } finally { db.close(); }
});
test("身份判断真实原子写入、重放、历史读取与撤销保留原合同", { skip: !directory }, async () => {
  const source = oracle(), db = await database();
  try {
    const repository = new DiscoveryIdentityRepository(db), input = identitySubmitInput.parse(source.input), expected = identityReviewReceipt.parse(source.receipt);
    const before = Math.floor(Date.now() / 1000), receipt = repository.submit("operator", input), after = Math.floor(Date.now() / 1000);
    assert.deepEqual({ ...receipt, created_at: expected.created_at }, expected);
    assert.ok(BigInt(receipt.created_at) >= BigInt(before) && BigInt(receipt.created_at) <= BigInt(after));
    assert.equal(db.prepare("SELECT request_hash FROM discovery_identity_reviews").get()!.request_hash, source.request_hash);
    assert.deepEqual(repository.submit("operator", input), receipt);
    assert.deepEqual(repository.get("operator", { campaign_id: input.pair.campaign_id, decision_id: input.decision_id }), receipt);
    assert.throws(() => repository.get("other", { campaign_id: input.pair.campaign_id, decision_id: input.decision_id }), /identity_review_account_mismatch/u);
    assert.throws(() => repository.submit("other", input), /identity_review_idempotency_conflict/u);
    assert.throws(() => repository.submit("operator", { ...input, rationale: `${input.rationale} Changed.` }), /identity_review_idempotency_conflict/u);
    assert.throws(() => repository.submit("operator", { ...input, decision_id: "duplicate" }), /identity_review_relationship_already_reviewed/u);
    db.prepare("UPDATE discovery_candidates SET evidence_json=json_set(evidence_json,'$.identity_test_source_change','new evidence') WHERE id=?").run(receipt.candidate_ids[0]);
    assert.deepEqual(repository.submit("operator", input), receipt);
    assert.throws(() => repository.submit("operator", { ...input, decision_id: "stale-material" }), /identity_review_material_changed/u);
    const revoke = identityRevokeInput.parse(source.revocation_input), revoked = repository.revoke("operator", revoke);
    assert.equal(revoked.kind, "revoke"); assert.equal(revoked.revokes_decision_id, input.decision_id);
    assert.equal(revoked.material_hash, receipt.material_hash);
    assert.deepEqual(repository.revoke("operator", revoke), revoked);
    assert.deepEqual(repository.get("operator", { campaign_id: input.pair.campaign_id, decision_id: input.decision_id }), receipt);
    assert.throws(() => repository.revoke("operator", { ...revoke, decision_id: "repeated-revoke" }), /identity_review_already_revoked/u);
    assert.throws(() => repository.revoke("operator", { ...revoke, decision_id: "nested", revokes_decision_id: revoke.decision_id }), /identity_review_cannot_revoke_revocation/u);
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM discovery_identity_reviews").get()!.n, 2);
  } finally { db.close(); }
});
test("原Rust历史账本可直接读取，真实SQLite拒写不产生成功收据", { skip: !directory }, async () => {
  const source = oracle(), merged = await database("merged.sqlite");
  try {
    const repository = new DiscoveryIdentityRepository(merged), input = identitySubmitInput.parse(source.input);
    assert.deepEqual(repository.submit("operator", input), source.receipt);
    assert.deepEqual(repository.get("operator", { campaign_id: input.pair.campaign_id, decision_id: input.decision_id }), source.receipt);
  } finally { merged.close(); }
  const db = await database();
  try {
    const repository = new DiscoveryIdentityRepository(db);
    db.exec("PRAGMA query_only=ON");
    assert.throws(() => repository.submit("operator", source.input));
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM discovery_identity_reviews").get()!.n, 0);
    assert.equal(db.isTransaction, false);
  } finally { db.close(); }
});
test("直接及传递身份冲突必须先撤销，未知关系不制造边", () => {
  assert.throws(() => validateIdentityAddition([["a", "b", "different_authors"], ["a", "c", "same_author"]], "same_author", ["c", "b"]), /identity_review_conflicting_relationship/u);
  assert.throws(() => validateIdentityAddition([["a", "b", "same_author"]], "same_author", ["b", "a"]), /identity_review_relationship_already_reviewed/u);
  assert.throws(() => validateIdentityAddition([["a", "c", "same_author"], ["c", "b", "same_author"]], "different_authors", ["a", "b"]), /identity_review_conflicting_relationship/u);
  validateIdentityAddition([["a", "b", "different_authors"]], "insufficient", ["a", "b"]);
  validateIdentityAddition([["a", "b", "different_authors"]], "same_author", ["a", "c"]);
});

test("四份原Rust账本保留来源、资格与分组，完整数字粉丝显示使用新快照哈希", { skip: !directory }, async () => {
  const source = oracle(), pair = identityPairInput.parse(source.pair);
  const hashes: Record<string, string> = { "baseline.sqlite": "c20ae0e5205393f9430e062ddf18b4dc5933216d5e8aaf523d3d36f0979168d8", "merged.sqlite": "5755f0adff002974a6c6a4a4cba1c8acae24f3edd4b9ceed7c5ad3056bcad2ab", "stale.sqlite": "8b4c54b1d12209b2bdc91e1d80f8bae1971d587dc01748b709b2d9a2ace33400", "revoked.sqlite": "3e16972a6b365ee20e048ece07743fcf92e5acd7fb3851daf6301ffb0c9e8ec0" };
  for (const [name, expected] of [["baseline.sqlite", source.before], ["merged.sqlite", source.merged], ["stale.sqlite", source.stale], ["revoked.sqlite", source.revoked]] as const) {
    const db = await database(name);
    try {
      const actual = new DiscoveryIdentityRepository(db).groups({ campaign_id: pair.campaign_id, expected_revision: pair.expected_revision });
      const converted = z.object({ groups: z.array(z.object({ members: z.array(z.object({ sources: z.array(z.object({ followers: z.string().optional() }).passthrough()) }).passthrough()) }).passthrough()) }).passthrough().parse(structuredClone(expected));
      for (const group of converted.groups) for (const member of group.members) for (const row of member.sources) if (row.followers !== undefined) row.followers = row.followers.replace(/^~/u, "");
      converted.snapshot_hash = hashes[name];
      assert.deepEqual(parseApplicationJson(stringifyApplicationJson(actual)), converted);
    } finally { db.close(); }
  }
});

test("来源和身份发布版本共同CAS、幂等及真实SQLite事务回滚", { skip: !directory }, async () => {
  const source = oracle(), pair = identityPairInput.parse(source.pair), db = await database("merged.sqlite");
  try {
    const repository = new DiscoveryIdentityPublicationRepository(db);
    const basis = repository.readBasis(pair.campaign_id, pair.expected_revision), material = buildIdentityLibraryMaterial(basis);
    const first = repository.reservePublication(basis, material.sources.snapshot_hash, material.identity.snapshot_hash);
    assert.deepEqual(first, { source_revision: 1, identity_revision: 1 });
    assert.deepEqual(repository.reservePublication(basis, material.sources.snapshot_hash, material.identity.snapshot_hash), first);
    db.prepare("UPDATE discovery_library_identity_versions SET identity_revision=? WHERE discovery_campaign_id=?").run(Number.MAX_SAFE_INTEGER, pair.campaign_id);
    assert.throws(() => repository.reservePublication(basis, "a".repeat(64), "b".repeat(64)), /library_identity_version_exhausted/u);
    assert.equal(db.prepare("SELECT source_revision FROM discovery_library_source_versions WHERE discovery_campaign_id=?").get(pair.campaign_id)!.source_revision, 1);
    assert.equal(db.isTransaction, false);
    const connection = new DatabaseSync(db.location()!);
    try { connection.prepare("UPDATE discovery_candidates SET display_name=display_name||' changed' WHERE id=?").run(identityReviewReceipt.parse(source.receipt).candidate_ids[0]); }
    finally { connection.close(); }
    assert.throws(() => repository.reservePublication(basis, material.sources.snapshot_hash, material.identity.snapshot_hash), /library_publication_changed/u);
    assert.equal(db.prepare("SELECT source_revision FROM discovery_library_source_versions WHERE discovery_campaign_id=?").get(pair.campaign_id)!.source_revision, 1);
  } finally { db.close(); }
});
