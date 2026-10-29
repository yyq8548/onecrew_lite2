import { Ajv2020 } from "ajv/dist/2020.js";
import addFormats from "ajv-formats";
import type { ValidateFunction } from "ajv";
import type { JsonSchemaType, JsonSchemaValidator, jsonSchemaValidator } from "@modelcontextprotocol/sdk/validation/types.js";
import { z } from "zod";
import type { McpCatalogueData } from "./mcp-catalogue.ts";

const formats = new Map<string, readonly [bigint, bigint]>([
  ["uint8", [0n, 255n]], ["uint16", [0n, 65535n]], ["uint32", [0n, 4294967295n]], ["uint64", [0n, 18446744073709551615n]], ["uint", [0n, 18446744073709551615n]],
  ["int32", [-2147483648n, 2147483647n]], ["int64", [-9223372036854775808n, 9223372036854775807n]],
]);
const integerRule = z.strictObject({ nullable: z.boolean(), format: z.string().optional(), minimum: z.union([z.string(), z.number()]).optional(), maximum: z.union([z.string(), z.number()]).optional(), exclusiveMinimum: z.union([z.string(), z.number()]).optional(), exclusiveMaximum: z.union([z.string(), z.number()]).optional() });
const schemaMaps = new Set(["properties", "patternProperties", "$defs", "definitions", "dependentSchemas"]);
const schemaArrays = new Set(["allOf", "anyOf", "oneOf", "prefixItems"]);
const schemaChildren = new Set(["items", "additionalProperties", "unevaluatedProperties", "unevaluatedItems", "not", "if", "then", "else", "contains", "propertyNames"]);
const record = (value: unknown): value is Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value);
function transformSchema(raw: unknown, native: boolean): unknown {
  if (typeof raw === "boolean") return raw;
  if (!record(raw)) throw new Error("mcp_json_schema_invalid");
  const result = structuredClone(raw);
  for (const [key, value] of Object.entries(raw)) {
    if (schemaMaps.has(key)) { if (!record(value)) throw new Error("mcp_json_schema_invalid"); result[key] = Object.fromEntries(Object.entries(value).map(([name, schema]) => [name, transformSchema(schema, native)])); }
    else if (schemaArrays.has(key)) { if (!Array.isArray(value)) throw new Error("mcp_json_schema_invalid"); result[key] = value.map(schema => transformSchema(schema, native)); }
    else if (schemaChildren.has(key)) result[key] = Array.isArray(value) ? value.map(schema => transformSchema(schema, native)) : transformSchema(value, native);
  }
  const integer = raw.type === "integer" || Array.isArray(raw.type) && raw.type.includes("integer");
  if (!integer) return result;
  if (raw.type !== "integer" && (!Array.isArray(raw.type) || raw.type.length !== 2 || !raw.type.includes("null"))) throw new Error("mcp_json_schema_integer_union_unsupported");
  if (raw.multipleOf !== undefined) throw new Error("mcp_json_schema_integer_multiple_unsupported");
  if (raw.format !== undefined && (typeof raw.format !== "string" || !formats.has(raw.format))) throw new Error("mcp_json_schema_integer_format_unsupported");
  if (native) {
    const range = typeof raw.format === "string" ? formats.get(raw.format)! : [-BigInt(Number.MAX_SAFE_INTEGER), BigInt(Number.MAX_SAFE_INTEGER)] as const;
    const minimum = range[0] < BigInt(Number.MIN_SAFE_INTEGER) ? Number.MIN_SAFE_INTEGER : Number(range[0]);
    const maximum = range[1] > BigInt(Number.MAX_SAFE_INTEGER) ? Number.MAX_SAFE_INTEGER : Number(range[1]);
    const minimumBound = raw.minimum, maximumBound = raw.maximum;
    if (minimumBound !== undefined && typeof minimumBound !== "number" && typeof minimumBound !== "bigint" || maximumBound !== undefined && typeof maximumBound !== "number" && typeof maximumBound !== "bigint") throw new Error("mcp_json_schema_numeric_bound_invalid");
    result.minimum = minimumBound === undefined || minimumBound < minimum ? minimum : Number(minimumBound);
    result.maximum = maximumBound === undefined || maximumBound > maximum ? maximum : Number(maximumBound);
    delete result.format;
  } else {
    const rule: Record<string, unknown> = { nullable: Array.isArray(raw.type), ...(typeof raw.format === "string" ? { format: raw.format } : {}) };
    for (const key of ["minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum"]) if (raw[key] !== undefined) {
      const value = raw[key]; if (typeof value !== "number" && typeof value !== "bigint") throw new Error("mcp_json_schema_numeric_bound_invalid");
      rule[key] = typeof value === "bigint" ? value.toString() : value; delete result[key];
    }
    delete result.type; delete result.format; result.onecrewInteger = rule;
  }
  return result;
}
function validateInteger(raw: unknown, value: unknown): boolean {
  const rule = integerRule.parse(raw);
  if (value === null) return rule.nullable;
  if (typeof value !== "bigint" && !(typeof value === "number" && Number.isSafeInteger(value))) return false;
  const integer = BigInt(value), range = rule.format === undefined ? undefined : formats.get(rule.format);
  if (range && (integer < range[0] || integer > range[1])) return false;
  const bound = (input: string | number) => typeof input === "string" ? BigInt(input) : input;
  return !(rule.minimum !== undefined && integer < bound(rule.minimum) || rule.maximum !== undefined && integer > bound(rule.maximum) || rule.exclusiveMinimum !== undefined && integer <= bound(rule.exclusiveMinimum) || rule.exclusiveMaximum !== undefined && integer >= bound(rule.exclusiveMaximum));
}
function ajv() {
  const result = new Ajv2020({ strict: false, validateSchema: true, validateFormats: true, allErrors: true });
  addFormats.default(result);
  result.addKeyword({ keyword: "onecrewInteger", schemaType: "object", errors: false, validate: validateInteger });
  for (const [name, range] of formats) result.addFormat(name, { type: "number", validate: (value: number) => Number.isSafeInteger(value) && BigInt(value) >= range[0] && BigInt(value) <= range[1] });
  return result;
}
export function nativeMcpOutputSchema(raw: Record<string, unknown>): Record<string, unknown> {
  const native = transformSchema(raw, true);
  return { type: "object", anyOf: [native, { type: "object", additionalProperties: false, required: ["application_json"], properties: { application_json: { type: "string", minLength: 1, maxLength: 40 * 1024 * 1024 } } }] };
}
export class McpJsonSchemaValidator implements jsonSchemaValidator {
  readonly #ajv = ajv();
  readonly #inputs = new Map<string, ValidateFunction>();
  validateOperation(id: string, revision: string, schema: Record<string, unknown>, input: unknown): void {
    const key = `${id}\0${revision}`;
    let validator = this.#inputs.get(key);
    if (!validator) {
      const transformed = transformSchema(schema, false);
      if (!record(transformed)) throw new Error("mcp_json_schema_invalid");
      validator = this.#ajv.compile(transformed); this.#inputs.set(key, validator);
    }
    if (!validator(input)) throw new Error("mcp_input_contract_mismatch");
  }
  getValidator<T>(schema: JsonSchemaType): JsonSchemaValidator<T> {
    const transformed = transformSchema(schema, false);
    if (!record(transformed)) throw new Error("mcp_json_schema_invalid");
    const validator = this.#ajv.compile(transformed);
    return input => validator(input) ? { valid: true, data: input as T, errorMessage: undefined } : { valid: false, data: undefined, errorMessage: this.#ajv.errorsText(validator.errors) };
  }
}
export class FrozenMcpOutputValidator {
  readonly #tools = new Map<string, () => ValidateFunction>();
  readonly #operations = new Map<string, () => ValidateFunction>();
  constructor(data: McpCatalogueData, surface: string) {
    const validator = ajv();
    const lazyValidator = (raw: unknown) => {
      let compiled: ValidateFunction | undefined;
      return () => {
        if (!compiled) {
          const transformed = transformSchema(raw, false);
          if (!record(transformed)) throw new Error("mcp_json_schema_invalid");
          compiled = validator.compile(transformed);
        }
        return compiled;
      };
    };
    const tools = data.tools.find(value => value.surface.toLowerCase() === surface)?.tools;
    if (!tools) throw new Error("mcp_surface_manifest_missing");
    for (const tool of tools) if (typeof tool.name === "string" && record(tool.outputSchema)) this.#tools.set(tool.name, lazyValidator(tool.outputSchema));
    for (const entry of data.entries) if (entry.kind === "operation") this.#operations.set(entry.metadata.id, lazyValidator(entry.output_schema));
  }
  validate(tool: string, value: unknown): void {
    const validator = this.#tools.get(tool)?.();
    if (!validator || !validator(value)) throw new Error("mcp_output_contract_mismatch");
    if (tool === "onecrew_tool_execute" && record(value) && Array.isArray(value.outcomes)) for (const outcome of value.outcomes) {
      if (record(outcome) && outcome.status === "completed") {
        const operation = typeof outcome.operation_id === "string" ? this.#operations.get(outcome.operation_id)?.() : undefined;
        if (!operation || !operation(outcome.result)) throw new Error("mcp_operation_output_contract_mismatch");
      }
    }
  }
}
