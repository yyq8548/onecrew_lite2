import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";
import { compileTableSyncRequest } from "../src/sync/table-sync-client.ts";
import { parseApplicationJson } from "../src/contracts/application-json.ts";

const version = "11111111-1111-4111-8111-111111111111", attempt = "22222222-2222-4222-8222-222222222222";
test("dynamic table sync compiles only original fixed endpoints and exact binary part bytes", () => {
  const bytes = Buffer.from("abc"), digest = createHash("md5").update(bytes).digest("base64");
  const part = compileTableSyncRequest({ operation: "part", version_id: version, attempt_id: attempt, number: 1, md5_base64: digest, bytes });
  assert.equal(part.method, "PUT");
  assert.equal(part.url.href, `https://api.onecrewai.io/dynamic-table-sync/uploads/${version}/parts/1`);
  assert.equal(part.headers.get("content-md5"), "kAFQmDzST7DWlj99KOF/cg==");
  assert.equal(part.headers.get("x-onecrew-upload-attempt"), attempt);
  assert.equal(part.headers.get("content-length"), "3");
  assert.deepEqual(part.body, bytes);
  bytes[0] = 0;
  assert.equal(part.body!.toString(), "abc");
  for (const number of [0, 2049]) assert.throws(() => compileTableSyncRequest({ operation: "part", version_id: version, attempt_id: attempt, number, md5_base64: digest, bytes }));
  assert.throws(() => compileTableSyncRequest({ operation: "capabilities", url: "https://other.example" }));
  for (const operation of ["capabilities", "consent"] as const) assert.equal(compileTableSyncRequest({ operation }).url.pathname, `/dynamic-table-sync/${operation}`);
  assert.equal(compileTableSyncRequest({ operation: "status", version_id: version }).url.pathname, `/dynamic-table-sync/uploads/${version}`);
});

test("dynamic table sync consent CAS, abort ownership and control byte limits retain the original wire contract", () => {
  const create = compileTableSyncRequest({ operation: "set_consent", input: { action: "grant", operation_id: attempt }, etag: null });
  assert.equal(create.headers.get("if-none-match"), "*");
  assert.equal(create.headers.has("if-match"), false);
  const update = compileTableSyncRequest({ operation: "set_consent", input: { action: "revoke", operation_id: attempt }, etag: '"actual-revision"' });
  assert.equal(update.headers.get("if-match"), '"actual-revision"');
  assert.equal(update.headers.has("if-none-match"), false);
  assert.throws(() => compileTableSyncRequest({ operation: "set_consent", input: {}, etag: "bad\r\nInjected: yes" }));
  const abort = compileTableSyncRequest({ operation: "abort", version_id: version, attempt_id: attempt });
  assert.deepEqual(parseApplicationJson(abort.body!), { attempt_id: attempt });
  assert.equal(abort.url.pathname, `/dynamic-table-sync/uploads/${version}/abort`);
  const complete = compileTableSyncRequest({ operation: "complete", version_id: version, input: { record_count: 18446744073709551615n } });
  assert.match(complete.body!.toString(), /18446744073709551615/);
  assert.throws(() => compileTableSyncRequest({ operation: "begin", input: { content: "中".repeat(6000) } }), /control_too_large/);
  assert.throws(() => compileTableSyncRequest({ operation: "complete", version_id: version, input: { content: "中".repeat(180000) } }), /control_too_large/);
});
