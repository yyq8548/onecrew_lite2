import assert from "node:assert/strict";
import { test } from "node:test";
import { loadCurrentMcpRegistry, loadOriginalMcpRegistry } from "../src/service/mcp-catalogue.ts";
import { McpJsonSchemaValidator } from "../src/service/mcp-json-schema.ts";

test("MCP 当前合同包含额外条件和修复检查点，冻结来源保持不变", async () => {
  const original = await loadOriginalMcpRegistry(), current = await loadCurrentMcpRegistry();
  const validator = new McpJsonSchemaValidator();
  const review = current.entries.find(entry => entry.kind === "operation" && entry.metadata.id === "onecrew.discovery.review.submit")!;
  assert.equal(review.kind, "operation");
  if (review.kind !== "operation") throw new Error("operation_missing");
  const finding = { criterion: { requirement: 0 }, outcome: "insufficient", rationale: "缺少条件证据", citations: [], tool: null, country_code: null, match_level: null };
  const input = { campaign_id: "plan", expected_revision: 1, search_hit_id: "hit", material_hash: "a".repeat(64), request_id: "review", reviewer: { kind: "agent", label: "reviewer" }, findings: Array.from({ length: 32 }, (_, requirement) => ({ ...finding, criterion: { requirement } })) };
  validator.validateOperation(review.metadata.id, review.input_revision, review.input_schema, input);
  assert.throws(() => validator.validateOperation(review.metadata.id, review.input_revision, review.input_schema, { ...input, findings: [{ ...finding, criterion: { requirement: 32 } }] }), /contract_mismatch/);
  assert.throws(() => validator.validateOperation(review.metadata.id, review.input_revision, review.input_schema, { ...input, findings: Array(41).fill(finding) }), /contract_mismatch/);
  const originalReview = original.entries.find(entry => entry.kind === "operation" && entry.metadata.id === review.metadata.id)!;
  assert.equal(originalReview.kind, "operation");
  if (originalReview.kind !== "operation") throw new Error("operation_missing");
  assert.notEqual(review.input_revision, originalReview.input_revision);
  assert.equal((originalReview.input_schema.properties as Record<string, { maxItems: number }>).findings!.maxItems, 6);
  for (const surface of current.surfaces) for (const group of surface.get) for (const operation of group.operations) if (operation.id === review.metadata.id) {
    assert.deepEqual(operation.input_schema, review.input_schema);
    assert.equal(operation.schema_revision, review.input_revision);
  }
  const search = current.entries.find(entry => entry.kind === "operation" && entry.metadata.id === "onecrew.discovery.search_once")!;
  if (search.kind !== "operation") throw new Error("operation_missing");
  validator.validateOperation(search.metadata.id, search.input_revision, search.input_schema, { campaign_id: "plan", expected_revision: 1, platform: "youtube", profile_ref: "work", limit: 5, recovery: { expected_stop_code: "provider_response_invalid_json", expected_stopped_at: 123 } });
  assert.throws(() => validator.validateOperation(search.metadata.id, search.input_revision, search.input_schema, { campaign_id: "plan", expected_revision: 1, platform: "youtube", profile_ref: "work", limit: 5, recovery: { expected_stop_code: "provider_response_invalid_json" } }), /contract_mismatch/);
});
