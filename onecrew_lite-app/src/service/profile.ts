import { createServer } from "node:net";
import { lstatSync } from "node:fs";
import { open, readFile, realpath } from "node:fs/promises";
import { join } from "node:path";
import { z } from "zod";
import { parseStoredJson } from "../contracts/stored-json.ts";
import { verifyPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";
import type { InstanceLease } from "./instance-lock.ts";

const profileSchema = z.object({
  format: z.literal(1),
  dataDirectory: z.string().min(1),
  port: z.number().int().min(1).max(65535),
  originSource: z.enum(["new-install", "legacy-upgrade"]),
}).strict();
export type ServiceProfile = Readonly<z.infer<typeof profileSchema>>;

// 只能在持有此目录实例锁时调用；旧端口由已验证的旧运行记录提供。
export async function loadOrCreateServiceProfile(lease: InstanceLease, legacyPort?: number): Promise<ServiceProfile> {
  lease.assertOwned();
  const directory = await realpath(await verifyPrivateDirectory(lease.directory));
  const path = join(directory, "profile.json");
  if (legacyPort !== undefined) z.number().int().min(1).max(65535).parse(legacyPort);
  if (lstatSync(path, { throwIfNoEntry: false })) {
    await verifyPrivateFile(path);
    const stored = profileSchema.parse(parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(await readFile(path)), true));
    if (stored.dataDirectory !== directory || (legacyPort !== undefined && stored.port !== legacyPort)) {
      throw new Error("Service profile identity conflicts with the data directory or legacy port");
    }
    return Object.freeze(stored);
  }
  const port = legacyPort ?? await allocateLoopbackPort();
  const profile: ServiceProfile = Object.freeze({ format: 1, dataDirectory: directory, port,
    originSource: legacyPort === undefined ? "new-install" : "legacy-upgrade" });
  const handle = await open(path, "wx", 0o600);
  try {
    lease.assertOwned();
    await verifyPrivateFile(path);
    await handle.writeFile(JSON.stringify(profile));
    await handle.sync();
  } finally {
    await handle.close();
  }
  return profile;
}

async function allocateLoopbackPort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => { server.off("error", reject); resolve(); });
  });
  const address = server.address();
  await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  if (address === null || typeof address === "string") throw new Error("Local port allocation did not bind TCP");
  return address.port;
}
