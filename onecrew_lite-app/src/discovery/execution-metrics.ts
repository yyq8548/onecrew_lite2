import { createHash, randomUUID } from "node:crypto";
import { lstat, open, readFile, rename } from "node:fs/promises";
import { join } from "node:path";
import { z } from "zod";
import { creatorU64 } from "@onecrew/runtime-contracts/creators";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { createPrivateDirectory, verifyPrivateDirectory, verifyPrivateFile } from "../storage/private-backup.ts";

const text = z.string(), u32 = z.number().int().min(0).max(4294967295);
export const discoveryReadFailure = z.strictObject({ recorded_at: creatorU64, step: text, platform: text.nullable(), target_id: text.nullable(), call_number: u32.nullable(), code: text,
  diagnostic: z.strictObject({ endpoint: text.refine(value => ["/installations/{id}", "/creator-sync/credential", "/me"].includes(value)), elapsed_ms: u32.optional(), http_status: z.number().int().min(100).max(599).optional(), transport_code: text.regex(/^[A-Za-z][A-Za-z0-9_]{0,63}$/u).optional() }).optional() });
export type DiscoveryReadFailure = z.infer<typeof discoveryReadFailure>;
const phase = z.strictObject({ calls: creatorU64, elapsed_ms: creatorU64 });
const metricsSchema = z.object({ campaign_id: text, phases: z.record(text, phase), failures: z.array(discoveryReadFailure).default([]), failure_count: creatorU64.default(0) });
const traceSchema = z.strictObject({ phase: text, start_ms: creatorU64, end_ms: creatorU64, relative_start_ms: creatorU64, relative_end_ms: creatorU64, process_id: u32, target_hash: text.nullable() });
type Trace = z.infer<typeof traceSchema>;
type Pending = { campaign: string; kind: "duration"; event: Trace } | { campaign: string; kind: "failure"; event: DiscoveryReadFailure };
const maximum = 18446744073709551615n, epoch = performance.now();
const saturating = (value: number | bigint, added: number | bigint) => { const result = BigInt(value) + BigInt(added); return result > maximum ? maximum : result; };
const writers = new Map<string, Promise<void>>();
const folders = new Map<string, Promise<string>>();
const missing = (error: unknown) => (error as NodeJS.ErrnoException).code === "ENOENT";

