import { createHash } from "node:crypto";
import { isAbsolute, relative, sep } from "node:path";
import { inspectLibraryMigrationCopy } from "onecrew_lite_tables/migration-copy";
import { z } from "zod";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import type { InstanceLease } from "../service/instance-lock.ts";
import { verifyPrivateDirectory } from "../storage/private-backup.ts";
import type { LegacyAccountSnapshot } from "./legacy-account-selection.ts";
import { parseLegacyLibraryCredential } from "./legacy-library-credential.ts";

const accountRow = z.object({ access_user_id: z.string().min(1), credential: z.unknown().nullable(), is_active: z.boolean(), created_at: z.string(), updated_at: z.string() }).strict();
const tenantRow = z.object({ id: z.string().min(1), owner_user_id: z.string().min(1), space_id: z.string().min(1), base_id: z.string().min(1) }).strict();

// 只读取主迁移器私有目录中的隔离副本；不激活账号，也不重新归属任何表格。
export async function readLibraryMigrationAccount(lease: InstanceLease, copyDirectory: string) {
  lease.assertOwned();
  const directory = await verifyPrivateDirectory(copyDirectory);
  const child = relative(lease.directory, directory);
  if (!child || child === ".." || child.startsWith(`..${sep}`) || isAbsolute(child)) throw new Error("Library migration inspection requires an isolated child directory");
  await verifyPrivateDirectory(`${directory}${sep}pglite`);
  let credentialToRelease: ReturnType<typeof parseLegacyLibraryCredential> | undefined;
  let completed = false;
  using release = { [Symbol.dispose]() { if (!completed) credentialToRelease?.dispose(); } };
  const result = await inspectLibraryMigrationCopy(directory, value => {
    lease.assertOwned();
    const accounts = z.array(accountRow).parse(value.accounts), tenants = z.array(tenantRow).parse(value.tenants);
    if (new Set(accounts.map(row => row.access_user_id)).size !== accounts.length || new Set(tenants.map(row => row.owner_user_id)).size !== tenants.length) throw new Error("Library migration ownership rows are ambiguous");
    const active = accounts.filter(row => row.is_active);
    if (active.length > 1) throw new Error("Library migration has multiple active accounts");
    const sourceFingerprint = createHash("sha256").update(stringifyApplicationJson({ accounts, tenants })).digest("hex");
    const knownAccountIds = Object.freeze(accounts.map(row => row.access_user_id));
    if (!active.length) return { snapshot: { source: "library", state: "missing" } as LegacyAccountSnapshot, sourceFingerprint, knownAccountIds, tenants, credential: undefined };
    const row = active[0]!;
    if (row.credential === null) throw new Error("Active Library account has no credential");
    const credential = parseLegacyLibraryCredential(row.credential, row.access_user_id);
    credentialToRelease = credential;
    const snapshot: LegacyAccountSnapshot = { source: "library", state: credential.metadata.status === "active" ? "present" : "inactive", userId: row.access_user_id, fingerprint: credential.fingerprint };
    return { snapshot, sourceFingerprint, knownAccountIds, tenants, credential };
  });
  lease.assertOwned();
  completed = true;
  return result;
}
