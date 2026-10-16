import { isInteger, isSafeNumber, parse, parseLosslessNumber } from "lossless-json";
import { assertStrictJson, isWellFormedUnicode } from "./json.ts";

// 使用成熟 JSON 解析器保留数字词法并拒绝重复键，避免先舍入再验证旧 u64 字段。
export function parseStoredJson(text: string, unsignedIntegersOnly = false): unknown {
  assertStrictJson(text);
  return parse(text, (key, value) => {
    if (!isWellFormedUnicode(key) || typeof value === "string" && !isWellFormedUnicode(value)) {
      throw new Error("Stored JSON contains invalid Unicode");
    }
    return value;
  }, { parseNumber: raw => {
    if (!unsignedIntegersOnly) return parseLosslessNumber(raw);
    if (raw.startsWith("-") || !isInteger(raw) || !isSafeNumber(raw)) {
      throw new Error("Stored JSON requires unsigned integers within the JavaScript safe range");
    }
    return Number(raw);
  } });
}
