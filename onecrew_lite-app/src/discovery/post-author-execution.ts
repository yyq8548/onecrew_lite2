import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { OpenCliReader } from "../platforms/opencli/reader.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";
import type { DiscoveryBrowserResolver, DiscoveryRepositoryResolver } from "./search-execution.ts";

export class DiscoveryPostAuthorExecutor {
  readonly accounts: AccountContextService; readonly repositories: DiscoveryRepositoryResolver; readonly browsers: DiscoveryBrowserResolver; readonly reader: OpenCliReader;
  constructor(accounts: AccountContextService, repositories: DiscoveryRepositoryResolver, browsers: DiscoveryBrowserResolver, reader: OpenCliReader) { this.accounts = accounts; this.repositories = repositories; this.browsers = browsers; this.reader = reader; }
  async readOnce(context: AccountContext, input: { campaign_id: string; expected_revision: number; search_hit_id: string }) {
    this.accounts.assertCurrent(context); const owned = await this.repositories.resolve(context); owned.assertOwned(); this.accounts.assertCurrent(context);
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_post_author_repository_owner_mismatch");
    const campaign = owned.repository.getPlan(input.campaign_id), claim = owned.repository.claimPostAuthor(input.campaign_id, input.expected_revision, input.search_hit_id);
    const observed = await (async () => {
      this.accounts.assertCurrent(context); owned.assertOwned();
      if (campaign.opencli_version !== "1.8.8" || this.reader.version !== campaign.opencli_version) throw new OpenCliError("opencli_version_changed");
      const browser = await this.browsers.resolve(context, claim.profile_ref); await browser.assertCurrent();
      if (browser.profile !== claim.profile_ref) throw new Error("discovery_browser_profile_mismatch");
      this.accounts.assertCurrent(context); owned.assertOwned(); return this.reader.twitterPostEvidence(browser, claim.author_handle, claim.post_id);
    })().then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
    owned.assertOwned();
    if (observed.ok) return owned.repository.finishPostAuthor(claim, { account_id: observed.value.content.identity.account_id, observed_source_url: observed.value.content.identity.source_url, stop_code: null, native_content: observed.value.content, native_profile: observed.value.profile });
    if (observed.error instanceof OpenCliError && observed.error.code !== "opencli_cancelled") return owned.repository.finishPostAuthor(claim, { account_id: null, observed_source_url: null, stop_code: observed.error.code === "opencli_read_command_unavailable" ? "opencli_command_unavailable" : observed.error.code });
    const interrupted = await Promise.resolve().then(() => owned.repository.interruptPostAuthor(claim)).then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown }));
    if (!interrupted.ok) throw new AggregateError([observed.error, interrupted.error], "discovery_post_author_read_and_interrupt_failed"); throw observed.error;
  }
}
