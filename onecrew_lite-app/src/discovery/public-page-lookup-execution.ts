import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { OpenCliReader } from "../platforms/opencli/reader.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";
import { publicWebUrl } from "../platforms/opencli/public-url.ts";
import type { DiscoveryBrowserResolver, DiscoveryRepositoryResolver } from "./search-execution.ts";
import { discoveryContactLeadMatches, discoveryContactLeadPriority } from "./contact-navigation.ts";
import { discoveryContactLookup, type DiscoveryContactLookup } from "./public-page-lookup.ts";
import type { DiscoveryContactAudit } from "./public-page-audit.ts";

export class DiscoveryContactLookupExecutor {
  readonly accounts: AccountContextService; readonly repositories: DiscoveryRepositoryResolver; readonly browsers: DiscoveryBrowserResolver; readonly reader: OpenCliReader;
  readonly audit: DiscoveryContactAudit | undefined;
  constructor(accounts: AccountContextService, repositories: DiscoveryRepositoryResolver, browsers: DiscoveryBrowserResolver, reader: OpenCliReader, audit?: DiscoveryContactAudit) { this.accounts = accounts; this.repositories = repositories; this.browsers = browsers; this.reader = reader; this.audit = audit; }
  async readOnce(context: AccountContext, raw: DiscoveryContactLookup): Promise<DiscoveryContactLookup> {
    const input = discoveryContactLookup.parse(raw);
    // 原运行入口已停用 Maigret 执行，历史收据和独立 Google 搜索仍保留。
    if (input.method !== "search") throw new Error("contact_lookup_retired");
    this.accounts.assertCurrent(context);
    const owned = await this.repositories.resolve(context); owned.assertOwned(); this.accounts.assertCurrent(context);
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_contact_lookup_repository_owner_mismatch");
    const browser = await this.browsers.resolve(context, input.profile_ref); await browser.assertCurrent();
    if (browser.profile !== input.profile_ref) throw new Error("discovery_browser_profile_mismatch");
    this.accounts.assertCurrent(context); owned.assertOwned();
    const claim = owned.repository.claimContactLookup(input);
    const started = performance.now(); let rawResults = 0, keptLinks = 0;
    const read = async () => {
      if (this.reader.version !== "1.8.8") throw new OpenCliError("opencli_version_changed");
      const hits = await this.reader.creatorSearch(browser, "google", claim.query, 5);
      rawResults = hits.length;
      hits.sort((left, right) => discoveryContactLeadPriority(claim.author_label, claim.username, left.source_url, left.title) - discoveryContactLeadPriority(claim.author_label, claim.username, right.source_url, right.title));
      const links = hits.filter(hit => discoveryContactLeadMatches(claim.author_label, claim.username, hit.source_url, hit.title)).flatMap(hit => publicWebUrl(hit.source_url) !== null ? [hit.source_url] : []);
      keptLinks = links.length; return links;
    };
    const observed = await read().then(links => ({ ok: true as const, links }), error => ({ ok: false as const, error: error as unknown }));
    owned.assertOwned();
    const complete = async (completion: DiscoveryContactLookup) => {
      const receipt = owned.repository.finishContactLookup(completion);
      await this.audit?.record(input.campaign_id, { kind: "lookup", candidate_id: input.candidate_id, call_number: claim.call_number, elapsed_ms: Math.floor(performance.now() - started), method: "search", raw_results: rawResults, kept_links: keptLinks, provider_status: completion.status === "completed" ? "completed" : "failed", stop_code: completion.stop_code });
      return receipt;
    };
    if (observed.ok) return complete({ ...claim, status: "completed", links: observed.links });
    if (!(observed.error instanceof OpenCliError)) throw observed.error;
    if (observed.error.code === "opencli_cancelled") throw observed.error;
    return complete({ ...claim, status: "failed", links: [], stop_code: observed.error.code === "opencli_read_command_unavailable" ? "opencli_command_unavailable" : observed.error.code });
  }
}
