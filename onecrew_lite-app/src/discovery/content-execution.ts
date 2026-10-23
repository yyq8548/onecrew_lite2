import { z } from "zod";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import type { OpenCliReader } from "../platforms/opencli/reader.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";
import { INACTIVE_CREATOR_SOURCES } from "../platforms/opencli/sources.ts";
import type { DiscoveryBrowserResolver, DiscoveryRepositoryResolver } from "./search-execution.ts";

export class DiscoveryContentExecutor {
  readonly accounts: AccountContextService; readonly repositories: DiscoveryRepositoryResolver; readonly browsers: DiscoveryBrowserResolver; readonly reader: OpenCliReader;
  constructor(accounts: AccountContextService, repositories: DiscoveryRepositoryResolver, browsers: DiscoveryBrowserResolver, reader: OpenCliReader) { this.accounts = accounts; this.repositories = repositories; this.browsers = browsers; this.reader = reader; }
  async readNativeOnce(context: AccountContext, input: { campaign_id: string; expected_revision: number; search_hit_id: string }) {
    this.accounts.assertCurrent(context); const owned = await this.repositories.resolve(context); owned.assertOwned(); this.accounts.assertCurrent(context);
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_content_repository_owner_mismatch");
    const experience = owned.repository.linkedinExperienceEligible(input.campaign_id,input.search_hit_id);
    const campaign = owned.repository.getPlan(input.campaign_id), claim = experience ? owned.repository.claimLinkedinExperience(input.campaign_id,input.expected_revision,input.search_hit_id) : owned.repository.claimContent(input.campaign_id, input.expected_revision, input.search_hit_id);
    const observed = await (async () => {
      if (INACTIVE_CREATOR_SOURCES.includes(claim.platform)) throw new OpenCliError("opencli_read_command_unavailable");
      this.accounts.assertCurrent(context); owned.assertOwned();
      if (campaign.opencli_version !== "1.8.8" || this.reader.version !== campaign.opencli_version) throw new OpenCliError("opencli_version_changed");
      const browser = await this.browsers.resolve(context, claim.profile_ref); await browser.assertCurrent();
      if (browser.profile !== claim.profile_ref) throw new Error("discovery_browser_profile_mismatch");
      this.accounts.assertCurrent(context); owned.assertOwned();
      let value: unknown;
      if (["linkedin", "facebook"].includes(claim.platform)) {
        const saved = owned.repository.getProfile(claim.campaign_id, claim.search_hit_id), native = z.record(z.string(), z.unknown()).parse(saved.native_profile);
        if (saved.status !== "completed" || saved.claim.expected_revision !== claim.expected_revision || saved.claim.profile_ref !== claim.profile_ref || saved.claim.author_id !== claim.author_id || saved.claim.call_number >= claim.call_number) throw new OpenCliError("opencli_invalid_output");
        if (claim.platform === "linkedin") {
          if (saved.stop_code !== null || saved.claim.platform !== claim.platform || saved.claim.author_url !== claim.source_url) throw new OpenCliError("opencli_invalid_output");
          const account = z.string().parse(native.account_id);
          if (experience && owned.repository.linkedinExperienceAccount(claim) !== account) throw new OpenCliError("opencli_invalid_output");
          if (experience) {
            const reused = owned.repository.reuseLinkedinExperience(claim);
            if (reused !== null) return { reused: true as const, receipt: reused };
          }
          value = experience ? await this.reader.linkedinCurrentExperience(browser,claim.author_id,account) : await this.reader.linkedinOriginalContent(browser, claim.author_id, account);
        } else value = native.adapter_version === "2.0.0" ? await this.reader.facebookNativeOriginal(browser, claim.author_id, z.string().parse(native.observed_profile_url), claim.source_url) : await this.reader.facebookReelContent(browser, claim.author_id, claim.source_url);
      } else if (claim.platform === "instagram") {
        const username = /^https:\/\/www\.instagram\.com\/([^/]+)\/$/u.exec(claim.source_url)?.[1]; if (!username) throw new OpenCliError("opencli_invalid_input");
        const query = campaign.scopes.find(scope => scope.platform === "instagram")?.keywords.join(" ") ?? "";
        const saved = owned.repository.getProfile(claim.campaign_id, claim.search_hit_id);
        if (saved.status !== "completed" || saved.stop_code !== null || saved.claim.platform !== "instagram" || saved.claim.expected_revision !== claim.expected_revision || saved.claim.profile_ref !== claim.profile_ref || saved.claim.author_id !== claim.author_id || saved.claim.call_number >= claim.call_number) throw new OpenCliError("opencli_invalid_output");
        const previews = z.array(z.object({ url: z.string(), preview: z.string() })).max(6).parse(saved.native_profile?.visible_posts ?? []);
        const terms = [...new Set(query.normalize("NFKC").toLowerCase().match(/[\p{L}\p{N}]{3,}/gu) ?? [])].filter(term => !["blogger", "creator", "influencer", "account", "official"].includes(term));
        const score = (post: typeof previews[number]) => terms.filter(term => post.preview.normalize("NFKC").toLowerCase().includes(term)).length;
        const selected = previews.reduce<typeof previews[number] | undefined>((best, post) => !best || score(post) > score(best) ? post : best, undefined);
        const posts = await this.reader.instagramRecentOriginalPosts(browser, username, claim.author_id, query, selected?.url);
        value = posts?.[0]; if (value === undefined) throw new OpenCliError("opencli_invalid_output");
      } else if (claim.platform === "twitter") {
        const post = /\/status\/([1-9][0-9]{0,19})\/?$/u.exec(claim.source_url)?.[1]; if (!post) throw new OpenCliError("opencli_invalid_input");
        value = await this.reader.twitterPostContent(browser, claim.author_id, post, owned.repository.contentNativeIdentity(claim));
      } else if (claim.platform === "bilibili") value = await this.reader.bilibiliVideoEvidence(browser, claim.source_url, claim.author_id);
      else if (claim.platform === "weibo") value = await this.reader.weiboPostEvidence(browser, claim.source_url);
      else throw new OpenCliError("opencli_read_command_unavailable");
      if (Buffer.byteLength(stringifyApplicationJson(value)) > 16000) throw new OpenCliError("opencli_output_too_large"); return { reused: false as const, native: value };
    })().then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
    owned.assertOwned();
    if (observed.ok) return observed.value.reused ? observed.value.receipt : experience ? owned.repository.finishLinkedinExperience(claim,observed.value.native,null) : owned.repository.finishContent(claim, observed.value.native, null);
    if (observed.error instanceof OpenCliError && !["opencli_cancelled","opencli_cancel_pending"].includes(observed.error.code)) {
      const code = observed.error.code === "opencli_read_command_unavailable" ? "opencli_command_unavailable" : observed.error.code;
      return experience ? owned.repository.finishLinkedinExperience(claim,null,code) : owned.repository.finishContent(claim,null,code);
    }
    const interrupted = await Promise.resolve().then(() => experience ? owned.repository.finishLinkedinExperience(claim,null,"interrupted_experience_read") : owned.repository.interruptContent(claim)).then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown }));
    if (!interrupted.ok) throw new AggregateError([observed.error, interrupted.error], "discovery_content_read_and_interrupt_failed"); throw observed.error;
  }
}
