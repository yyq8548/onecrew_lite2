import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";

const object = z.record(z.string(), z.unknown());
function rejectUnsafeNumbers(value: unknown): void {
  if (typeof value === "number" && (!Number.isFinite(value) || Number.isInteger(value) && !Number.isSafeInteger(value)) || typeof value === "bigint") throw new Error("mcp_input_lossless_json_required");
  if (Array.isArray(value)) for (const item of value) rejectUnsafeNumbers(item);
  else if (value !== null && typeof value === "object") for (const item of Object.values(value)) rejectUnsafeNumbers(item);
}

// application_json 是完整参数对象的精确编码，不能与普通字段混合。
export function decodeMcpInput(input: unknown): Record<string, unknown> {
  const raw = object.parse(input);
  if (Object.hasOwn(raw, "application_json")) {
    const envelope = z.object({ application_json: z.string().refine(value => Buffer.byteLength(value) <= 1024 * 1024) }).strict().parse(raw);
    return object.parse(parseApplicationJson(envelope.application_json));
  }
  rejectUnsafeNumbers(raw);
  return raw;
}
