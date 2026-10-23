import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { OpenCliReader } from "../platforms/opencli/reader.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";
import type { DiscoveryBrowserResolver, DiscoveryRepositoryResolver } from "./search-execution.ts";

export interface DiscoveryFacebookExecutionInput { campaign_id: string; expected_revision: number; search_hit_id: string }
export class DiscoveryFacebookExecutor {
  readonly accounts: AccountContextService; readonly repositories: DiscoveryRepositoryResolver; readonly browsers: DiscoveryBrowserResolver; readonly reader: OpenCliReader;
  constructor(accounts: AccountContextService, repositories: DiscoveryRepositoryResolver, browsers: DiscoveryBrowserResolver, reader: OpenCliReader) { this.accounts = accounts; this.repositories = repositories; this.browsers = browsers; this.reader = reader; }
  async #owned(context: AccountContext) {
    this.accounts.assertCurrent(context); const owned = await this.repositories.resolve(context); owned.assertOwned(); this.accounts.assertCurrent(context);
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_facebook_repository_owner_mismatch");
    return owned;
  }
  async readIdentityOnce(context: AccountContext, input: DiscoveryFacebookExecutionInput) {
    const owned = await this.#owned(context), campaign = owned.repository.getPlan(input.campaign_id);
    const claim = owned.repository.claimFacebookIdentity(input.campaign_id, input.expected_revision, input.search_hit_id);
    const observed = await (async () => {
      if (this.reader.version !== "1.8.8" || campaign.opencli_version !== this.reader.version) throw new OpenCliError("opencli_version_changed");
      const browser = await this.browsers.resolve(context, claim.profile_ref); await browser.assertCurrent();
      if (browser.profile !== claim.profile_ref) throw new Error("discovery_browser_profile_mismatch");
      this.accounts.assertCurrent(context); owned.assertOwned();
      return this.reader.facebookSourceIdentity(browser, claim.source_url);
    })().then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
    owned.assertOwned();
    if (observed.ok) return owned.repository.finishFacebookIdentity(claim, observed.value, null);
    if (observed.error instanceof OpenCliError && observed.error.code !== "opencli_cancelled") return owned.repository.finishFacebookIdentity(claim, null, observed.error.code === "opencli_read_command_unavailable" ? "opencli_command_unavailable" : observed.error.code);
    const interrupted = await Promise.resolve().then(() => owned.repository.interruptFacebookIdentity(claim)).then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown }));
    if (!interrupted.ok) throw new AggregateError([observed.error, interrupted.error], "discovery_facebook_identity_read_and_interrupt_failed");
    throw observed.error;
  }
  async readTargetsOnce(context: AccountContext, input: DiscoveryFacebookExecutionInput) {
    const owned = await this.#owned(context), campaign = owned.repository.getPlan(input.campaign_id);
    if (this.reader.version !== "1.8.8" || campaign.opencli_version !== this.reader.version) throw new OpenCliError("opencli_version_changed");
    const claim = owned.repository.claimFacebookTargets(input.campaign_id, input.expected_revision, input.search_hit_id);
    const observed = await (async () => {
      const browser = await this.browsers.resolve(context, claim.profile_ref); await browser.assertCurrent();
      if (browser.profile !== claim.profile_ref) throw new Error("discovery_browser_profile_mismatch");
      this.accounts.assertCurrent(context); owned.assertOwned();
      return claim.source_url.startsWith("https://www.facebook.com/profile.php?") ? this.reader.facebookRecentLanguageUrls(browser, claim.author_id) : this.reader.facebookNativeTargets(browser, claim.author_id, claim.source_url);
    })().then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
    owned.assertOwned();
    if (observed.ok) return owned.repository.finishFacebookTargets(claim, observed.value, null);
    if (observed.error instanceof OpenCliError && observed.error.code !== "opencli_cancelled") return owned.repository.finishFacebookTargets(claim, null, observed.error.code === "opencli_read_command_unavailable" ? "opencli_command_unavailable" : observed.error.code);
    const interrupted = await Promise.resolve().then(() => owned.repository.interruptFacebookTargets(claim)).then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown }));
    if (!interrupted.ok) throw new AggregateError([observed.error, interrupted.error], "discovery_facebook_targets_read_and_interrupt_failed");
    throw observed.error;
  }
}
