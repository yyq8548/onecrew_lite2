import { setTimeout as delay } from "node:timers/promises";
import { lstat, readFile } from "node:fs/promises";
import { join } from "node:path";
import { parseStoredJson } from "../../contracts/stored-json.ts";
import { OpenCliError } from "./errors.ts";
import { OpenCliRegistry, parseConnectedOpenCliProfiles, parseOpenCliBridgeStatus, parseOpenCliJson, parseOpenCliVersion, validateOpenCliProfile } from "./registry.ts";
import { OPENCLI_PROBE_TIMEOUT, OPENCLI_READ_TIMEOUT, runTrustedOpenCliProcess } from "./process.ts";
import type { TrustedOpenCliProcess } from "./process.ts";
import { resolveOpenCliInstallation, VERIFIED_OPENCLI_VERSION } from "./installation.ts";
import { OpenCliAddons } from "./addons.ts";
import { ambiguousCreatorSearchEmpty, CREATOR_SEARCH_SOURCES, creatorSearchArguments, creatorSearchSource } from "./sources.ts";
import { decodeCreatorSearch } from "./search-results.ts";
import type { CreatorSearchHit } from "./search-results.ts";
import type { ReviewedOpenCliRead } from "./read-plan.ts";
import { youtubeChannelPlan, youtubeLanguagePlan, youtubeRecentLanguagePlan, youtubeSearchPlan, youtubeTranscriptPlan, youtubeVideoPlan } from "./youtube.ts";
import { youtubeChannelContactPlan, youtubeVideoEvidencePlan } from "./youtube-evidence.ts";
import { decodePublicWebRead } from "./web.ts";
import { checkPublicWebDestination, validatePublicWebUrl } from "./public-url.ts";
import { nativeCommentsPlan } from "./native-comments.ts";
import type { NativeOpenCliCommentSource } from "./native-comments.ts";
import { bilibiliProfilePlan, instagramProfilePlan, tiktokProfilePlan, twitterIdentityPlan, twitterProfilePlan } from "./profiles.ts";
import { bilibiliLanguagePlan, instagramCommentLanguagePlan, instagramRecentOriginalsPlan, tiktokLanguagePlan, tiktokRecentLanguagePlan } from "./language.ts";
import { twitterPostIdentityPlan, twitterPostContentPlan, twitterLanguagePlan, twitterRecentContentPlan } from "./twitter-posts.ts";
import { bilibiliVideoPlan, bilibiliVideoPagePlan } from "./bilibili-video.ts";
import { weiboProfilePlan, weiboPostPlan, weiboCommentsPlan } from "./weibo.ts";
import { pinterestProfilePlan, pinterestContentPlan } from "./pinterest.ts";
import { redditProfilePlan, redditRecentPlan, redditLanguagePlan, redditPostPlan } from "./reddit.ts";
import { douyinProfilePlan, douyinContentPlan, douyinCommentsPlan } from "./douyin.ts";
import { blueskyProfilePlan, blueskyPostPlan, blueskyUriFromSource } from "./bluesky.ts";
import { linkedinProfilePlan, linkedinContentPlan, linkedinExperiencePlan, linkedinContactPlan, linkedinCommentsPlan } from "./linkedin.ts";
import { facebookIdentityPlan, facebookProfilePlan, facebookContactPlan } from "./facebook-profile.ts";
import { facebookRecentPlan, facebookReelPlan, facebookLanguagePlan, facebookNativeOriginalPlan } from "./facebook-language.ts";
import { creatorProfileSource } from "./profile-source.ts";
import { instagramProfileIdentityPlan, tiktokProfileIdentityPlan } from "./profile-identity.ts";
import { creatorAuthArguments, creatorAuthProbe, decodeCreatorAuthStatus, decodeCreatorWhoami } from "./auth-status.ts";
import type { OpenCliAuthObservation } from "./auth-status.ts";
import { openCliLoginPage } from "./login-pages.ts";

