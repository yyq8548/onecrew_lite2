import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, open } from "node:fs/promises";
import { join } from "node:path";
import { z } from "zod";
import { createPrivateDirectory, verifyPrivateDirectory } from "../storage/private-backup.ts";
import { stringifyApplicationJson } from "../contracts/application-json.ts";

const u32 = z.number().int().min(0).max(4294967295), count = z.number().int().nonnegative().safe();
const base = { candidate_id: z.string(), call_number: u32, elapsed_ms: count, provider_status: z.enum(["completed", "failed"]), stop_code: z.string().nullable() };
export const discoveryContactMetric = z.discriminatedUnion("kind", [
  z.strictObject({ ...base, kind: z.literal("page") }),
  z.strictObject({ ...base, kind: z.literal("lookup"), method: z.literal("search"), raw_results: count, kept_links: count }),
  z.strictObject({ kind: z.literal("web_source"), call_number: u32, search_hit_id: z.string(), origin_platform: z.string(), target_platform: z.string(), reserved_native_calls: count, issued_native_commands: count, native_timings: z.strictObject({ identity_ms: count.nullable(), profile_ms: count.nullable(), content_ms: count.nullable() }), elapsed_ms: count, provider_status: z.enum(["completed", "failed"]), stop_code: z.string().nullable(), native_identity_observed: z.boolean(), contact_or_recommendation_promoted: z.literal(false) }),
]);
export type DiscoveryContactMetric = z.infer<typeof discoveryContactMetric>;
export interface DiscoveryContactAudit { record(campaign: string, event: DiscoveryContactMetric): Promise<void> }
const writers = new Map<string, Promise<void>>();
export class FileDiscoveryContactAudit implements DiscoveryContactAudit {
  readonly #directory: string; readonly #assertOwned: () => void;
  constructor(directory: string, assertOwned: () => void) { this.#directory = directory; this.#assertOwned = assertOwned; }
  record(campaign: string, raw: DiscoveryContactMetric): Promise<void> {
    const event = discoveryContactMetric.parse(raw), directory = join(this.#directory, "contact-enrichment-metrics");
    const prior = writers.get(directory) ?? Promise.resolve();
    const next = prior.then(async () => {
      this.#assertOwned(); await verifyPrivateDirectory(this.#directory);
      const info = await lstat(directory).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return null; throw error; });
      if (info === null) await createPrivateDirectory(directory); else await verifyPrivateDirectory(directory);
      const path = join(directory, `${createHash("sha256").update(campaign).digest("hex")}.jsonl`);
      this.#assertOwned();
      const before = await lstat(path, { bigint: true }).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return null; throw error; });
      if (before && (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1n)) throw new Error("contact_audit_file_invalid");
      await using file = await open(path, constants.O_WRONLY | constants.O_APPEND | constants.O_CREAT | (constants.O_NOFOLLOW ?? 0), 0o600);
      const opened = await file.stat({ bigint: true });
      if (!opened.isFile() || opened.nlink !== 1n || before && (before.dev !== opened.dev || before.ino !== opened.ino)) throw new Error("contact_audit_file_changed");
      this.#assertOwned();
      await file.writeFile(`${stringifyApplicationJson({ ...event, recorded_at: Math.floor(Date.now() / 1000) })}\n`);
    });
    writers.set(directory, next);
    return next.finally(() => { if (writers.get(directory) === next) writers.delete(directory); });
  }
}
