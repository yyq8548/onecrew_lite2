import { randomUUID } from "node:crypto";
import type { AccountContext, AccountContextService } from "../../identity/account-context.ts";
import { verifyPrivateDirectory } from "../../storage/private-backup.ts";
import { validateOpenCliProfile } from "../opencli/registry.ts";
import type { TrustedOpenCliProcess } from "../opencli/process.ts";
import type { AccountBrowserBindingResolver, BrowserAccountDirectoryResolver, VerifiedOwnedBrowserProfile } from "./binding.ts";
import { prepareManagedExtensionChallenge, type ManagedExtensionChallenge } from "./extension-challenge.ts";
import { ManagedBrowserOwnership } from "./extension-ownership.ts";
import { createOwnedChromeProfile, launchOwnedChrome, type OwnedChromeProcess } from "./chrome-host.ts";
import { managedBrowserConnection } from "@onecrew/runtime-contracts/browser-connection";
import { prepareManagedChromeProfilesDirectory } from "./profile-directory.ts";
import { readdir, lstat } from "node:fs/promises";
import { join } from "node:path";
import { verifyAccountOwnedChromeDirectory } from "./binding.ts";

interface Flow {
  id: string; context: AccountContext; created: number; phase: "ready_to_begin" | "starting" | "awaiting_user_in_app" | "finalizing" | "completed" | "canceled" | "failed";
  controller: AbortController; abort: () => void; browser?: OwnedChromeProcess; challenge?: ManagedExtensionChallenge; pending?: Promise<unknown>; binding?: Readonly<{ bindingId: string; contextId: string }>;
  resume?: VerifiedOwnedBrowserProfile;
}
export interface ManagedOpenCliDependencies { installation: TrustedOpenCliProcess; configurationDirectory: string }
export class ManagedBrowserConnections {
  readonly #accounts: AccountContextService;
  readonly #directories: BrowserAccountDirectoryResolver;
  readonly #ownership: ManagedBrowserOwnership;
  readonly #bindings: AccountBrowserBindingResolver;
  readonly #origin: string;
  readonly #dependencies: (context: AccountContext) => Promise<ManagedOpenCliDependencies>;
  readonly #flows = new Map<string, Flow>();
  readonly #cleanupFailures: unknown[] = [];
  #closed = false;
  constructor(accounts: AccountContextService, directories: BrowserAccountDirectoryResolver, ownership: ManagedBrowserOwnership, bindings: AccountBrowserBindingResolver, origin: string, dependencies: (context: AccountContext) => Promise<ManagedOpenCliDependencies>) {
    this.#accounts = accounts; this.#directories = directories; this.#ownership = ownership; this.#bindings = bindings; this.#origin = origin; this.#dependencies = dependencies;
  }
  prepare(context: AccountContext) {
    this.#accounts.assertCurrent(context); if (this.#closed) throw new Error("browser.connections_closed");
    if (this.#flows.size >= 128) throw new Error("browser.connection_limit");
    const controller = new AbortController(), id = randomUUID(), abort = () => { controller.abort(); void this.cancel(context, id).catch(error => { this.#cleanupFailures.push(error); }); };
    const flow: Flow = { id, context, created: performance.now(), phase: "ready_to_begin", controller, abort };
    this.#flows.set(id, flow); context.signal.addEventListener("abort", abort, { once: true }); return this.#view(flow);
  }
  async prepareReopen(context: AccountContext, bindingId: string) {
    const profile = await this.#bindings.resolveOwnedProfile(context, bindingId); await profile.assertCurrent();
    const view = this.prepare(context), flow = this.#get(context, view.flow_id); flow.resume = profile; return view;
  }
  status(context: AccountContext, id: string) { return this.#view(this.#get(context, id)); }
  async begin(context: AccountContext, id: string, confirmed: boolean) {
    const flow = this.#get(context, id);
    if (!confirmed) throw new Error("browser.connection_confirmation_required");
    if (flow.phase !== "ready_to_begin") throw new Error("browser.connection_state_invalid");
    flow.phase = "starting";
    const operation = (async () => {
      const account = await this.#directories.accountDirectory(context), challenge = await prepareManagedExtensionChallenge(this.#origin, flow.controller.signal);
      account.assertOwned(); this.#accounts.assertCurrent(context); flow.controller.signal.throwIfAborted();
      if (flow.resume) await flow.resume.assertCurrent();
      const profiles = await prepareManagedChromeProfilesDirectory(account.directory);
      const bound = await this.#bindings.listOwned(context);
      const boundDirectories = new Set<string>();
      for (const item of bound) boundDirectories.add((await this.#bindings.resolveOwnedProfile(context, item.binding_id)).directory);
      const pending: string[] = [];
      if (!flow.resume) for (const entry of await readdir(profiles, { withFileTypes: true })) {
        if (!entry.isDirectory() || entry.isSymbolicLink()) throw new Error("browser.profile_directory_invalid");
        const candidate = await verifyAccountOwnedChromeDirectory(account.directory, join(profiles, entry.name));
        if (boundDirectories.has(candidate.path)) continue;
        // 安装目录只用于恢复未完成的专用窗口；登记仍必须通过真实扩展随机码验证。
        const installed = await lstat(join(candidate.path, "Default", "Extensions", "ildkmabpimmkaediidaifkhjpohdnifk")).then(value => value.isDirectory() && !value.isSymbolicLink(), error => { if (error.code === "ENOENT") return false; throw error; });
        if (installed) pending.push(candidate.path);
      }
      if (pending.length > 1) throw new Error("browser.pending_profile_ambiguous");
      const directory = flow.resume?.directory ?? pending[0] ?? await createOwnedChromeProfile(profiles);
      flow.browser = await launchOwnedChrome({ profileDirectory: directory, url: challenge.url, signal: flow.controller.signal });
      flow.challenge = challenge; this.#accounts.assertCurrent(context); flow.controller.signal.throwIfAborted(); flow.phase = "awaiting_user_in_app";
    })();
    flow.pending = operation;
    try { await operation; return this.#view(flow); } catch (error) { flow.phase = flow.controller.signal.aborted ? "canceled" : "failed"; if (flow.browser) await flow.browser.close().catch(cleanup => { throw new AggregateError([error, cleanup], "browser.connection_cleanup_failed"); }); throw error; } finally { delete flow.pending; }
  }
  async finalize(context: AccountContext, id: string, contextId: string, confirmed: boolean) {
    const flow = this.#get(context, id); validateOpenCliProfile(contextId);
    if (!confirmed) throw new Error("browser.connection_confirmation_required");
    if (flow.phase !== "awaiting_user_in_app" || !flow.challenge || !flow.browser) throw new Error("browser.connection_state_invalid");
    if (flow.resume && flow.resume.profile !== contextId) throw new Error("browser.reopen_context_mismatch");
    const challenge = flow.challenge, browser = flow.browser; flow.phase = "finalizing";
    let retryable = false;
    const operation = (async () => {
      const dependency = await this.#dependencies(context); await verifyPrivateDirectory(dependency.configurationDirectory);
      if (flow.resume) await flow.resume.assertCurrent();
      const proof = await challenge.verify({ process: browser, ...dependency, contextId, signal: flow.controller.signal }).catch(error => {
        if (!flow.controller.signal.aborted) { browser.assertRunning(); retryable = true; }
        throw error;
      });
      this.#accounts.assertCurrent(context); flow.controller.signal.throwIfAborted();
      flow.binding = await this.#ownership.register(context, proof); flow.phase = "completed";
    })();
    flow.pending = operation;
    try { await operation; return this.#view(flow); } catch (error) { if (retryable && !flow.controller.signal.aborted) flow.phase = "awaiting_user_in_app"; else { flow.phase = flow.controller.signal.aborted ? "canceled" : "failed"; await browser.close().catch(cleanup => { throw new AggregateError([error, cleanup], "browser.connection_cleanup_failed"); }); } throw error; } finally { delete flow.pending; }
  }
  async cancel(context: AccountContext, id: string) {
    const flow = this.#get(context, id, true); flow.controller.abort();
    if (flow.pending) await flow.pending.then(() => {}, () => {});
    if (flow.browser) await flow.browser.close();
    flow.phase = flow.binding ? "completed" : "canceled"; context.signal.removeEventListener("abort", flow.abort); return this.#view(flow);
  }
  async close(): Promise<void> {
    this.#closed = true;
    const failures: unknown[] = [...this.#cleanupFailures];
    for (const flow of this.#flows.values()) {
      await this.cancel(flow.context, flow.id).then(() => {}, error => { failures.push(error); });
    }
    if (failures.length) throw new AggregateError(failures, "browser.connection_cleanup_failed");
  }
  #get(context: AccountContext, id: string, cleanup = false): Flow {
    const flow = this.#flows.get(id);
    if (!flow || flow.context !== context) throw new Error("browser.connection_owner_mismatch");
    if (!cleanup) { this.#accounts.assertCurrent(context); if (this.#closed || performance.now() - flow.created > 30 * 60_000) throw new Error("browser.connection_expired"); }
    return flow;
  }
  #view(flow: Flow) { return managedBrowserConnection.parse({ flow_id: flow.id, phase: flow.phase, ...(flow.binding ? { binding_id: flow.binding.bindingId, context_id: flow.binding.contextId } : {}) }); }
}
