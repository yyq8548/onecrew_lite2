import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { OpenCliReader } from "../platforms/opencli/reader.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";
import type { DiscoveryBrowserResolver, DiscoveryRepositoryResolver } from "./search-execution.ts";

export class DiscoverySourceAuthorExecutor {
  readonly accounts: AccountContextService; readonly repositories: DiscoveryRepositoryResolver; readonly browsers: DiscoveryBrowserResolver; readonly reader: OpenCliReader;
  constructor(accounts: AccountContextService, repositories: DiscoveryRepositoryResolver, browsers: DiscoveryBrowserResolver, reader: OpenCliReader) { this.accounts = accounts; this.repositories = repositories; this.browsers = browsers; this.reader = reader; }
  async readOnce(context: AccountContext, input: { campaign_id: string; expected_revision: number; search_hit_id: string }) {
    this.accounts.assertCurrent(context); const owned = await this.repositories.resolve(context); owned.assertOwned(); this.accounts.assertCurrent(context);
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_source_author_repository_owner_mismatch");
    const campaign = owned.repository.getPlan(input.campaign_id), claim = owned.repository.claimSourceAuthor(input.campaign_id, input.expected_revision, input.search_hit_id), timings: { content_ms: number | null; profile_ms: number | null } = { content_ms: null, profile_ms: null };
    const observed = await (async () => {
      this.accounts.assertCurrent(context); owned.assertOwned();
      if (campaign.opencli_version !== "1.8.8" || this.reader.version !== campaign.opencli_version) throw new OpenCliError("opencli_version_changed");
      const browser = await this.browsers.resolve(context, claim.profile_ref); await browser.assertCurrent(); if (browser.profile !== claim.profile_ref) throw new Error("discovery_browser_profile_mismatch");
      let started = performance.now();
      const contentResult = await this.reader.douyinOriginalContent(browser, claim.post_id).then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown })); timings.content_ms = Math.floor(performance.now() - started);
      if (!contentResult.ok) throw contentResult.error;
      const saved = owned.repository.savedSourceAuthorProfile(claim, contentResult.value.author_id);
      if (saved) return { profile: saved, content: contentResult.value };
      // 第二个原生读取仍须通过当前账号验证；两个观察只作为同一次逻辑claim提交。
      this.accounts.assertCurrent(context); owned.assertOwned(); await browser.assertCurrent(); started = performance.now();
      const profileResult = await this.reader.douyinPublicProfile(browser, contentResult.value.author_id).then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown })); timings.profile_ms = Math.floor(performance.now() - started);
      if (!profileResult.ok) throw profileResult.error; return { profile: profileResult.value, content: contentResult.value };
    })().then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
    owned.assertOwned();
    if (observed.ok) return { read: owned.repository.finishSourceAuthor(claim, observed.value.profile, observed.value.content, null), timings };
    if (observed.error instanceof OpenCliError && observed.error.code !== "opencli_cancelled") return { read: owned.repository.finishSourceAuthor(claim, null, null, observed.error.code === "opencli_read_command_unavailable" ? "opencli_command_unavailable" : observed.error.code), timings };
    const interrupted = await Promise.resolve().then(() => owned.repository.interruptSourceAuthor(claim)).then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown }));
    if (!interrupted.ok) throw new AggregateError([observed.error, interrupted.error], "discovery_source_author_read_and_interrupt_failed"); throw observed.error;
  }
}
