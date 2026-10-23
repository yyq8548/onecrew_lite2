import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import type { OpenCliReader } from "../platforms/opencli/reader.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";
import { facebookSourceValid } from "../platforms/opencli/facebook-profile.ts";
import { INACTIVE_CREATOR_SOURCES } from "../platforms/opencli/sources.ts";
import type { DiscoveryBrowserResolver, DiscoveryRepositoryResolver } from "./search-execution.ts";

export class DiscoveryProfileExecutor {
  readonly #accounts: AccountContextService; readonly #repositories: DiscoveryRepositoryResolver; readonly #browsers: DiscoveryBrowserResolver; readonly #reader: OpenCliReader;
  constructor(accounts: AccountContextService, repositories: DiscoveryRepositoryResolver, browsers: DiscoveryBrowserResolver, reader: OpenCliReader) { this.#accounts = accounts; this.#repositories = repositories; this.#browsers = browsers; this.#reader = reader; }
  async readOnce(context: AccountContext, input: { campaign_id: string; expected_revision: number; search_hit_id: string }) {
    this.#accounts.assertCurrent(context); const owned = await this.#repositories.resolve(context); owned.assertOwned(); this.#accounts.assertCurrent(context);
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_profile_repository_owner_mismatch");
    const campaign = owned.repository.getPlan(input.campaign_id);
    if (campaign.phase !== "active" || campaign.revision !== input.expected_revision) throw new Error("discovery_profile_not_approved");
    const pending = owned.repository.getProfileSource(input.campaign_id, input.search_hit_id);
    const browser = await this.#browsers.resolve(context, pending.profile_ref); await browser.assertCurrent();
    this.#accounts.assertCurrent(context); owned.assertOwned();
    const claim = owned.repository.claimProfile(input.campaign_id, input.expected_revision, input.search_hit_id);
    const observed = await (async () => {
      if (INACTIVE_CREATOR_SOURCES.includes(claim.platform)) throw new OpenCliError("opencli_read_command_unavailable");
      this.#accounts.assertCurrent(context); owned.assertOwned();
      if (this.#reader.version !== "1.8.8" || campaign.opencli_version !== this.#reader.version) throw new OpenCliError("opencli_version_changed");
      if (browser.profile !== claim.profile_ref) throw new Error("discovery_browser_profile_mismatch");
      this.#accounts.assertCurrent(context); owned.assertOwned();
      const reused = owned.repository.reuseProfile(claim);
      if (reused !== null) return { reused: true as const, receipt: reused };
      const value = claim.platform === "twitter" ? await this.#reader.twitterIdentityProfile(browser, claim.author_id) : claim.platform === "facebook" && facebookSourceValid(claim.source_url) ? await this.#reader.facebookNativeProfile(browser, claim.author_id, claim.source_url) : await this.#reader.creatorProfileEvidence(browser, claim.platform, claim.author_id, claim.author_url);
      if (Buffer.byteLength(stringifyApplicationJson(value)) > 16000) throw new OpenCliError("opencli_output_too_large");
      return { reused: false as const, native: value };
    })().then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
    // 已开始的读取始终提交原账号回执；账号切换只禁止下一次读取。
    owned.assertOwned();
    if (observed.ok) return observed.value.reused ? observed.value.receipt : owned.repository.finishProfile(claim, observed.value.native, null);
    if (observed.error instanceof OpenCliError && !["opencli_cancelled", "opencli_cancel_pending"].includes(observed.error.code)) return owned.repository.finishProfile(claim, null, observed.error.code === "opencli_read_command_unavailable" ? "opencli_command_unavailable" : observed.error.code);
    const interrupted = await Promise.resolve().then(() => owned.repository.interruptProfile(claim)).then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown }));
    if (!interrupted.ok) throw new AggregateError([observed.error, interrupted.error], "discovery_profile_read_and_interrupt_failed");
    throw observed.error;
  }
}
