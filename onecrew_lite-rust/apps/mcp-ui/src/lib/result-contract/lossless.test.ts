import { describe, expect, it } from "vitest";
import { parseApplicationJson, stringifyApplicationJson } from "@onecrew/runtime-contracts/json";
import { wireResultInput, wireResultSnapshot } from "@onecrew/runtime-contracts/result-wire";
import { parseInitialResult, parseResultStateOutput } from "./parse";
import { SNAPSHOT } from "@mcp-ui/test-support/fixtures";

describe("lossless browser result protocol", () => {
  it("preserves the complete u64 revision through exact metadata, response and outgoing input", () => {
    const maximum = 18_446_744_073_709_551_615n;
    const snapshot = structuredClone(SNAPSHOT);
    snapshot.binding.revision = maximum;
    snapshot.continuation.continuation_revision = maximum;
    const json = stringifyApplicationJson(snapshot);
    expect(json).toContain('"revision":18446744073709551615');
    expect(wireResultSnapshot.parse(parseApplicationJson(json)).binding.revision).toBe(maximum);
    expect(parseInitialResult({ content: [], _meta: { "onecrew/result-json": json, "onecrew/lossless-required": true, viewUUID: snapshot.binding.view.identity } })).toEqual({ status: "valid", snapshot });
    expect(parseResultStateOutput(parseApplicationJson(stringifyApplicationJson({ kind: "snapshot", snapshot })))).toEqual({ kind: "snapshot", snapshot });
    const input = { action: "read_current", binding: snapshot.binding };
    expect(wireResultInput.parse(parseApplicationJson(stringifyApplicationJson(input)))).toEqual(input);
  });

  it("rejects lossy numbers, overflow, duplicate keys and malformed Unicode", () => {
    expect(wireResultInput.safeParse({ action: "read_current", binding: { ...SNAPSHOT.binding, revision: Number.MAX_SAFE_INTEGER + 1 } }).success).toBe(false);
    expect(wireResultInput.safeParse({ action: "read_current", binding: { ...SNAPSHOT.binding, revision: 18_446_744_073_709_551_616n } }).success).toBe(false);
    expect(() => parseApplicationJson('{"a":1,"a":2}')).toThrow();
    expect(() => parseApplicationJson('{"binding":{},"binding":{}}')).toThrow();
    expect(() => parseApplicationJson('{"a":{"nested":1,"nested":1}}')).toThrow();
    expect(() => parseApplicationJson('{"a":1,}')).toThrow();
    expect(() => parseApplicationJson('{/* comment */"a":1}')).toThrow();
    expect(() => parseApplicationJson(new Uint8Array([0xc3, 0x28]))).toThrow();
    expect(() => parseApplicationJson(new Uint8Array([0xef, 0xbb, 0xbf, 0x7b, 0x7d]))).toThrow();
    expect(() => parseApplicationJson('"\\ud800"')).toThrow();
    expect(() => stringifyApplicationJson({ revision: Number.MAX_SAFE_INTEGER + 1 })).toThrow();
    expect(() => stringifyApplicationJson({ invalid: Number.POSITIVE_INFINITY })).toThrow();
    expect(parseInitialResult({ content: [], _meta: { "onecrew/result": SNAPSHOT, "onecrew/lossless-required": true } })).toEqual({ status: "invalid" });
    expect(parseInitialResult({ content: [], _meta: { "onecrew/result": SNAPSHOT, "onecrew/result-json": '{"binding":{},"binding":{}}', viewUUID: SNAPSHOT.binding.view.identity } })).toEqual({ status: "invalid" });
  });
});
