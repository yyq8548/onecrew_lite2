import test from "node:test";
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { mkdir, rm } from "node:fs/promises";
import { resolve, join } from "node:path";
import { AccountContextService } from "../src/identity/account-context.ts";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { UnifiedAccountService } from "../src/identity/account-service.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { OwnerPlatformVaultStore } from "../src/platforms/browser/owner-vault-store.ts";
import { PlatformVaultReader } from "../src/platforms/browser/platform-vault.ts";
import { pendingYoutubeCredentialDeletes } from "../src/platforms/browser/youtube-credentials-delete.ts";
import { OperatorDisconnectRepository } from "../src/platforms/browser/operator-disconnect-repository.ts";

test("recovery scans the real owned Creator database without opening platform secrets and rejects expired account contexts", async () => {
  await mkdir(resolve(".scratch"), { recursive: true });
  const root = await createPrivateDirectory(resolve(".scratch", `recovery-read-${randomUUID()}`));
  const lease = await acquireInstanceLock(root), accounts = new AccountContextService();
  const databases = new AccountDatabases(lease, accounts);
  const context = accounts.activate({ user: { id: "recovery-owner", email: "recovery@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
  const identity = new UnifiedAccountService(join(root, "unopened-vault.sqlite"), join(root, "unopened-masterkey"), {
    resolve: async user => ({ tenantId: "tenant", baseId: "base", user }),
  });
  const store = new OwnerPlatformVaultStore(lease, accounts, identity, databases, new PlatformVaultReader(join(root, "unopened-vault.sqlite"), join(root, "unopened-masterkey")));
  try {
    await databases.ensureNewAccount(context);
    const read = () => store.readCreator(context, creator => ({ youtube: pendingYoutubeCredentialDeletes(creator), operators: new OperatorDisconnectRepository(creator).pending() }));
    assert.deepEqual(await read(), { youtube: [], operators: [] });
    const owned = await databases.platformStore(context), account = randomUUID();
    owned.database.prepare("INSERT INTO account_refs(id,platform,kind,label,active,availability,created_at,updated_at) VALUES(?,'x','profile','@operator',1,'available',1,1)").run(account);
    owned.database.exec("BEGIN IMMEDIATE");
    new OperatorDisconnectRepository(owned.database).begin("recover-existing", account);
    owned.database.exec("COMMIT");
    assert.deepEqual((await read()).operators, [{ key: "recover-existing", account_ref_id: account }]);
    accounts.logout();
    await assert.rejects(read(), /account changed/u);
  } finally {
    await databases.close(); await lease.close(); await rm(root, { recursive: true });
  }
});