// 每个实例绑定真实账号私有目录；只保存聚合耗时及分类错误，不保存页面或凭据。
export class FileDiscoveryExecutionMetrics {
  readonly #directory: string; readonly #assertOwned: () => void; #pending: Pending[] = [];
  constructor(directory: string, assertOwned: () => void) { this.#directory = directory; this.#assertOwned = assertOwned; }
  enqueue(campaign: string, phase: string, elapsedMs: number | bigint, target: string | null = null): void {
    const elapsed = BigInt(creatorU64.parse(elapsedMs)), end = BigInt(Date.now()), relative = BigInt(Math.floor(performance.now() - epoch));
    const event = traceSchema.parse({ phase, start_ms: end > elapsed ? end - elapsed : 0n, end_ms: end, relative_start_ms: relative > elapsed ? relative - elapsed : 0n, relative_end_ms: relative, process_id: process.pid, target_hash: target === null ? null : createHash("sha256").update(`${campaign}:${target}`).digest("hex") });
    this.#pending.push({ campaign, kind: "duration", event });
  }
  enqueueFailure(campaign: string, raw: DiscoveryReadFailure): void { this.#pending.push({ campaign, kind: "failure", event: discoveryReadFailure.parse(raw) }); }
  async #folder(create: boolean): Promise<string | null> {
    this.#assertOwned(); await verifyPrivateDirectory(this.#directory);
    const folder = join(this.#directory, "discovery-metrics"), info = await lstat(folder).then(value => value, error => { if (missing(error)) return null; throw error; });
    if (info === null) {
      if (!create) return null;
      let creation = folders.get(folder);
      if (!creation) { creation = createPrivateDirectory(folder); folders.set(folder, creation); }
      await creation; await verifyPrivateDirectory(folder);
    } else await verifyPrivateDirectory(folder);
    return folder;
  }
  #name(campaign: string): string { return createHash("sha256").update(campaign).digest("hex"); }
  async #load(folder: string, campaign: string) {
    const path = join(folder, `${this.#name(campaign)}.json`), exists = await lstat(path).then(() => true, error => { if (missing(error)) return false; throw error; });
    if (!exists) return metricsSchema.parse({ campaign_id: campaign, phases: {} });
    await verifyPrivateFile(path); const info = await lstat(path, { bigint: true });
    if (info.nlink !== 1n || info.size > 16n * 1024n * 1024n) throw new Error("discovery_metrics_file_invalid");
    const bytes = await readFile(path), after = await lstat(path, { bigint: true });
    if (info.dev !== after.dev || info.ino !== after.ino || info.mtimeNs !== after.mtimeNs || info.ctimeNs !== after.ctimeNs) throw new Error("discovery_metrics_file_changed");
    return metricsSchema.parse(parseApplicationJson(bytes));
  }
  async failureCount(campaign: string): Promise<number | bigint> {
    const folder = await this.#folder(false); if (folder === null) return 0;
    const metrics = await this.#load(folder, campaign); this.#assertOwned();
    return BigInt(metrics.failure_count) >= BigInt(metrics.failures.length) ? metrics.failure_count : metrics.failures.length;
  }
  async flushPending(): Promise<void> {
    const pending = this.#pending; this.#pending = [];
    const batches = new Map<string, Pending[]>();
    for (const item of pending) { const batch = batches.get(item.campaign) ?? []; batch.push(item); batches.set(item.campaign, batch); }
    const results = await Promise.allSettled([...batches].map(([campaign, events]) => {
      const key = JSON.stringify([this.#directory, campaign]), previous = writers.get(key) ?? Promise.resolve();
      const work = previous.then(async () => {
        const folder = await this.#folder(true); if (folder === null) throw new Error("discovery_metrics_directory_missing");
        const metrics = await this.#load(folder, campaign), durations: Trace[] = [];
        for (const item of events) {
          if (item.kind === "failure") {
            metrics.failure_count = saturating(BigInt(metrics.failure_count) > BigInt(metrics.failures.length) ? metrics.failure_count : metrics.failures.length, 1);
            if (metrics.failures.length === 200) metrics.failures.shift(); metrics.failures.push(item.event);
          } else {
            const current = Object.hasOwn(metrics.phases, item.event.phase) ? metrics.phases[item.event.phase]! : { calls: 0, elapsed_ms: 0 };
            Object.defineProperty(metrics.phases, item.event.phase, { configurable: true, enumerable: true, writable: true, value: { calls: saturating(current.calls, 1), elapsed_ms: saturating(current.elapsed_ms, BigInt(item.event.end_ms) - BigInt(item.event.start_ms)) } });
            durations.push(item.event);
          }
        }
        metrics.phases = Object.fromEntries(Object.entries(metrics.phases).sort(([a], [b]) => Buffer.compare(Buffer.from(a), Buffer.from(b))));
        const path = join(folder, `${this.#name(campaign)}.json`), temporary = `${path}.${randomUUID()}.pending`;
        this.#assertOwned();
        { await using file = await open(temporary, "wx", 0o600); await file.writeFile(stringifyApplicationJson(metrics)); await file.sync(); }
        this.#assertOwned(); await rename(temporary, path);
        if (durations.length) {
          const eventPath = join(folder, `${this.#name(campaign)}.events.jsonl`), before = await lstat(eventPath).then(value => value, error => { if (missing(error)) return null; throw error; });
          if (before && (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1)) throw new Error("discovery_metrics_event_file_invalid");
          await using file = await open(eventPath, "a", 0o600);
          const opened = await file.stat(); if (opened.nlink !== 1 || before && (before.dev !== opened.dev || before.ino !== opened.ino)) throw new Error("discovery_metrics_event_file_changed");
          this.#assertOwned(); await file.writeFile(durations.map(value => stringifyApplicationJson(value)).join("\n") + "\n");
        }
      });
      writers.set(key, work);
      return work.finally(() => { if (writers.get(key) === work) writers.delete(key); });
    }));
    const errors = results.filter((item): item is PromiseRejectedResult => item.status === "rejected").map(item => item.reason as unknown);
    if (errors.length) throw new AggregateError(errors, "discovery_metrics_flush_failed");
  }
}
