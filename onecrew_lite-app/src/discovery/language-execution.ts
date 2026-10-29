import { z } from "zod";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";
import type { OpenCliReader, VerifiedOpenCliBrowserBinding } from "../platforms/opencli/reader.ts";
import { INACTIVE_CREATOR_SOURCES } from "../platforms/opencli/sources.ts";
import type { DiscoveryBrowserResolver, DiscoveryRepositoryResolver } from "./search-execution.ts";
import { discoveryLanguageSample } from "./review-material.ts";
import { validDiscoveryLanguageSample, type DiscoveryLanguageClaim } from "./language-contracts.ts";
import type { DiscoveryLanguageRepository } from "./language-repository.ts";

const object = (value: unknown) => z.record(z.string(), z.unknown()).parse(value);
const text = z.string().refine(value => value.isWellFormed());
type Material = ReturnType<DiscoveryLanguageRepository["readContext"]>;
function bodyText(claim: DiscoveryLanguageClaim, material: Material) {
  const saved = material.body;
  if (!saved || saved.claim.expected_revision !== claim.revision || saved.claim.platform !== claim.platform || saved.claim.profile_ref !== claim.profile_ref || saved.claim.author_id !== claim.author_id || saved.claim.call_number >= claim.call_number || saved.status !== "completed" || saved.stop_code !== null) throw new OpenCliError("opencli_invalid_output");
  const native = object(saved.native_content);
  const source = ["instagram", "linkedin"].includes(claim.platform) ? native.source_url : saved.claim.source_url;
  if (source !== claim.root_url) throw new OpenCliError("opencli_invalid_output");
  return { native, text: text.parse(native.text) };
}

