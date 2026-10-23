import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { z } from "zod";
import { McpJsonSchemaValidator } from "../src/service/mcp-json-schema.ts";
import { TELEGRAM_CONNECTION_OPERATION_IDS, telegramConnectionOperationSchemas } from "../src/platforms/browser/telegram-connection-operations.ts";

test("十项Telegram生命周期真实原metadata与实际参数schema保留秘密类别/确认/未选平台连接合同", () => {
  const source = z.object({ entries: z.array(z.record(z.string(), z.unknown())) }).parse(JSON.parse(readFileSync(new URL("../src/service/mcp-registry-data.json", import.meta.url), "utf8")));
  const entrySchema = z.object({ metadata: z.object({ id: z.string() }), kind: z.literal("operation"), input_schema: z.record(z.string(), z.unknown()), input_revision: z.string(), connection: z.unknown(), secret: z.string(), sensitive_input_paths: z.array(z.string()), safety: z.object({ confirmation: z.object({ parameter_pointers: z.array(z.string()) }) }) });
  const validator = new McpJsonSchemaValidator();
  const samples: Record<typeof TELEGRAM_CONNECTION_OPERATION_IDS[number], unknown> = {
    "telegram.connection.login.start": { api_id: 12345, api_hash: "api-hash-secret", proxy: { kind: "socks5", host: "proxy.example", port: 1080 }, idempotency_key: "telegram-qr-default" },
    "telegram.connection.login.status": { operation_id: "operation" }, "telegram.connection.login.otp.submit": { operation_id: "operation", otp: "12345" }, "telegram.connection.login.two_factor.submit": { operation_id: "operation", password: "private" }, "telegram.connection.login.cancel": { operation_id: "operation", confirmed: true },
    "telegram.connection.reconnect.prepare": { platform: "telegram", intent: "reconnect", method: "existing_account", operator_account_id: "selected" }, "telegram.connection.reconnect.begin": { flow_id: "flow", confirmed: true, idempotency_key: "begin" }, "telegram.connection.reconnect.status": { flow_id: "flow" }, "telegram.connection.reconnect.finalize": { flow_id: "flow", confirmed: true, idempotency_key: "finalize" }, "telegram.connection.reconnect.cancel": { flow_id: "flow", confirmed: true },
  };
  for (const id of TELEGRAM_CONNECTION_OPERATION_IDS) {
    const raw = source.entries.find(entry => entry.kind === "operation" && entry.metadata !== null && typeof entry.metadata === "object" && "id" in entry.metadata && entry.metadata.id === id); const entry = entrySchema.parse(raw), schema = telegramConnectionOperationSchemas[id], sample = samples[id];
    assert.deepEqual(entry.connection, { kind: "none" });
    assert.equal(entry.secret, /login\.(start|otp\.submit|two_factor\.submit)$/u.test(id) ? "transient" : "none");
    assert.deepEqual(entry.sensitive_input_paths, id.endsWith("login.start") ? ["/api_hash", "/proxy/password", "/phone"] : id.endsWith("otp.submit") ? ["/otp"] : id.endsWith("two_factor.submit") ? ["/password"] : []);
    assert.deepEqual(entry.safety.confirmation.parameter_pointers, /\.(begin|finalize|cancel)$/u.test(id) ? ["/confirmed"] : []);
    validator.validateOperation(id, entry.input_revision, entry.input_schema, sample); assert.equal(schema.safeParse(sample).success, true);
    const extra = { ...z.record(z.string(), z.unknown()).parse(sample), injected: "not_allowed" };
    assert.throws(() => validator.validateOperation(id, entry.input_revision, entry.input_schema, extra)); assert.equal(schema.safeParse(extra).success, false);
  }
  assert.equal(telegramConnectionOperationSchemas["telegram.connection.reconnect.prepare"].safeParse({ platform: "x", intent: "reconnect", method: "existing_account" }).success, false);
  assert.equal(telegramConnectionOperationSchemas["telegram.connection.login.two_factor.submit"].safeParse({ flow_id: "flow", password: "private" }).success, false);
});
