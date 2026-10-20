import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";
import type { OpenCliReader } from "../platforms/opencli/reader.ts";
import type { DiscoveryBrowserResolver, DiscoveryRepositoryResolver } from "./search-execution.ts";
import { discoveryWebSourceTarget } from "./web-source-contracts.ts";
import type { DiscoveryContactAudit } from "./public-page-audit.ts";

export class DiscoveryWebSourceExecutor {
  readonly accounts: AccountContextService; readonly repositories: DiscoveryRepositoryResolver; readonly browsers: DiscoveryBrowserResolver; readonly reader: OpenCliReader; readonly audit: DiscoveryContactAudit | undefined;
  constructor(accounts: AccountContextService, repositories: DiscoveryRepositoryResolver, browsers: DiscoveryBrowserResolver, reader: OpenCliReader, audit?: DiscoveryContactAudit) { this.accounts = accounts; this.repositories = repositories; this.browsers = browsers; this.reader = reader; this.audit = audit; }
  async readOnce(context: AccountContext, input: { campaign_id: string; expected_revision: number; search_hit_id: string }) {
    this.accounts.assertCurrent(context); const owned = await this.repositories.resolve(context); owned.assertOwned(); this.accounts.assertCurrent(context);
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_web_source_repository_owner_mismatch");
    const campaign = owned.repository.getPlan(input.campaign_id), source = owned.repository.webSourceSource(input.campaign_id, input.search_hit_id), target = discoveryWebSourceTarget(source.source_url);
    if (!target) throw new OpenCliError("opencli_invalid_input");
    if (this.reader.version !== "1.8.8" || campaign.opencli_version !== this.reader.version) throw new OpenCliError("opencli_version_changed");
    const browser = await this.browsers.resolve(context, source.profile_ref); await browser.assertCurrent();
    if (browser.profile !== source.profile_ref) throw new Error("discovery_browser_profile_mismatch");
    if (target.platform === "youtube") await this.reader.checkYoutubeEvidenceProfile(browser); else await this.reader.checkBrowserProfile(browser);
    this.accounts.assertCurrent(context); owned.assertOwned();
    const claim = owned.repository.claimWebSource(input.campaign_id, input.expected_revision, input.search_hit_id), started = performance.now();
    const timings: Record<"identity_ms" | "profile_ms" | "content_ms", number | null> = { identity_ms: null, profile_ms: null, content_ms: null };
    let issued = 0;
    const timed = async <T>(stage: keyof typeof timings, work: () => Promise<T>): Promise<T> => {
      const begin = performance.now(); issued++;
      return work().then(value => { timings[stage] = Math.floor(performance.now() - begin); return value; }, error => { timings[stage] = Math.floor(performance.now() - begin); throw error; });
    };
    const observed = await (async () => {
      if (claim.profile_ref !== browser.profile || claim.source_url !== source.source_url || claim.target_platform !== target.platform || claim.video_id !== target.id) throw new OpenCliError("opencli_invalid_output");
      if (claim.target_platform === "tiktok") {
        const profile = await timed("identity_ms", () => this.reader.tiktokProfileIdentity(browser, claim.video_id));
        return { identity: profile, profile };
      }
      if (claim.target_platform === "instagram") {
        const profile = await timed("profile_ms", () => this.reader.instagramProfileIdentity(browser, claim.video_id));
        return { identity: profile, profile };
      }
      if (claim.target_platform === "douyin") {
        const content = await timed("content_ms", () => this.reader.douyinOriginalContent(browser, claim.video_id));
        const profile = await timed("profile_ms", () => this.reader.douyinPublicProfile(browser, content.author_id));
        return { profile, content };
      }
      if (claim.target_platform === "twitter") {
        const handle = target.handle;
        if (!handle) throw new OpenCliError("opencli_invalid_output");
        const identity = await timed("identity_ms", () => this.reader.twitterPostIdentity(browser, handle, claim.video_id));
        const profile = await timed("profile_ms", () => this.reader.twitterIdentityProfile(browser, handle));
        if (identity.account_id !== profile.account_id) throw new OpenCliError("opencli_invalid_output");
        const content = await timed("content_ms", () => this.reader.twitterPostContent(browser, handle, claim.video_id, identity.account_id));
        return { identity, profile, content };
      }
      return timed("content_ms", () => this.reader.youtubeVideoEvidence(browser, claim.video_id));
    })().then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
    owned.assertOwned();
    if (!observed.ok && (!(observed.error instanceof OpenCliError) || ["opencli_cancelled", "opencli_cancel_pending"].includes(observed.error.code))) throw observed.error;
    const stop = observed.ok ? null : (observed.error as OpenCliError).code === "opencli_read_command_unavailable" ? "opencli_command_unavailable" : (observed.error as OpenCliError).code;
    const receipt = owned.repository.finishWebSource(claim, observed.ok ? observed.value : null, stop);
    await this.audit?.record(claim.campaign_id, { kind: "web_source", call_number: claim.call_number, search_hit_id: claim.search_hit_id, origin_platform: claim.origin_platform, target_platform: claim.target_platform, reserved_native_calls: target.cost, issued_native_commands: issued, native_timings: timings, elapsed_ms: Math.floor(performance.now() - started), provider_status: receipt.status === "completed" ? "completed" : "failed", stop_code: receipt.stop_code, native_identity_observed: receipt.native_author_id !== null, contact_or_recommendation_promoted: false });
    return receipt;
  }
}