export class DiscoveryLanguageExecutor {
  readonly accounts: AccountContextService;
  readonly repositories: DiscoveryRepositoryResolver;
  readonly browsers: DiscoveryBrowserResolver;
  readonly reader: OpenCliReader;
  constructor(accounts: AccountContextService, repositories: DiscoveryRepositoryResolver, browsers: DiscoveryBrowserResolver, reader: OpenCliReader) { this.accounts = accounts; this.repositories = repositories; this.browsers = browsers; this.reader = reader; }
  async readOnce(context: AccountContext, input: { campaign_id: string; expected_revision: number; search_hit_id: string }) {
    this.accounts.assertCurrent(context);
    const owned = await this.repositories.resolve(context); owned.assertOwned(); this.accounts.assertCurrent(context);
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_language_repository_owner_mismatch");
    const campaign = owned.repository.getPlan(input.campaign_id);
    if (campaign.revision !== input.expected_revision || campaign.phase !== "active") throw new Error("discovery_language_approval_changed");
    if (!owned.repository.languageEligible(input.campaign_id, input.search_hit_id)) return null;
    const source = owned.repository.languageSource(input.campaign_id, input.search_hit_id);
    if (INACTIVE_CREATOR_SOURCES.includes(source.platform)) throw new OpenCliError("opencli_read_command_unavailable");
    if (campaign.opencli_version !== "1.8.8" || this.reader.version !== campaign.opencli_version) throw new OpenCliError("opencli_version_changed");
    const browser = await this.browsers.resolve(context, source.profile_ref); await browser.assertCurrent();
    if (browser.profile !== source.profile_ref) throw new Error("discovery_browser_profile_mismatch");
    owned.assertOwned(); this.accounts.assertCurrent(context);
    const claim = owned.repository.claimLanguage(input.campaign_id, input.expected_revision, input.search_hit_id);
    if (claim === null) return null;
    const result = await (async () => {
      if (claim.profile_ref !== browser.profile || claim.platform !== source.platform) throw new OpenCliError("opencli_invalid_output");
      if (claim.ordinal !== 1 || claim.previous_urls.length > 0) return { sample: null, stop: "language_sources_exhausted" };
      const material = owned.repository.languageContext(claim);
      const value = object(await this.#read(browser, claim, material, typeof campaign.criteria.language === "string" && campaign.criteria.language.trim().length > 0));
      const sample = discoveryLanguageSample.safeParse({ ...value, comments: z.array(z.unknown()).parse(value.comments).slice(0, 5) });
      if (!sample.success || !validDiscoveryLanguageSample(sample.data, claim)) throw new OpenCliError("opencli_invalid_output");
      return { sample: sample.data, stop: null };
    })().then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error instanceof z.ZodError ? new OpenCliError("opencli_invalid_output") : error as unknown }));
    owned.assertOwned();
    if (!result.ok && (!(result.error instanceof OpenCliError) || result.error.code === "opencli_cancelled")) throw result.error;
    const value = result.ok ? result.value : { sample: null, stop: (result.error as OpenCliError).code === "opencli_read_command_unavailable" ? "opencli_command_unavailable" : (result.error as OpenCliError).code };
    owned.repository.finishLanguage(claim, value.sample, value.stop);
    return { claim: { campaign_id: claim.campaign_id, expected_revision: claim.revision, search_hit_id: claim.search_hit_id, platform: claim.platform, profile_ref: claim.profile_ref, author_id: claim.author_id, source_url: value.sample?.source_url ?? claim.root_url, call_number: claim.call_number }, status: value.sample === null ? "failed" : "completed", native_content: null, language_sample: value.sample, stop_code: value.stop, captured_at: Math.floor(Date.now() / 1000) };
  }
  async #read(browser: VerifiedOpenCliBrowserBinding, claim: DiscoveryLanguageClaim, material: Material, includeComments: boolean): Promise<unknown> {
    const source = claim.root_url, id = source.split("/").at(-1)!;
    if (claim.platform === "douyin") {
      const saved = bodyText(claim, material), joint = material.joint;
      if (!joint || joint.status !== "completed" || joint.stop_code !== null || joint.duplicate_of !== null || joint.expected_revision !== claim.revision || joint.profile_ref !== claim.profile_ref || joint.author_id !== claim.author_id || joint.call_number !== material.body!.claim.call_number || joint.source_url !== source) throw new OpenCliError("opencli_invalid_output");
      const window = await this.reader.douyinOriginalComments(browser, text.parse(joint.post_id), claim.author_id);
      if (saved.text !== window.text || saved.native.text_truncated !== window.text_truncated) throw new OpenCliError("opencli_invalid_output");
      return { author_id: claim.author_id, source_url: source, text: window.text, comments: window.comments.map(comment => ({ source_url: `${source}#comment-${comment.id}`, text: comment.text })) };
    }
    if (claim.platform === "linkedin") {
      const body = material.body, profile = material.profile, canonical = `https://www.linkedin.com/in/${claim.author_handle}/`;
      if (!body || !profile || body.status !== "completed" || body.stop_code !== null || profile.status !== "completed" || profile.stop_code !== null || body.claim.expected_revision !== claim.revision || profile.claim.expected_revision !== claim.revision || body.claim.platform !== claim.platform || profile.claim.platform !== claim.platform || body.claim.profile_ref !== claim.profile_ref || profile.claim.profile_ref !== claim.profile_ref || body.claim.author_id !== claim.author_handle || profile.claim.author_id !== claim.author_handle || body.claim.source_url !== canonical || profile.claim.source_url !== canonical || profile.claim.call_number >= body.claim.call_number || body.claim.call_number >= claim.call_number) throw new OpenCliError("opencli_invalid_output");
      const native = object(body.native_content), identity = object(profile.native_profile);
      if (native.source_url !== source || native.account_id !== claim.author_id || identity.account_id !== claim.author_id) throw new OpenCliError("opencli_invalid_output");
      const comments = await this.reader.linkedinOriginalComments(browser, claim.author_handle, claim.author_id, source);
      return { author_id: claim.author_id, source_url: source, text: text.parse(native.text), comments: comments.map(comment => ({ source_url: `${source}#comment-${comment.id}`, text: comment.text })) };
    }
    if (["instagram", "xiaohongshu"].includes(claim.platform)) {
      const saved = bodyText(claim, material);
      return { author_id: claim.author_id, source_url: source, text: saved.text, comments: [] };
    }
    if (claim.platform === "weibo") {
      const saved = bodyText(claim, material), comments = await this.reader.weiboOriginalComments(browser, text.parse(saved.native.post_id));
      return { author_id: claim.author_id, source_url: source, text: saved.text, comments: comments.map(comment => ({ source_url: `${source}#comment-${comment.id}`, text: comment.text })) };
    }
    if (claim.platform === "facebook") return this.reader.facebookLanguageSample(browser, claim.author_id, source);
    if (claim.platform === "tiktok") return this.reader.tiktokLanguageSample(browser, claim.author_handle, claim.author_id, source, includeComments);
    if (claim.platform === "bilibili") return this.reader.bilibiliLanguageSample(browser, claim.author_id, source);
    if (claim.platform === "google" || claim.platform === "yahoo") {
      const web = material.web;
      if (!web || web.status !== "completed" || !web.is_primary_source || web.expected_revision !== claim.revision || web.origin_platform !== claim.platform || web.profile_ref !== claim.profile_ref || web.native_author_id !== claim.author_id || web.candidate_id !== claim.candidate_id || web.call_number >= claim.call_number || source !== (web.target_platform === "youtube" ? `https://www.youtube.com/watch?v=${web.video_id}` : web.source_url)) throw new OpenCliError("opencli_invalid_output");
      if (web.target_platform === "youtube") return this.reader.youtubeLanguageSample(browser, claim.author_id, web.video_id);
      if (web.target_platform === "twitter") {
        const handle = /^https:\/\/x\.com\/([A-Za-z0-9_]{1,15})\/status\/([1-9][0-9]{0,19})$/u.exec(source);
        if (!handle || handle[1]!.toLowerCase() !== claim.author_handle) throw new OpenCliError("opencli_invalid_output");
        return this.reader.twitterLanguageSample(browser, claim.author_handle, handle[2]!, claim.author_id);
      }
      if (claim.author_handle !== claim.author_id) throw new OpenCliError("opencli_invalid_output");
      const window = await this.reader.douyinOriginalComments(browser, web.video_id, claim.author_id), original = object(object(web.native).content);
      if (original.text !== window.text || original.text_truncated !== window.text_truncated) throw new OpenCliError("opencli_invalid_output");
      return { author_id: claim.author_id, source_url: source, text: window.text, comments: window.comments.map(comment => ({ source_url: `${source}#comment-${comment.id}`, text: comment.text })) };
    }
    if (claim.platform === "youtube") return this.reader.youtubeLanguageSample(browser, claim.author_id, source);
    if (claim.platform === "twitter") return this.reader.twitterLanguageSample(browser, claim.author_handle, id, claim.author_id);
    throw new OpenCliError("opencli_read_command_unavailable");
  }
}
