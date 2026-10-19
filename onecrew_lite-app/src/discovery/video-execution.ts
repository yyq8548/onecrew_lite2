import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { OpenCliReader } from "../platforms/opencli/reader.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";
import type { DiscoveryBrowserResolver, DiscoveryRepositoryResolver } from "./search-execution.ts";

// 描述中的链接仅作为待核实线索；读取页面仍须单独验证来源与预算。
export function discoveryDescriptionLinks(description: string): string[] {
  const links: string[] = [];
  for (const token of description.split(/\s+/u)) {
    if (token.includes("...") || token.includes("…")) continue;
    const value = token.replace(/^[()[\]<>,.;!"']+|[()[\]<>,.;!"']+$/gu, "");
    if (Buffer.byteLength(value) > 2048 || !value.startsWith("https://")) continue;
    const url = URL.parse(value);
    if (!url || !url.hostname || url.username || url.password || links.includes(value)) continue;
    links.push(value); if (links.length === 10) break;
  }
  return links;
}

export class DiscoveryVideoExecutor {
  readonly accounts: AccountContextService; readonly repositories: DiscoveryRepositoryResolver; readonly browsers: DiscoveryBrowserResolver; readonly reader: OpenCliReader;
  constructor(accounts: AccountContextService, repositories: DiscoveryRepositoryResolver, browsers: DiscoveryBrowserResolver, reader: OpenCliReader) { this.accounts = accounts; this.repositories = repositories; this.browsers = browsers; this.reader = reader; }
  async readOnce(context: AccountContext, input: { campaign_id: string; expected_revision: number; profile_ref: string; search_hit_id: string }) {
    this.accounts.assertCurrent(context); const owned = await this.repositories.resolve(context); owned.assertOwned(); this.accounts.assertCurrent(context);
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_video_repository_owner_mismatch");
    const campaign = owned.repository.getPlan(input.campaign_id);
    if (this.reader.version !== "1.8.8" || campaign.opencli_version !== this.reader.version) throw new OpenCliError("opencli_version_changed");
    const browser = await this.browsers.resolve(context, input.profile_ref); await browser.assertCurrent();
    if (browser.profile !== input.profile_ref) throw new Error("discovery_browser_profile_mismatch");
    this.accounts.assertCurrent(context); owned.assertOwned();
    const claim = owned.repository.claimVideo(input.campaign_id, input.expected_revision, input.profile_ref, input.search_hit_id);
    const observed = await this.reader.youtubeVideoEvidence(browser, claim.source_url).then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
    // 原账号的真实读取结果必须提交，账号切换只阻止后续工作。
    owned.assertOwned();
    if (observed.ok) {
      const video = observed.value;
      return owned.repository.finishVideo(claim, { campaign_id: claim.campaign_id, expected_revision: claim.expected_revision, platform: "youtube", source_profile_ref: claim.profile_ref, platform_account_id: video.channelId, display_name: video.author, profile_url: `https://www.youtube.com/channel/${video.channelId}`, evidence: { source_urls: [claim.source_url, video.sourceUrl], subscribers_raw: video.subscribers, subscribers_source_url: video.sourceUrl, captured_at: Math.floor(Date.now() / 1000), video_title: video.title, video_description: video.description, ...(video.observations ? { video_observations: video.observations } : {}), public_links: discoveryDescriptionLinks(video.description) } }, null);
    }
    if (observed.error instanceof OpenCliError && observed.error.code !== "opencli_cancelled") return owned.repository.finishVideo(claim, null, observed.error.code === "opencli_read_command_unavailable" ? "opencli_command_unavailable" : observed.error.code);
    const interrupted = await Promise.resolve().then(() => owned.repository.interruptVideo(claim)).then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown }));
    if (!interrupted.ok) throw new AggregateError([observed.error, interrupted.error], "discovery_video_read_and_interrupt_failed");
    throw observed.error;
  }
}
