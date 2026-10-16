import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { OpenCliReader, VerifiedOpenCliBrowserBinding } from "../platforms/opencli/reader.ts";
import { creatorSearchSource } from "../platforms/opencli/sources.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";
import type { DiscoveryRepository } from "./repository.ts";
import type { DiscoverySearchRecovery } from "./stops.ts";

export interface OwnedDiscoveryRepository {
  readonly owner: Readonly<{ accountId: string; tenantId: string; baseId: string }>;
  readonly repository: DiscoveryRepository;
  assertOwned(): void;
}
export interface DiscoveryRepositoryResolver { resolve(context: AccountContext): Promise<OwnedDiscoveryRepository> }
export interface DiscoveryBrowserResolver { resolve(context: AccountContext, bindingId: string): Promise<VerifiedOpenCliBrowserBinding> }
export class DiscoverySearchExecutor {
  readonly #accounts: AccountContextService;
  readonly #repositories: DiscoveryRepositoryResolver;
  readonly #browsers: DiscoveryBrowserResolver;
  readonly #reader: OpenCliReader;
  constructor(accounts: AccountContextService, repositories: DiscoveryRepositoryResolver, browsers: DiscoveryBrowserResolver, reader: OpenCliReader) { this.#accounts = accounts; this.#repositories = repositories; this.#browsers = browsers; this.#reader = reader; }
  async searchOnce(context: AccountContext, input: Readonly<{ planId: string; revision: number; platform: string; profileRef: string; limit: number; explicitRecovery?: boolean; recovery?: DiscoverySearchRecovery | null }>) {
    this.#accounts.assertCurrent(context); const source = creatorSearchSource(input.platform);
    if (!source.discovery || !Number.isInteger(input.limit) || input.limit < 1 || input.limit > source.maximum) throw new Error("discovery_search_input_invalid");
    const owned = await this.#repositories.resolve(context); owned.assertOwned(); this.#accounts.assertCurrent(context);
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("Discovery repository belongs to another account");
    const plan = owned.repository.getPlan(input.planId);
    if (plan.opencli_version !== "1.8.8") throw new OpenCliError("opencli_version_changed");
    const browser = await this.#browsers.resolve(context, input.profileRef); await browser.assertCurrent(); this.#accounts.assertCurrent(context); owned.assertOwned();
    if (browser.profile !== input.profileRef) throw new Error("discovery_browser_profile_mismatch");
    const claim = owned.repository.claimSearch(input.planId, input.revision, input.platform, input.profileRef, input.explicitRecovery ?? false, input.recovery ?? null);
    const result = await this.#reader.creatorSearch(browser, input.platform, claim.query, input.limit).then(hits => ({ hits }), error => ({ error }));
    owned.assertOwned();
    if ("error" in result) {
      if (context.signal.aborted || browser.signal.aborted || result.error instanceof OpenCliError && result.error.code === "opencli_cancelled") { owned.repository.interruptSearch(claim); throw result.error; }
      if (!(result.error instanceof OpenCliError)) { owned.repository.interruptSearch(claim); throw result.error; }
      const code = result.error.code === "opencli_read_command_unavailable" ? "opencli_command_unavailable" : result.error.code;
      owned.repository.finishSearch(claim, code === "opencli_risk_control" ? "risk_control_stop" : code === "opencli_auth_required" ? "unauthorized" : "technical_stop", code, []);
      throw result.error;
    }
    const commit = await Promise.resolve().then(() => owned.repository.finishSearch(claim, "completed", null, result.hits)).then(() => ({ ok: true as const }), error => ({ ok: false as const, error }));
    if (!commit.ok) {
      const stopped = await Promise.resolve().then(() => owned.repository.finishSearch(claim, "technical_stop", "search_completion_failed", [])).then(() => ({ ok: true as const }), error => ({ ok: false as const, error }));
      if (!stopped.ok) throw new AggregateError([commit.error, stopped.error], "Discovery result and failure receipt could not be committed");
      throw commit.error;
    }
    this.#accounts.assertCurrent(context); await browser.assertCurrent();
    return { search_claim: claim, returned_hits: result.hits.length, returned_sources: result.hits, plan: owned.repository.getPlan(input.planId) };
  }
}
