import { randomBytes, randomUUID } from "node:crypto";
import { join, relative, resolve } from "node:path";
import { z } from "zod";
import { unifiedResources } from "../application/unified-resources.ts";
import type { InstanceLease } from "./instance-lock.ts";
import { listenLocalServer } from "./http.ts";
import { loadOrCreateServiceProfile } from "./profile.ts";
import { verifyPrivateFile } from "../storage/private-backup.ts";
import { dataPathIdentity } from "./build-identity.ts";
import { parseApplicationJson } from "../contracts/application-json.ts";

const sha256 = z.string().regex(/^[a-f0-9]{64}$/u);
const healthSchema = z.object({ service: z.literal("onecrew_lite"), protocol_version: z.literal(1), instance_id: z.uuid(), migration_state: z.literal("development"), production_ready: z.literal(false), registered_operations: z.array(z.string()), web_assets_sha256: sha256, resource_identity: z.object({ kind: z.enum(["source_development", "sea_candidate"]), executable_sha256: sha256, source_tree_sha256: sha256, dependency_lock_sha256: sha256, data_paths_sha256: sha256, candidate: z.literal(true), profile_port: z.number().int().min(1).max(65535) }).strict() }).strict();
export type CandidateHealth = z.infer<typeof healthSchema>;

// 在原端口验收候选资源；不写正式发现文件、当前版本指针或旧数据。
export async function startCandidateRuntime(lease: InstanceLease, options: { generationRoot: string; webAssetsDirectory: string; libraryWebAssetsDirectory?: string }) {
  lease.assertOwned();
  await verifyPrivateFile(join(lease.directory, "profile.json"));
  const profile = await loadOrCreateServiceProfile(lease);
  const generationRoot = resolve(options.generationRoot), vaultPath = join(generationRoot, "vault", "vault.sqlite"), masterKeyPath = join(generationRoot, "masterkey");
  if (!relative(lease.directory, generationRoot)) throw new Error("Candidate generation must be separate from the current data root");
  const resources = await unifiedResources({ vaultPath, masterKeyPath, candidateGenerationRoot: generationRoot, webAssetsDirectory: options.webAssetsDirectory, ...(options.libraryWebAssetsDirectory ? { libraryWebAssetsDirectory: options.libraryWebAssetsDirectory } : {}) })(lease, profile);
  let opened = false;
  const controlToken = randomBytes(32).toString("hex"), instanceId = randomUUID();
  try {
    const server = await listenLocalServer({ port: profile.port, instanceId, controlToken, operations: resources.operations, ...(resources.browserSessions ? { browserSessions: resources.browserSessions } : {}), ...(resources.configureRoutes ? { configureRoutes: resources.configureRoutes } : {}), ...(resources.webAssets ? { webAssets: resources.webAssets } : {}), ...(resources.libraryWebAssets ? { libraryWebAssets: resources.libraryWebAssets } : {}), ...(resources.resourceIdentity ? { resourceIdentity: resources.resourceIdentity } : {}) });
    let closing: Promise<void> | undefined;
    const expectedPaths = dataPathIdentity({ dataDirectory: resolve(lease.directory), generationRoot, vaultPath, masterKeyPath, tablesDirectory: join(generationRoot, "tables") });
    opened = true;
    return {
      origin: server.origin,
      async health(): Promise<CandidateHealth> {
        lease.assertOwned();
        if (closing) throw new Error("Candidate runtime is stopping");
        const response = await fetch(`${server.origin}/_local/status`, { headers: { authorization: `Bearer ${controlToken}` }, redirect: "error", signal: AbortSignal.timeout(5000) });
        if (!response.ok) throw new Error("Candidate health readback failed");
        const bytes = new Uint8Array(await response.arrayBuffer());
        if (bytes.byteLength > 64 * 1024) throw new Error("Candidate health readback exceeded its limit");
        const health = healthSchema.parse(parseApplicationJson(bytes));
        if (health.instance_id !== instanceId || health.resource_identity.profile_port !== profile.port || health.resource_identity.data_paths_sha256 !== expectedPaths || health.web_assets_sha256 !== resources.webAssets?.fingerprint()) throw new Error("Candidate resource identity mismatch");
        lease.assertOwned();
        return health;
      },
      close(): Promise<void> {
        closing ??= (async () => { resources.stopAcceptingWork?.(); try { await server.close(); } finally { await resources.close?.(); } })();
        return closing;
      },
      toJSON(): never { throw new Error("Candidate runtime authority must not be serialized"); },
    };
  } finally { if (!opened) await resources.close?.(); }
}
