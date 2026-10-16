import { createHash } from "node:crypto";
import { inspect } from "node:util";
import { z } from "zod";
import type { AccessInstallationCredential } from "./access-verification.ts";
import { validateAccessBearer } from "./legacy-access-credential.ts";

const unsigned = z.number().int().nonnegative().safe();
const uuid = z.uuid().transform(value => value.toLowerCase());
const legacyLibraryCredentialSchema = z.object({
  bearer: z.string().min(80).max(165),
  installation: z.object({
    installation_id: uuid, credential_id: uuid, access_user_id: z.string().min(1).max(128),
    access_device_id: z.string().regex(/^dev_[A-Za-z0-9_-]{24}$/u),
    access_pairing_id: z.string().regex(/^pair_[A-Za-z0-9_-]{24}$/u),
    source_instance_id: uuid, client_platform: z.enum(["macos", "windows"]), app_version: z.string(),
    status: z.enum(["active", "revoked"]), revision: unsigned.min(1),
    created_at: unsigned, updated_at: unsigned, last_seen_at: z.number().finite().nullable(), revoked_at: z.number().finite().nullable(),
  }).strict(),
}).strict();

// 输入来自隔离 Library 副本的旧账号行。通过本地格式验证不代表在线有效。
export function parseLegacyLibraryCredential(value: unknown, storedUserId: string): LegacyLibraryCredential {
  const parsed = legacyLibraryCredentialSchema.parse(value);
  if (parsed.installation.access_user_id !== storedUserId) throw new Error("Stored Library credential identity mismatch");
  validateAccessBearer(parsed.installation.credential_id, parsed.bearer);
  return new LegacyLibraryCredential(parsed);
}

class LegacyLibraryCredential implements AccessInstallationCredential {
  readonly metadata: Readonly<z.infer<typeof legacyLibraryCredentialSchema>["installation"]>;
  readonly fingerprint: string;
  readonly #bearer: Buffer;
  #disposed = false;
  constructor(value: z.infer<typeof legacyLibraryCredentialSchema>) {
    this.metadata = Object.freeze(value.installation);
    this.fingerprint = createHash("sha256").update(JSON.stringify(value)).digest("hex");
    this.#bearer = Buffer.from(value.bearer);
    Object.freeze(this);
  }
  exposeBearer(): string {
    if (this.#disposed) throw new Error("Legacy Library credential is disposed");
    return this.#bearer.toString("utf8");
  }
  dispose(): void { this.#bearer.fill(0); this.#disposed = true; }
  toJSON(): never { throw new Error("Legacy Library credentials cannot be serialized"); }
  [inspect.custom](): string { return "LegacyLibraryCredential [redacted]"; }
}
