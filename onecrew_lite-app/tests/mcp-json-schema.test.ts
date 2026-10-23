import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { CreatorRepository } from "../src/creators/repository.ts";
import { loadOriginalMcpRegistry } from "../src/service/mcp-catalogue.ts";
import { FrozenMcpOutputValidator, McpJsonSchemaValidator, nativeMcpOutputSchema } from "../src/service/mcp-json-schema.ts";

test("Rust integer formats check exact bigint boundaries while native SDK objects stay safe", () => {
  const validator = new McpJsonSchemaValidator();
  for (const [format, maximum] of [["uint8", 255n], ["uint16", 65535n], ["uint32", 4294967295n], ["uint64", 18446744073709551615n], ["uint", 18446744073709551615n], ["int32", 2147483647n], ["int64", 9223372036854775807n]] as const) {
    const validate = validator.getValidator({ type: "object", additionalProperties: false, required: ["value"], properties: { value: { type: ["integer", "null"], format } } });
    assert.equal(validate({ value: maximum }).valid, true, format);
    assert.equal(validate({ value: maximum + 1n }).valid, false, format);
    assert.equal(validate({ value: null }).valid, true, format);
    assert.equal(validate({ value: Number.MAX_SAFE_INTEGER + 1 }).valid, false, format);
    assert.equal(validate({ value: "42" }).valid, false, format);
    if (format.startsWith("u")) assert.equal(validate({ value: -1n }).valid, false, format);
  }
  const schema = nativeMcpOutputSchema({ type: "object", required: ["revision"], properties: { revision: { type: "integer", minimum: 1, format: "uint64" } } });
  assert.equal(JSON.stringify(schema).includes('"format":"uint64"'), false);
  assert.equal(JSON.stringify(schema).includes(`"maximum":${Number.MAX_SAFE_INTEGER}`), true);
});

test("frozen actual Creator output accepts u64MAX and rejects invalid data before lossless transport wrapping", async () => {
  const database = new DatabaseSync(":memory:");
  try {
    database.exec("PRAGMA foreign_keys=ON");
    for (const file of ["creator-schema90.sql", "creator-indexes90.sql", "creator-triggers90.sql"]) database.exec(readFileSync(new URL(`../src/storage/${file}`, import.meta.url), "utf8"));
    database.exec("PRAGMA user_version=90");
    const repository = new CreatorRepository(database), creator = repository.create({ display_name: "Frozen contract actual SQLite" });
    repository.update({ creator_id: creator.creator_id, follower_counts: { x: 18446744073709551615n } });
    const detail = repository.get(creator.creator_id), validator = new FrozenMcpOutputValidator(await loadOriginalMcpRegistry(), "full");
    const result = (value: unknown) => ({ batch_id: "contract-boundary", outcomes: [{ status: "completed", call_id: "read", operation_id: "onecrew.creator.get", result: value }], result_ref: null, presentation_failure: null });
    validator.validate("onecrew_tool_execute", result(detail));
    assert.throws(() => validator.validate("onecrew_tool_execute", result({ ...detail, creator: { ...detail.creator, follower_counts: { x: 18446744073709551616n } } })), /output_contract/);
    assert.throws(() => validator.validate("onecrew_tool_execute", result({ ...detail, creator: { ...detail.creator, follower_counts: { x: "18446744073709551615" } } })), /output_contract/);
    assert.throws(() => validator.validate("onecrew_tool_execute", { application_json: "arbitrary unvalidated text" }), /output_contract/);
  } finally { database.close(); }
});
