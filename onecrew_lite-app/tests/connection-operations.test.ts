import assert from "node:assert/strict";
import { randomBytes, randomUUID } from "node:crypto";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { z } from "zod";
import { connectionOperationIds, connectionOperationScope, connectionCheckSchema, connectionProviderFailure, projectOperatorConnectionGet, projectYoutubeConnectionGet } from "../src/platforms/connection-operations.ts";
import { readStorageSql } from "../src/storage/sql-resource.ts";
import { encryptOwnerVaultSecret } from "../src/platforms/browser/owner-vault-store.ts";
import { decryptVaultEnvelope } from "../src/storage/vault-reader.ts";
import { platformProfileSecretSchema } from "../src/platforms/browser/platform-vault.ts";
import { parseStoredJson } from "../src/contracts/stored-json.ts";
import { XReadFailure } from "../src/platforms/browser/native-x-search.ts";
import { NativeReadTransportError } from "../src/native-capabilities/network.ts";
import { InstagramLoginResponseError, InstagramLoginSessionError } from "../src/platforms/browser/browser-login-identity.ts";
import { loadCurrentMcpRegistry } from "../src/service/mcp-catalogue.ts";
import { FrozenMcpOutputValidator } from "../src/service/mcp-json-schema.ts";

const wire = (operation: string, result: unknown) => ({ batch_id: "connection-contract", outcomes: [{ status: "completed", call_id: "read", operation_id: operation, result }], result_ref: null, presentation_failure: null });
test("真实Vault4元数据与AES读取保留三平台完整详情、坏密文不可用且隐藏TG电话与XChat", async () => {
  const database = new DatabaseSync(":memory:"), key = randomBytes(32), validator = new FrozenMcpOutputValidator(await loadCurrentMcpRegistry(), "full");
  try {
    database.exec(readStorageSql("vault-schema.sql"));
    for (const platform of ["x", "instagram", "telegram"] as const) {
      const id = randomUUID(), secret = { provider_account_id: "12345", cookies: [], x_chat_passcode: null, instagram_session_settings: null, telegram_api_id: null, telegram_api_hash: null, telegram_session: null, profile_name: "Public label", browser_profile_dir: "", fingerprint_seed: "", proxy: null, proxy_egress: null };
      const bytes = Buffer.from(JSON.stringify(secret)), encrypted = encryptOwnerVaultSecret(key, bytes); bytes.fill(0);
      database.prepare("INSERT INTO profiles(id,platform,provider_account_id,username,encrypted_secret,created_at,updated_at) VALUES(?,?,?,'creator',?,'1','2')").run(id, platform, "12345", encrypted);
      const row = database.prepare("SELECT * FROM profiles WHERE id=?").get(id)!; assert.ok(row.encrypted_secret instanceof Uint8Array);
      const clear = decryptVaultEnvelope(key, row.encrypted_secret); const decoded = platformProfileSecretSchema.parse(parseStoredJson(clear.toString("utf8"))); clear.fill(0);
      const { encrypted_secret: _encrypted, ...metadata } = row;
      const rows = [{ ...metadata, accountRefId: id, credentialReadable: true, profileName: decoded.profile_name }];
      const output = projectOperatorConnectionGet(platform, id, rows); assert.equal(output.status, "found"); if (output.status !== "found") throw new Error("Expected actual Vault row");
      assert.equal(output.detail.label, platform === "telegram" ? "@creator" : "Public label"); assert.equal(output.detail.configured, true); assert.equal("x_chat_passcode_status" in output.detail, false);
      validator.validate("onecrew_tool_execute", wire(`${platform}.connection.get`, output));
      const unavailable = projectOperatorConnectionGet(platform, id, [{ ...rows[0], credentialReadable: false, profileName: null }]); validator.validate("onecrew_tool_execute", wire(`${platform}.connection.get`, unavailable));
      assert.equal(unavailable.status === "found" && unavailable.detail.credential_availability, "unavailable");
      if (platform === "telegram") { const phone = projectOperatorConnectionGet(platform, id, [{ ...rows[0], username: "+1 (555) 123-4567", profileName: "+1 (555) 123-4567" }]); assert.equal(phone.status === "found" && phone.detail.username, ""); assert.equal(JSON.stringify(phone).includes("555"), false); }
      assert.deepEqual(projectOperatorConnectionGet(platform, "missing", rows), { status: "not_found", operator_account_id: "missing" });
      assert.throws(() => decryptVaultEnvelope(key, new Uint8Array([1, 2, 3])));
    }
    assert.equal(database.prepare("SELECT count(*) AS n FROM profiles").get()?.n, 3);
  } finally { key.fill(0); database.close(); }
});
test("真实Vault4 YouTube元数据详情保持原issues顺序与u64历史值", async () => {
  const database = new DatabaseSync(":memory:"), validator = new FrozenMcpOutputValidator(await loadCurrentMcpRegistry(), "full"), id = randomUUID();
  try {
    database.exec(readStorageSql("vault-schema.sql"));
    database.prepare("INSERT INTO service_credentials(id,provider,label,metadata_json,encrypted_secret,disabled,cooldown_until,last_used_at,last_error,created_at,updated_at) VALUES(?,'youtube','Key',?, ?,1,-1,9007199254740993,'private provider text','1','2')").run(id, JSON.stringify({ api: "invalid", project: 1 }), randomBytes(32));
    const statement = database.prepare("SELECT * FROM service_credentials WHERE id=?"); statement.setReadBigInts(true); const row = statement.get(id)!;
    const { encrypted_secret: _encrypted, ...metadata } = row;
    const output = projectYoutubeConnectionGet(id, [{ ...metadata, accountRefId: id, credentialReadable: false }], 100); assert.equal(output.status, "found"); if (output.status !== "found") throw new Error("Expected actual Vault row");
    assert.deepEqual(output.detail.issues, ["invalid_metadata", "credential_unreadable", "disabled", "cooling_down", "recorded_failure"]); assert.equal(output.detail.cooldown_until, 18446744073709551615n); assert.equal(output.detail.last_checked_at, 9007199254740993n);
    assert.equal(JSON.stringify(output.detail.issues).includes("private provider text"), false); validator.validate("onecrew_tool_execute", wire("youtube.connection.get", output));
    const scalar = projectYoutubeConnectionGet(id, [{ ...metadata, metadata_json: "null", accountRefId: id, credentialReadable: true }], 100);
    assert.equal(scalar.status, "found"); if (scalar.status === "found") assert.equal(scalar.detail.issues[0], "invalid_metadata");
    assert.deepEqual(projectYoutubeConnectionGet("missing", []), { status: "not_found", credential_id: "missing" });
  } finally { database.close(); }
});
test("九个实际连接合同严守空参数与toolkit、错误精确分类且不吞本地异常", async () => {
  const validator = new FrozenMcpOutputValidator(await loadCurrentMcpRegistry(), "full"); assert.equal(connectionOperationIds.length, 9);
  for (const operation of connectionOperationIds) {
    const platform = operation.split(".")[0]!, connection = platform === "youtube" ? { kind: "service_credential", toolkit: "youtube", credential_id: randomUUID() } : platform === "email" ? { kind: "mailbox", toolkit: "email", mailbox_ref_id: "mailbox" } : { kind: "operator_account", toolkit: platform, operator_account_id: randomUUID() };
    const input = { operation_id: operation, schema_revision: "1", connection, arguments: {} };
    assert.equal(connectionOperationScope(operation).safeParse(input).success, true);
    assert.equal(connectionOperationScope(operation).safeParse({ ...input, arguments: { secret: "not allowed" } }).success, false);
    assert.equal(connectionOperationScope(operation).safeParse({ ...input, connection: { kind: "none" } }).success, false);
    if (operation.endsWith(".check")) for (const status of ["ok", "proxy_unreachable", "network_unreachable", "credential_rejected", "provider_unavailable"] as const) validator.validate("onecrew_tool_execute", wire(operation, connectionCheckSchema.parse({ status, proxy_configured: false, egress: null, account_handle: null, checked_at: 18446744073709551615n })));
  }
  assert.equal(connectionProviderFailure("x", new XReadFailure(null)), "network_unreachable"); assert.equal(connectionProviderFailure("x", new XReadFailure(401)), "credential_rejected"); assert.equal(connectionProviderFailure("x", new XReadFailure(429)), "provider_unavailable");
  assert.equal(connectionProviderFailure("instagram", new NativeReadTransportError("request_failed")), "network_unreachable");
  assert.equal(connectionProviderFailure("instagram", new InstagramLoginResponseError(401, "login_required")), "provider_unavailable");
  assert.equal(connectionProviderFailure("instagram", new InstagramLoginSessionError()), "credential_rejected");
  const local = new Error("owner_changed"); assert.throws(() => connectionProviderFailure("x", local), error => error === local);
});
