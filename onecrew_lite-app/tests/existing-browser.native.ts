import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { copyFile, mkdir, rm } from "node:fs/promises";
import { join, resolve } from "node:path";
import { DatabaseSync, backup } from "node:sqlite";
import test from "node:test";
import { createEmbeddedTables } from "onecrew_lite_tables/embedded";
import { UnifiedAccountService } from "../src/identity/account-service.ts";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { AccessVaultStore } from "../src/identity/access-vault-store.ts";
import { AccountBrowserBindingResolver } from "../src/platforms/browser/binding.ts";
import { OpenCliAddons } from "../src/platforms/opencli/addons.ts";
import { OpenCliReader } from "../src/platforms/opencli/reader.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("existing Chrome grant uses a real Access identity and connected extension, and expires with the session", { timeout: 180_000 }, async () => {
  const source = process.env.ONECREW_BROWSER_TEST_DATA, addonsPath = process.env.ONECREW_BROWSER_TEST_ADDONS, home = process.env.ONECREW_BROWSER_TEST_OPENCLI_HOME;
  assert.ok(source && addonsPath && home, "Set actual local data and OpenCLI paths for native acceptance");
  await mkdir(resolve(".scratch"), { recursive: true });
  const root = await createPrivateDirectory(resolve(".scratch", `existing-browser-${randomUUID()}`));
  const lease = await acquireInstanceLock(root), vault = join(root, "vault.sqlite"), key = join(root, "masterkey");
  const database = new DatabaseSync(join(source, "vault.sqlite"), { readOnly: true });
  await backup(database, vault); database.close(); await copyFile(join(source, "masterkey"), key);
  const tableRoot = await createPrivateDirectory(join(root, "tables")), tables = await createEmbeddedTables(tableRoot, "http://127.0.0.1:62462");
  const identities = new UnifiedAccountService(vault, key, tables.tenants);
  identities.bindCredentialStore(new AccessVaultStore(lease, vault, key));
  const accounts = new AccountDatabases(lease, identities.contexts);
  try {
    const context = await identities.refresh(); await accounts.ensureNewAccount(context);
    const bindings = new AccountBrowserBindingResolver(identities.contexts, identities, accounts);
    const reader = await OpenCliReader.inspect(new OpenCliAddons(resolve(addonsPath), resolve(home)));
    const profiles = await reader.connectedProfiles(); assert.ok(profiles.length > 0, "A connected Chrome extension is required");
    const profile = profiles[0]!.context_id;
    assert.equal(await bindings.resolveExisting(context, profile), null);
    await bindings.connectExisting(context, profile, binding => reader.checkBrowserProfile(binding));
    const selected = await bindings.resolveProfileRef(context, profile);
    await reader.checkBrowserProfile(selected); await selected.assertCurrent();
    identities.invalidate();
    await assert.rejects(selected.assertCurrent(), { code: "access.account_changed" });
    const renewed = await identities.refresh();
    const restored = await bindings.resolveProfileRef(renewed, profile);
    await reader.checkBrowserProfile(restored);
    console.log(JSON.stringify({ extensionVerified: true, grantPersisted: true, staleSessionRejected: true, accountReverified: true }));
  } finally {
    identities.invalidate(); accounts.close(); await tables.close(); await lease.close(); await rm(root, { recursive: true });
  }
});
