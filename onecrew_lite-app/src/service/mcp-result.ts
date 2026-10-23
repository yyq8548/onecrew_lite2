import { stringifyApplicationJson } from "../contracts/application-json.ts";

function supportsJsonNumbers(value: unknown): boolean {
  if (typeof value === "bigint") return value >= BigInt(Number.MIN_SAFE_INTEGER) && value <= BigInt(Number.MAX_SAFE_INTEGER);
  if (typeof value === "number") return Number.isFinite(value) && (!Number.isInteger(value) || Number.isSafeInteger(value));
  if (Array.isArray(value)) return value.every(supportsJsonNumbers);
  if (value !== null && typeof value === "object") return Object.values(value).every(supportsJsonNumbers);
  return true;
}

// Host 的普通 JSON 解码不能表示完整 u64；精确正文和初始卡片元数据使用共享 lossless 合同。
export function encodeMcpResult(output: unknown, snapshot?: { binding: { view: { identity: string } } }) {
  const encoded = stringifyApplicationJson(output);
  const safeOutput = supportsJsonNumbers(output);
  const content = [{ type: "text" as const, text: encoded }];
  const structured = { structuredContent: safeOutput ? JSON.parse(encoded) as Record<string, unknown> : { application_json: encoded } };
  if (!snapshot) return { content, ...structured, ...(safeOutput ? {} : { _meta: { "onecrew/lossless-required": true } }) };
  const preciseSnapshot = stringifyApplicationJson(snapshot), safeSnapshot = supportsJsonNumbers(snapshot);
  return { content, ...structured, _meta: { viewUUID: snapshot.binding.view.identity, "onecrew/result-json": preciseSnapshot, ...(safeSnapshot ? { "onecrew/result": JSON.parse(preciseSnapshot) as unknown } : {}), ...(!safeOutput || !safeSnapshot ? { "onecrew/lossless-required": true } : {}) } };
}
