import { isInteger, isSafeNumber, parse, stringify } from "lossless-json";
import { visit } from "jsonc-parser";
export const isWellFormedUnicode = (value: string): boolean => !/[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/.test(value);

export function assertStrictJson(text: string): void {
  const objects: Set<string>[] = [];
  visit(text, {
    onObjectBegin: () => { objects.push(new Set()); },
    onObjectProperty: property => {
      const properties = objects.at(-1);
      if (!properties || properties.has(property)) throw new SyntaxError("Duplicate key in JSON");
      properties.add(property);
    },
    onObjectEnd: () => { objects.pop(); },
    onError: () => { throw new SyntaxError("Invalid strict JSON"); },
  }, { disallowComments: true, allowTrailingComma: false, allowEmptyContent: false });
}

function decodeUtf8(bytes: Uint8Array): string {
  try { return new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes); }
  catch (cause) { throw new SyntaxError("JSON requires valid UTF-8", { cause }); }
}

export function parseApplicationJson(bytes: Uint8Array | string): unknown {
  const text = typeof bytes === "string" ? bytes : decodeUtf8(bytes);
  if (!isWellFormedUnicode(text)) throw new SyntaxError("JSON contains invalid Unicode");
  assertStrictJson(text);
  return parse(text, (key, value) => {
    if (!isWellFormedUnicode(key) || typeof value === "string" && !isWellFormedUnicode(value)) throw new SyntaxError("JSON contains invalid Unicode");
    return value;
  }, { parseNumber: raw => {
    if (isInteger(raw)) return isSafeNumber(raw) ? Number(raw) : BigInt(raw);
    if (!isSafeNumber(raw)) throw new SyntaxError("JSON number cannot be represented without loss");
    return Number(raw);
  } });
}

export function stringifyApplicationJson(value: unknown): string {
  const result = stringify(value, (key, item) => {
    if (!isWellFormedUnicode(key) || typeof item === "string" && !isWellFormedUnicode(item)) throw new SyntaxError("JSON contains invalid Unicode");
    if (typeof item === "number" && (!Number.isFinite(item) || Number.isInteger(item) && !Number.isSafeInteger(item))) throw new RangeError("JSON number cannot be represented without loss");
    return item;
  });
  if (result === undefined) throw new Error("Application response is not JSON");
  return result;
}