// 由应用唯一的账号设置与 Chrome 连接映射提供。请求里的 profile 字段不是身份凭据。
export interface VerifiedOpenCliBrowserBinding {
  readonly profile: string;
  readonly signal: AbortSignal;
  assertCurrent(): Promise<void>;
}
// 实现方须核对已保存的 Creator/Search 证据和预算，不能把请求中的 URL 当作授权。
export interface VerifiedOpenCliPublicSource { readonly url: string; assertApproved(): Promise<void> }
interface ReaderConfiguration { readonly installation: TrustedOpenCliProcess; readonly addons: OpenCliAddons }
let recovery: Promise<unknown> = Promise.resolve();

export class OpenCliReader {
  readonly #installation: TrustedOpenCliProcess;
  readonly #addons: OpenCliAddons;
  readonly #registry: OpenCliRegistry;
  readonly version: string;
  private constructor(configuration: ReaderConfiguration, version: string, registry: OpenCliRegistry) {
    this.#installation = Object.freeze({ ...configuration.installation }); this.#addons = configuration.addons; this.version = version; this.#registry = registry;
  }
  static async inspect(addons: OpenCliAddons, installation?: TrustedOpenCliProcess, signal?: AbortSignal): Promise<OpenCliReader> {
    const selected = installation ?? await resolveOpenCliInstallation();
    const options = { deadlineMs: OPENCLI_PROBE_TIMEOUT, configurationDirectory: addons.configurationDirectory, ...(signal ? { signal } : {}) };
    const version = parseOpenCliVersion((await runTrustedOpenCliProcess(selected, ["--version"], options)).stdout);
    if (version !== VERIFIED_OPENCLI_VERSION) throw new OpenCliError("opencli_version_changed");
    const commands = (await runTrustedOpenCliProcess(selected, ["list", "-f", "json"], options)).stdout;
    const registry = new OpenCliRegistry(await parseOpenCliJson(commands));
    return new OpenCliReader({ installation: selected, addons }, version, registry);
  }
  inventory() { return this.#registry.inventory(this.version); }
  async openCreatorLoginPage(binding: VerifiedOpenCliBrowserBinding, platform: string) {
    const url = openCliLoginPage(platform); await this.checkBrowserProfile(binding);
    const receipt = await parseOpenCliJson((await this.#run(["--profile", binding.profile, "browser", "onecrew-login", "tab", "new", url], OPENCLI_READ_TIMEOUT, binding)).stdout);
    if (!receipt || typeof receipt !== "object" || !("url" in receipt) || receipt.url !== url || !("page" in receipt) || typeof receipt.page !== "string" || !receipt.page) throw new OpenCliError("opencli_invalid_output");
    await binding.assertCurrent(); return { profile: binding.profile, platform, opened: true as const };
  }
  async recoverLocalBridge(owner: Readonly<{ signal: AbortSignal; assertCurrent(): Promise<void> }>) {
    const timeout = AbortSignal.timeout(25_000), signal = AbortSignal.any([owner.signal, timeout]);
    const run = async (args: string[]) => { signal.throwIfAborted(); await owner.assertCurrent(); const result = await runTrustedOpenCliProcess(this.#installation, args, { deadlineMs: OPENCLI_PROBE_TIMEOUT, signal, configurationDirectory: this.#addons.configurationDirectory }); await owner.assertCurrent(); signal.throwIfAborted(); return result; };
    const operation = recovery.then(async () => {
      if (parseOpenCliVersion((await run(["--version"])).stdout) !== this.version) throw new OpenCliError("opencli_version_changed");
      let status = parseOpenCliBridgeStatus((await run(["daemon", "status"])).stdout); if (status.daemon_running) return status;
      await run(["daemon", "restart"]);
      for (let attempt = 0; attempt < 10; attempt++) { await delay(500, undefined, { signal }); status = parseOpenCliBridgeStatus((await run(["daemon", "status"])).stdout); if (status.daemon_running && status.extension_connected) break; }
      return status;
    });
    recovery = operation.then(() => undefined, () => undefined); return operation;
  }
  async checkCreatorAuthStatus(binding: VerifiedOpenCliBrowserBinding, platform: string): Promise<OpenCliAuthObservation> {
    const probe = creatorAuthProbe(platform);
    await this.checkBrowserProfile(binding);
    const observed = (status: OpenCliAuthObservation["status"], method: OpenCliAuthObservation["method"], issue: string | null): OpenCliAuthObservation => ({ platform, profile_ref: binding.profile, checked_at: Math.floor(Date.now() / 1000), status, method, issue });
    if (probe.addon && await this.#addons.state("onecrew-site-auth") !== "ready") return observed("unknown", "skipped", "auth_addon_required");
    if (!this.#registry.hasBrowserRead(probe.site, "whoami")) return observed("unknown", "skipped", "auth_probe_unavailable");
    const method = probe.full ? "full" : "quick";
    const args = creatorAuthArguments(platform, binding.profile);
    const output = await this.#run(args, probe.full ? 30000 : OPENCLI_PROBE_TIMEOUT, binding, false, probe.foregroundBrowser === true).then(value => ({ value }), error => ({ error }));
    if ("error" in output) {
      if (probe.foregroundBrowser && output.error instanceof OpenCliError && output.error.code === "opencli_auth_required") return observed("not_logged_in", method, null);
      if (output.error instanceof OpenCliError && output.error.code === "opencli_timeout") return observed("error", method, "auth_probe_timeout");
      if (output.error instanceof OpenCliError && output.error.code === "opencli_risk_control") return observed("error", method, "auth_probe_rate_limited");
      throw output.error;
    }
    if (probe.addon) await this.#addons.requireReady("onecrew-site-auth");
    const value = await parseOpenCliJson(output.value.stdout);
    const result = probe.foregroundBrowser ? decodeCreatorWhoami(value) : decodeCreatorAuthStatus(value, probe.site);
    if (result.method !== "skipped" && result.method !== method) throw new OpenCliError("opencli_invalid_output");
    return observed(result.status, result.method, result.issue);
  }
  async creatorProfileEvidence(binding: VerifiedOpenCliBrowserBinding, site: string, authorId: string, authorUrl: string) {
    const username = creatorProfileSource(site, authorId, authorUrl);
    switch (site) {
      case "linkedin": return this.linkedinPublicProfile(binding, username);
      case "pinterest": return this.pinterestPublicProfile(binding, authorId, username);
      case "bluesky": return this.blueskyPublicProfile(binding, authorId);
      case "weibo": return this.weiboPublicProfile(binding, authorId);
      case "facebook": return this.facebookPublicProfile(binding, authorId);
      case "bilibili": return this.bilibiliPublicProfile(binding, authorId);
      case "instagram": return this.instagramPublicProfile(binding, username, authorId);
      case "tiktok": return this.tiktokPublicProfile(binding, username, authorId);
      case "twitter": return this.twitterPublicProfile(binding, username);
      case "reddit": return this.redditPublicProfile(binding, username, authorId);
      default: throw new OpenCliError("opencli_read_command_unavailable");
    }
  }
  async #run(args: readonly string[], deadlineMs: number, binding?: VerifiedOpenCliBrowserBinding, webDiagnostics = false, foregroundBrowser = false) {
    if (binding) { binding.signal.throwIfAborted(); await binding.assertCurrent(); await this.#requireContextRoute(binding.profile); binding.signal.throwIfAborted(); }
    const output = await runTrustedOpenCliProcess(this.#installation, args, { deadlineMs, webDiagnostics, foregroundBrowser, configurationDirectory: this.#addons.configurationDirectory, ...(binding ? { signal: binding.signal } : {}) }).then(value => ({ value }), error => ({ error }));
    if ("error" in output && output.error instanceof OpenCliError && output.error.code === "opencli_cancel_pending") throw output.error;
    if (binding) { binding.signal.throwIfAborted(); await binding.assertCurrent(); await this.#requireContextRoute(binding.profile); binding.signal.throwIfAborted(); }
    if ("error" in output) throw output.error;
    return output.value;
  }
  async #requireContextRoute(contextId: string): Promise<void> {
    const path = join(this.#addons.configurationDirectory, "browser-profiles.json");
    const info = await lstat(path).then(value => value, error => { if ((error as NodeJS.ErrnoException).code === "ENOENT") return null; throw error; });
    if (!info) return;
    if (!info.isFile() || info.isSymbolicLink() || info.size > 256 * 1024) throw new OpenCliError("opencli_profile_unavailable");
    const config = parseStoredJson(new TextDecoder("utf-8", { fatal: true }).decode(await readFile(path)), true);
    if (!config || typeof config !== "object" || Array.isArray(config)) throw new OpenCliError("opencli_invalid_output");
    const aliases = (config as Record<string, unknown>).aliases;
    if (aliases !== undefined && (!aliases || typeof aliases !== "object" || Array.isArray(aliases))) throw new OpenCliError("opencli_invalid_output");
    if (aliases && Object.hasOwn(aliases, contextId) && (aliases as Record<string, unknown>)[contextId] !== contextId) throw new OpenCliError("opencli_profile_unavailable");
  }
  async #requireVersion(binding?: VerifiedOpenCliBrowserBinding): Promise<void> {
    const actual = parseOpenCliVersion((await this.#run(["--version"], OPENCLI_PROBE_TIMEOUT, binding)).stdout);
    if (actual !== this.version) throw new OpenCliError("opencli_version_changed");
  }
  async connectedProfiles() {
    await this.#requireVersion();
    return parseConnectedOpenCliProfiles((await this.#run(["profile", "list"], OPENCLI_PROBE_TIMEOUT)).stdout);
  }
  async bridgeStatus() {
    await this.#requireVersion();
    return parseOpenCliBridgeStatus((await this.#run(["daemon", "status"], OPENCLI_PROBE_TIMEOUT)).stdout);
  }
  async checkBrowserProfile(binding: VerifiedOpenCliBrowserBinding): Promise<void> {
    validateOpenCliProfile(binding.profile);
    const [, bridge, inventory] = await Promise.all([
      this.#requireVersion(binding),
      this.#run(["daemon", "status"], OPENCLI_PROBE_TIMEOUT, binding),
      this.#run(["profile", "list"], OPENCLI_PROBE_TIMEOUT, binding),
    ]);
    const status = parseOpenCliBridgeStatus(bridge.stdout);
    if (!status.daemon_running) throw new OpenCliError("opencli_daemon_unavailable");
    if (!status.extension_connected) throw new OpenCliError("opencli_extension_unavailable");
    const profiles = parseConnectedOpenCliProfiles(inventory.stdout);
    if (!profiles.some(profile => profile.context_id === binding.profile)) throw new OpenCliError("opencli_profile_unavailable");
  }
  async creatorSearchCapabilities() {
    await this.#requireVersion();
    const addons = await this.#addons.status();
    return Object.entries(CREATOR_SEARCH_SOURCES).filter(([, source]) => source.discovery).map(([platform, source]) => ({ platform, command: `${source.site}/${source.name}`, browser_required: true, registered: this.#registry.hasBrowserRead(source.site, source.name), addon: source.addon })).filter(source => source.registered && (source.addon === null || addons.some(addon => addon.id === source.addon && addon.state === "ready"))).map(({ addon: _addon, ...source }) => source);
  }
  async creatorSearch(binding: VerifiedOpenCliBrowserBinding, platform: string, query: string, limit: number): Promise<CreatorSearchHit[]> {
    const args = creatorSearchArguments(binding.profile, platform, query, limit);
    const source = creatorSearchSource(platform);
    if (!this.#registry.hasBrowserRead(source.site, source.name)) throw new OpenCliError("opencli_read_command_unavailable");
    if (source.addon) await this.#addons.requireReady(source.addon);
    await this.checkBrowserProfile(binding);
    const output = await this.#run(args, OPENCLI_READ_TIMEOUT, binding, false, platform === "instagram").then(value => value, error => {
      if (error instanceof OpenCliError && error.code === "opencli_command_failed" && error.exitCode === 66 && !["bilibili", "tiktok"].includes(platform) && !ambiguousCreatorSearchEmpty(platform)) return null;
      throw error;
    });
    if (output === null) return [];
    if (source.addon) await this.#addons.requireReady(source.addon);
    await binding.assertCurrent(); binding.signal.throwIfAborted();
    return decodeCreatorSearch(platform, await parseOpenCliJson(output.stdout), limit);
  }
  async #native<T>(binding: VerifiedOpenCliBrowserBinding, plan: ReviewedOpenCliRead<T>): Promise<T> {
    validateOpenCliProfile(binding.profile);
    if (plan.public ? !this.#registry.hasPublicRead(plan.site, plan.name) : !this.#registry.hasBrowserRead(plan.site, plan.name)) throw new OpenCliError("opencli_read_command_unavailable");
    for (const addon of plan.addons) await this.#addons.requireReady(addon);
    if (plan.public) await this.#requireVersion(binding); else await this.checkBrowserProfile(binding);
    const args = ["--profile", binding.profile, plan.site, plan.name, ...plan.args];
    const output = await this.#run(args, OPENCLI_READ_TIMEOUT, binding, false, plan.foregroundBrowser).then(value => value, error => {
      if (plan.emptyOn66 && error instanceof OpenCliError && error.code === "opencli_command_failed" && error.exitCode === 66) return null;
      throw error;
    });
    if (output === null) return plan.emptyOn66!();
    await this.#requireVersion(binding);
    for (const addon of plan.addons) await this.#addons.requireReady(addon);
    if (plan.maximumBytes !== undefined && Buffer.byteLength(output.stdout) > plan.maximumBytes) throw new OpenCliError("opencli_output_too_large");
    return plan.decode(await parseOpenCliJson(output.stdout));
  }
  youtubeSearch(binding: VerifiedOpenCliBrowserBinding, query: string, limit: number) { return this.#native(binding, youtubeSearchPlan(query, limit)); }
  youtubeChannel(binding: VerifiedOpenCliBrowserBinding, channel: string) { return this.#native(binding, youtubeChannelPlan(channel)); }
  youtubeVideo(binding: VerifiedOpenCliBrowserBinding, video: string) { return this.#native(binding, youtubeVideoPlan(video)); }
  youtubeTranscript(binding: VerifiedOpenCliBrowserBinding, video: string) { return this.#native(binding, youtubeTranscriptPlan(video)); }
  youtubeLanguageSample(binding: VerifiedOpenCliBrowserBinding, channel: string, video: string) { return this.#native(binding, youtubeLanguagePlan(channel, video)); }
  youtubeRecentLanguageUrls(binding: VerifiedOpenCliBrowserBinding, channel: string) { return this.#native(binding, youtubeRecentLanguagePlan(channel)); }
  async checkYoutubeEvidenceProfile(binding: VerifiedOpenCliBrowserBinding): Promise<void> {
    await this.#addons.requireReady("onecrew-youtube");
    if (!this.#registry.hasBrowserRead("onecrew-youtube", "video-evidence")) throw new OpenCliError("opencli_read_command_unavailable");
    await this.checkBrowserProfile(binding);
  }
  youtubeVideoEvidence(binding: VerifiedOpenCliBrowserBinding, video: string) { return this.#native(binding, youtubeVideoEvidencePlan(video)); }
  youtubeChannelContact(binding: VerifiedOpenCliBrowserBinding, channel: string, task: string) { return this.#native(binding, youtubeChannelContactPlan(channel, task)); }
  nativeComments(binding: VerifiedOpenCliBrowserBinding, source: NativeOpenCliCommentSource, limit: number) { return this.#native(binding, nativeCommentsPlan(source, limit)); }
  tiktokPublicProfile(binding: VerifiedOpenCliBrowserBinding, username: string, authorId: string) { return this.#native(binding, tiktokProfilePlan(username, authorId)); }
  tiktokProfileIdentity(binding: VerifiedOpenCliBrowserBinding, username: string) { return this.#native(binding, tiktokProfileIdentityPlan(username)); }
  instagramProfileIdentity(binding: VerifiedOpenCliBrowserBinding, username: string) { return this.#native(binding, instagramProfileIdentityPlan(username)); }
  bilibiliPublicProfile(binding: VerifiedOpenCliBrowserBinding, mid: string) { return this.#native(binding, bilibiliProfilePlan(mid)); }
  instagramPublicProfile(binding: VerifiedOpenCliBrowserBinding, username: string, authorId: string) { return this.#native(binding, instagramProfilePlan(username, authorId)); }
  twitterPublicProfile(binding: VerifiedOpenCliBrowserBinding, handle: string) { return this.#native(binding, twitterProfilePlan(handle)); }
  twitterIdentityProfile(binding: VerifiedOpenCliBrowserBinding, handle: string) { return this.#native(binding, twitterIdentityPlan(handle)); }
  twitterPostIdentity(binding: VerifiedOpenCliBrowserBinding, handle: string, postId: string) { return this.#native(binding, twitterPostIdentityPlan(handle, postId)); }
  twitterPostContent(binding: VerifiedOpenCliBrowserBinding, handle: string, postId: string, accountId: string) { return this.#native(binding, twitterPostContentPlan(handle, postId, accountId)); }
  twitterLanguageSample(binding: VerifiedOpenCliBrowserBinding, handle: string, postId: string, accountId: string) { return this.#native(binding, twitterLanguagePlan(handle, postId, accountId)); }
  twitterRecentContent(binding: VerifiedOpenCliBrowserBinding, handle: string) { return this.#native(binding, twitterRecentContentPlan(handle)); }
  bilibiliSourceVideoEvidence(binding: VerifiedOpenCliBrowserBinding, sourceUrl: string) { return this.#native(binding, bilibiliVideoPlan(sourceUrl)); }
  weiboPublicProfile(binding: VerifiedOpenCliBrowserBinding, authorId: string) { return this.#native(binding, weiboProfilePlan(authorId)); }
  weiboPostEvidence(binding: VerifiedOpenCliBrowserBinding, sourceUrl: string) { return this.#native(binding, weiboPostPlan(sourceUrl)); }
  weiboOriginalComments(binding: VerifiedOpenCliBrowserBinding, nativePostId: string) { return this.#native(binding, weiboCommentsPlan(nativePostId)); }
  pinterestPublicProfile(binding: VerifiedOpenCliBrowserBinding, authorId: string, username: string) { return this.#native(binding, pinterestProfilePlan(authorId, username)); }
  pinterestPublicContent(binding: VerifiedOpenCliBrowserBinding, authorId: string, username: string) { return this.#native(binding, pinterestContentPlan(authorId, username)); }
  pinterestPublicPinContent(binding: VerifiedOpenCliBrowserBinding, authorId: string, username: string, pinId: string) { return this.#native(binding, pinterestContentPlan(authorId, username, pinId)); }
  redditPublicProfile(binding: VerifiedOpenCliBrowserBinding, username: string, authorId: string) { return this.#native(binding, redditProfilePlan(username, authorId)); }
  redditRecentLanguageUrls(binding: VerifiedOpenCliBrowserBinding, username: string, authorId: string) { return this.#native(binding, redditRecentPlan(username, authorId)); }
  redditPostContent(binding: VerifiedOpenCliBrowserBinding, authorId: string, source: string) { return this.#native(binding, redditPostPlan(authorId, source)); }
  redditLanguageSample(binding: VerifiedOpenCliBrowserBinding, username: string, authorId: string, source: string) { return this.#native(binding, redditLanguagePlan(username, authorId, source)); }
  douyinPublicProfile(binding: VerifiedOpenCliBrowserBinding, authorId: string) { return this.#native(binding, douyinProfilePlan(authorId)); }
  douyinOriginalContent(binding: VerifiedOpenCliBrowserBinding, videoId: string, authorId?: string) { return this.#native(binding, douyinContentPlan(videoId, authorId)); }
  douyinOriginalComments(binding: VerifiedOpenCliBrowserBinding, videoId: string, authorId: string) { return this.#native(binding, douyinCommentsPlan(videoId, authorId)); }
  blueskyPublicProfile(binding: VerifiedOpenCliBrowserBinding, did: string) { return this.#native(binding, blueskyProfilePlan(did)); }
  blueskyOriginalPost(binding: VerifiedOpenCliBrowserBinding, uri: string) { return this.#native(binding, blueskyPostPlan(uri)); }
  blueskyPostEvidence(binding: VerifiedOpenCliBrowserBinding, source: string) { return this.#native(binding, blueskyPostPlan(blueskyUriFromSource(source))); }
  blueskyOriginalComments(binding: VerifiedOpenCliBrowserBinding, uri: string) { return this.#native(binding, blueskyPostPlan(uri, true)); }
  linkedinPublicProfile(binding: VerifiedOpenCliBrowserBinding, handle: string) { return this.#native(binding, linkedinProfilePlan(handle)); }
  linkedinOriginalContent(binding: VerifiedOpenCliBrowserBinding, handle: string, accountId: string) { return this.#native(binding, linkedinContentPlan(handle, accountId)); }
  linkedinCurrentExperience(binding: VerifiedOpenCliBrowserBinding, handle: string, accountId: string) { return this.#native(binding, linkedinExperiencePlan(handle, accountId)); }
  linkedinContactPage(binding: VerifiedOpenCliBrowserBinding, handle: string, accountId: string) { return this.#native(binding, linkedinContactPlan(handle, accountId)); }
  linkedinOriginalComments(binding: VerifiedOpenCliBrowserBinding, handle: string, accountId: string, source: string) { return this.#native(binding, linkedinCommentsPlan(handle, accountId, source)); }
  facebookSourceIdentity(binding: VerifiedOpenCliBrowserBinding, source: string) { return this.#native(binding, facebookIdentityPlan(source)); }
  facebookPublicProfile(binding: VerifiedOpenCliBrowserBinding, authorId: string) { return this.#native(binding, facebookProfilePlan(authorId)); }
  facebookNativeProfile(binding: VerifiedOpenCliBrowserBinding, authorId: string, source: string) { return this.#native(binding, facebookProfilePlan(authorId, source)); }
  facebookContactPage(binding: VerifiedOpenCliBrowserBinding, authorId: string, source?: string) { return this.#native(binding, facebookContactPlan(authorId, undefined, source)); }
  facebookNativeContactPage(binding: VerifiedOpenCliBrowserBinding, authorId: string, base: string, source: string) { return this.#native(binding, facebookContactPlan(authorId, { base, source })); }
  facebookNativeTargets(binding: VerifiedOpenCliBrowserBinding, authorId: string, base: string) { return this.#native(binding, facebookRecentPlan(authorId, base)); }
  facebookRecentLanguageUrls(binding: VerifiedOpenCliBrowserBinding, authorId: string) { return this.#native(binding, facebookRecentPlan(authorId)); }
  facebookReelContent(binding: VerifiedOpenCliBrowserBinding, authorId: string, source: string) { return this.#native(binding, facebookReelPlan(authorId, source)); }
  facebookLanguageSample(binding: VerifiedOpenCliBrowserBinding, authorId: string, source: string) { return this.#native(binding, facebookLanguagePlan(authorId, source)); }
  facebookNativeOriginal(binding: VerifiedOpenCliBrowserBinding, authorId: string, base: string, source: string) { return this.#native(binding, facebookNativeOriginalPlan(authorId, base, source)); }
  bilibiliVideoEvidence(binding: VerifiedOpenCliBrowserBinding, sourceUrl: string, authorId: string) { return this.#native(binding, bilibiliVideoPagePlan(sourceUrl, authorId)); }
  tiktokRecentLanguageUrls(binding: VerifiedOpenCliBrowserBinding, username: string, authorId: string) { return this.#native(binding, tiktokRecentLanguagePlan(username, authorId)); }
  tiktokLanguageSample(binding: VerifiedOpenCliBrowserBinding, username: string, authorId: string, source: string, includeComments = true) { return this.#native(binding, tiktokLanguagePlan(username, authorId, source, includeComments)); }
  bilibiliLanguageSample(binding: VerifiedOpenCliBrowserBinding, mid: string, source: string) { return this.#native(binding, bilibiliLanguagePlan(mid, source)); }
  bilibiliRecentLanguageUrls(binding: VerifiedOpenCliBrowserBinding, mid: string) { return this.#native(binding, bilibiliLanguagePlan(mid, null)); }
  instagramRecentOriginalPosts(binding: VerifiedOpenCliBrowserBinding, username: string, authorId: string, query = "", sourceUrl?: string) { return this.#native(binding, instagramRecentOriginalsPlan(username, authorId, query, sourceUrl)); }
  instagramCommentLanguageEvidence(binding: VerifiedOpenCliBrowserBinding, username: string, source: string) { return this.#native(binding, instagramCommentLanguagePlan(username, source)); }
  async readPublicWeb(binding: VerifiedOpenCliBrowserBinding, source: VerifiedOpenCliPublicSource) {
    const url = source.url;
    validatePublicWebUrl(url);
    if (!this.#registry.hasBrowserRead("web", "read")) throw new OpenCliError("opencli_read_command_unavailable");
    await source.assertApproved();
    await this.checkBrowserProfile(binding);
    await checkPublicWebDestination(url, binding.signal);
    await source.assertApproved();
    const output = await this.#run(["--profile", binding.profile, "web", "read", "--url", url, "--stdout", "--download-images", "false", "--frames", "none", "--diagnose", "--wait", "3"], OPENCLI_READ_TIMEOUT, binding, true);
    await source.assertApproved();
    const read = await decodePublicWebRead(url, output.stdout, output.stderr);
    await checkPublicWebDestination(read.document_url, binding.signal);
    await binding.assertCurrent(); binding.signal.throwIfAborted();
    return read;
  }
  // 仅用户明确请求的本地恢复调用此方法；不打开平台页面，不重启健康 daemon。
  async recoverBridge(binding: VerifiedOpenCliBrowserBinding) {
    const timeout = AbortSignal.timeout(25_000);
    const scoped = { profile: binding.profile, signal: AbortSignal.any([binding.signal, timeout]), assertCurrent: () => binding.assertCurrent() };
    const operation = recovery.then(async () => {
      await this.#requireVersion(scoped);
      let status = parseOpenCliBridgeStatus((await this.#run(["daemon", "status"], OPENCLI_PROBE_TIMEOUT, scoped)).stdout);
      if (status.daemon_running) return status;
      await this.#run(["daemon", "restart"], OPENCLI_PROBE_TIMEOUT, scoped);
      for (let attempt = 0; attempt < 10; attempt++) {
        await delay(500, undefined, { signal: scoped.signal });
        status = parseOpenCliBridgeStatus((await this.#run(["daemon", "status"], OPENCLI_PROBE_TIMEOUT, scoped)).stdout);
        if (status.daemon_running && status.extension_connected) break;
      }
      return status;
    });
    recovery = operation.then(() => undefined, () => undefined);
    return operation.then(value => value, error => {
      if (timeout.aborted && !binding.signal.aborted) throw new OpenCliError("opencli_timeout");
      throw error;
    });
  }
}
