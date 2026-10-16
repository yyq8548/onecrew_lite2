import assert from "node:assert/strict";
import test from "node:test";
import { decodeMcpInput } from "../src/service/mcp-input.ts";

test("MCP full argument envelope preserves u64 and rejects mixed or already rounded inputs", () => {
  assert.deepEqual(decodeMcpInput({ action: "read_current", binding: { revision: 2 } }), { action: "read_current", binding: { revision: 2 } });
  assert.deepEqual(decodeMcpInput({ application_json: '{"action":"read_current","binding":{"revision":18446744073709551615}}' }), { action: "read_current", binding: { revision: 18446744073709551615n } });
  assert.throws(() => decodeMcpInput({ action: "read_current", binding: { revision: Number(18446744073709551615n) } }), /lossless_json_required/);
  assert.throws(() => decodeMcpInput({ application_json: "{}", action: "confirm" }));
  assert.throws(() => decodeMcpInput({ application_json: '{"binding":{},"binding":{}}' }), /Duplicate/);
  assert.throws(() => decodeMcpInput({ application_json: '{"instruction":"\\ud800"}' }));
  assert.throws(() => decodeMcpInput({ application_json: "[]" }));
  assert.throws(() => decodeMcpInput({ application_json: `{"text":"${"a".repeat(1024 * 1024)}"}` }));
});
