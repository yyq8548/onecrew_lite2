import assert from "node:assert/strict";
import { randomBytes, randomUUID } from "node:crypto";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { join, resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { writeStringSession } from "@mtcute/core/utils.js";
import { assertTelegramSecretStep, validateTelegramConnectionStart, telegramConnectionFingerprint } from "../src/platforms/browser/telegram-connection-contracts.ts";
import { persistTelegramConnection } from "../src/platforms/browser/telegram-connection-persistence.ts";
import { importGrammersSession } from "../src/platforms/candidates/telegram-read.ts";
import { decryptVaultEnvelope } from "../src/storage/vault-reader.ts";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { readStorageSql } from "../src/storage/sql-resource.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import type { OwnerPlatformWriteScope } from "../src/platforms/browser/owner-vault-store.ts";

const input = { api_id: 12345, api_hash: "api-hash-secret", proxy: { kind: "socks5", host: "proxy.example", port: 1080, username: "proxy-user-secret", password: "proxy-password-secret" }, idempotency_key: "telegram-qr-default" };
test("原Telegram直接输入默认QR，E164与SOCKS5错误在连接前拒绝且秘密不进入错误", () => {
  const parsed = validateTelegramConnectionStart(input); assert.equal(parsed.method, "qr");
  for (const raw of [{ ...input, proxy: { ...input.proxy, kind: "https" } }, { ...input, method: "phone", phone: "+852 9123 4567" }, { ...input, api_id: 0 }]) {
    assert.throws(() => validateTelegramConnectionStart(raw), error => { assert.ok(error instanceof Error); for (const value of ["api-hash-secret", "proxy-user-secret", "proxy-password-secret"]) assert.equal(error.message.includes(value), false); return true; });
  }
  assert.equal(telegramConnectionFingerprint(parsed), telegramConnectionFingerprint(validateTelegramConnectionStart({ ...input, method: "qr", phone: null })));
  assert.notEqual(telegramConnectionFingerprint(parsed), telegramConnectionFingerprint(validateTelegramConnectionStart({ ...input, api_hash: "changed" })));
});
test("原OTP与2FA操作严格绑定当前步骤，重复领取/跨步骤输入在MTProto调用前拒绝", () => {
  const phone = { phase: "awaiting_secret_in_app", method: "telegram_phone", hasClient: true, hasInput: true, hasCode: true, hasPasswordWaiter: false };
  assertTelegramSecretStep(phone, false); assert.throws(() => assertTelegramSecretStep(phone, true), /not awaiting Telegram 2FA/);
  const password = { ...phone, hasCode: false }; assertTelegramSecretStep(password, true); assert.throws(() => assertTelegramSecretStep(password, false), /not awaiting a Telegram OTP/);
  assert.throws(() => assertTelegramSecretStep({ ...phone, phase: "starting" }, false), /operation is in progress/);
  assertTelegramSecretStep({ ...password, method: "telegram_qr", hasPasswordWaiter: true }, true); assert.throws(() => assertTelegramSecretStep({ ...password, method: "telegram_qr" }, true), /not awaiting Telegram 2FA/);
});
test("真实双SQLite事务保存Telegram密文、稳定账号与成熟session；Creator拒写共同回滚", async () => {
  await mkdir(resolve(".scratch"), { recursive: true }); const root = await mkdtemp(resolve(".scratch", "telegram-persist-")), directory = await createPrivateDirectory(join(root, "private")), creatorPath = join(directory, "creator.sqlite");
  await initializeNewCreatorDatabase(creatorPath); const creator = new DatabaseSync(creatorPath), vault = new DatabaseSync(join(directory, "vault.sqlite")), key = randomBytes(32), authKey = randomBytes(256);
  vault.exec(readStorageSql("vault-schema.sql"));
  const scope: OwnerPlatformWriteScope = { creator, vault, key, manifest: { format: 2, owner: { accountId: "owner", tenantId: "tenant", baseId: "base" }, deviceId: "device", profiles: [], services: [], storage: { kind: "account_vault", directory: `platform-vault-${randomUUID()}` } } };
  const session = writeStringSession({ version: 3, authKey, primaryDcs: { main: { id: 2, ipAddress: "149.154.167.51", port: 443 }, media: { id: 2, ipAddress: "149.154.167.51", port: 443 } } });
  const account = { id: "stable-tg-id", username: "original_name", first_name: "Test", last_name: null, is_bot: false, verified: false }, parsed = validateTelegramConnectionStart(input);
  try {
    assert.throws(() => persistTelegramConnection(scope, parsed, account, session, "Original label"), /requires_shared_transaction/);
    creator.exec("BEGIN IMMEDIATE"); vault.exec("BEGIN IMMEDIATE"); const first = persistTelegramConnection(scope, parsed, account, session, "Original label"); creator.exec("COMMIT"); vault.exec("COMMIT");
    const original = vault.prepare("SELECT * FROM profiles WHERE id=?").get(first.operator_account_id)!;
    const clear = decryptVaultEnvelope(key, original.encrypted_secret as Uint8Array); const secret = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(clear)); clear.fill(0);
    const restored = importGrammersSession(secret.telegram_session); assert.deepEqual(Buffer.from(restored.session.authKey), authKey); restored.session.authKey.fill(0);
    assert.equal(creator.prepare("SELECT label FROM account_refs WHERE id=?").get(first.operator_account_id)?.label, "original_name");
    creator.exec("CREATE TRIGGER reject_telegram BEFORE UPDATE ON account_refs BEGIN SELECT RAISE(FAIL,'reject'); END; BEGIN IMMEDIATE"); vault.exec("BEGIN IMMEDIATE");
    assert.throws(() => persistTelegramConnection(scope, parsed, { ...account, username: "renamed_user" }, session, "Replacement label"), /reject/); creator.exec("ROLLBACK"); vault.exec("ROLLBACK");
    assert.deepEqual(vault.prepare("SELECT * FROM profiles WHERE id=?").get(first.operator_account_id), original);
    creator.exec("DROP TRIGGER reject_telegram; BEGIN IMMEDIATE"); vault.exec("BEGIN IMMEDIATE");
    const renamed = persistTelegramConnection(scope, parsed, { ...account, username: "renamed_user" }, session, "Replacement label", { accountRefId: first.operator_account_id, providerAccountId: account.id }); creator.exec("COMMIT"); vault.exec("COMMIT");
    assert.equal(renamed.operator_account_id, first.operator_account_id); assert.equal(scope.manifest.profiles.length, 1); assert.equal(renamed.username, "renamed_user");
    creator.exec("BEGIN IMMEDIATE"); vault.exec("BEGIN IMMEDIATE"); assert.throws(() => persistTelegramConnection(scope, parsed, { ...account, id: "another" }, session, "Wrong", { accountRefId: first.operator_account_id, providerAccountId: account.id }), /identity_mismatch/); creator.exec("ROLLBACK"); vault.exec("ROLLBACK");
  } finally { key.fill(0); authKey.fill(0); creator.close(); vault.close(); await rm(root, { recursive: true }); }
});
