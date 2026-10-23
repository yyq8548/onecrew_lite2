import test from "node:test";
import assert from "node:assert/strict";
import { randomBytes, randomUUID } from "node:crypto";
import { mkdir, readdir, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import { createEmbeddedTables } from "onecrew_lite_tables/embedded";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { UnifiedAccountService } from "../src/identity/account-service.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { readStorageSql } from "../src/storage/sql-resource.ts";
import { PlatformVaultReader } from "../src/platforms/browser/platform-vault.ts";
import { OwnerPlatformVaultStore } from "../src/platforms/browser/owner-vault-store.ts";
import { OperatorBrowserConnections } from "../src/platforms/browser/operator-browser-connection.ts";
import { TelegramConnectionRuntime } from "../src/platforms/browser/telegram-connection.ts";
import { registerOperatorConnectionOperations, browserConnectionOperationIds } from "../src/platforms/browser/operator-connection-operations.ts";
import { OperationRegistry } from "../src/application/operation-registry.ts";

test("real signed-out Access owner cannot issue platform storage and every browser lifecycle validates before effects", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/", import.meta.url)); await mkdir(scratch, { recursive: true }); const root = await createPrivateDirectory(join(scratch, `op-${randomUUID().slice(0, 12)}`)), lease = await acquireInstanceLock(root);
  const vaultPath = join(root, "vault.sqlite"), keyPath = join(root, "masterkey"); await writeFile(vaultPath, "", { flag: "wx", mode: 0o600 }); const vault = new DatabaseSync(vaultPath); vault.exec(readStorageSql("vault-schema.sql") + ";PRAGMA user_version=4"); vault.close(); const key = randomBytes(32); await writeFile(keyPath, key, { flag: "wx", mode: 0o600 }); key.fill(0);
  const tables = await createEmbeddedTables(await createPrivateDirectory(join(root, "tables")), "http://127.0.0.1:31999"), identities = new UnifiedAccountService(vaultPath, keyPath, tables.tenants), databases = new AccountDatabases(lease, identities.contexts), store = new OwnerPlatformVaultStore(lease, identities.contexts, identities, databases, new PlatformVaultReader(vaultPath, keyPath)), browser = new OperatorBrowserConnections(identities.contexts, databases, store), telegram = new TelegramConnectionRuntime(identities.contexts, store), registry = new OperationRegistry(identities.contexts);
  try {
    registerOperatorConnectionOperations(registry, browser, telegram); for (const id of browserConnectionOperationIds) assert.ok(registry.names().includes(id));
    for (const platform of ["x", "instagram"] as const) {
      assert.throws(() => registry.validate(`${platform}.connection.reconnect.prepare`, {})); assert.throws(() => registry.validate(`${platform}.connection.reconnect.begin`, { flow_id: randomUUID(), confirmed: false, idempotency_key: "k" }));
      assert.throws(() => registry.validate(`${platform}.connection.browser.start`, { platform, profile_name: "", proxy: { kind: "http", host: "127.0.0.1", port: 1000 }, idempotency_key: "k" }));
      assert.throws(() => registry.validate(`${platform}.connection.browser.start`, { platform, profile_name: "local", proxy: { kind: "http", host: "127.0.0.1", port: 1000 }, idempotency_key: "k", x_chat_preference: "disabled" }));
      assert.throws(() => registry.validate(`${platform}.connection.reconnect.cancel`, { flow_id: randomUUID(), confirmed: true, extra: "forbidden" }));
    }
    const context = identities.contexts.activate({ user: { id: "local-context-without-remote-proof", email: "local@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" }); await databases.ensureNewAccount(context); const directory = await databases.accountDirectory(context), before = await readdir(directory.directory); let consumed = false;
    await assert.rejects(store.mutate(context, () => { consumed = true; })); assert.equal(consumed, false); assert.deepEqual(await readdir(directory.directory), before); assert.equal(identities.contexts.currentIfActive(), undefined);
  } finally { await browser.close(); await telegram.close(); databases.close(); identities.invalidate(); await tables.close(); await lease.close(); await rm(root, { recursive: true }); }
});
