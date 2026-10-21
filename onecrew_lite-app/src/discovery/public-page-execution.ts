import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { OpenCliReader } from "../platforms/opencli/reader.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";
import { validatePublicWebUrl } from "../platforms/opencli/public-url.ts";
import { facebookNativeContactBase } from "../platforms/opencli/facebook-profile.ts";
import type { DiscoveryBrowserResolver, DiscoveryRepositoryResolver } from "./search-execution.ts";
import { discoveryPublicPageInput, type DiscoveryPublicPageInput } from "./contact-native.ts";
import type { DiscoveryPublicPageClaim } from "./public-page-repository.ts";
import type { DiscoveryProfileReceipt } from "./profile-repository.ts";
import type { DiscoveryContactAudit } from "./public-page-audit.ts";

export function discoveryFacebookContactAuthor(saved: DiscoveryProfileReceipt, claim: DiscoveryPublicPageClaim, base: string): string {
  const native = saved.native_profile, scope = claim.input;
  if (saved.status !== "completed" || saved.stop_code !== null || saved.claim.campaign_id !== scope.campaign_id || saved.claim.search_hit_id !== claim.search_hit_id || saved.claim.expected_revision !== scope.expected_revision || saved.claim.profile_ref !== scope.profile_ref || saved.claim.platform !== "facebook" || saved.claim.call_number >= claim.call_number || native?.adapter_version !== "2.0.0" || native.observed_profile_url !== base || native.id !== saved.claim.author_id || saved.claim.source_url !== claim.source_url || !Array.isArray(native.contact_links) || !native.contact_links.includes(scope.requested_url)) throw new OpenCliError("opencli_invalid_output");
  return saved.claim.author_id;
}
function facebookAuthor(source: string): string | null {
  const match = /^https:\/\/www\.facebook\.com\/profile\.php\?id=([1-9]\d{0,19})&sk=(?:about_contact_and_basic_info|directory_contact_info|directory_links)$/u.exec(source);
  return match?.[1] ?? null;
}
function linkedinHandle(source: string): string | null {
  const match = /^https:\/\/www\.linkedin\.com\/in\/([A-Za-z\d][A-Za-z\d_-]{0,127})\/overlay\/contact-info\/$/u.exec(source);
  return match && !["me", "self"].includes(match[1]!.toLowerCase()) ? match[1]! : null;
}
export class DiscoveryPublicPageExecutor {
  readonly accounts: AccountContextService; readonly repositories: DiscoveryRepositoryResolver; readonly browsers: DiscoveryBrowserResolver; readonly reader: OpenCliReader;
  readonly audit: DiscoveryContactAudit | undefined;
  constructor(accounts: AccountContextService, repositories: DiscoveryRepositoryResolver, browsers: DiscoveryBrowserResolver, reader: OpenCliReader, audit?: DiscoveryContactAudit) { this.accounts = accounts; this.repositories = repositories; this.browsers = browsers; this.reader = reader; this.audit = audit; }
  async readOnce(context: AccountContext, raw: DiscoveryPublicPageInput) {
    const input = discoveryPublicPageInput.parse(raw);
    validatePublicWebUrl(input.requested_url);
    this.accounts.assertCurrent(context);
    const owned = await this.repositories.resolve(context); owned.assertOwned(); this.accounts.assertCurrent(context);
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_public_page_repository_owner_mismatch");
    const campaign = owned.repository.getPlan(input.campaign_id);
    if (this.reader.version !== "1.8.8" || campaign.opencli_version !== this.reader.version) throw new OpenCliError("opencli_version_changed");
    const browser = await this.browsers.resolve(context, input.profile_ref); await browser.assertCurrent();
    if (browser.profile !== input.profile_ref) throw new Error("discovery_browser_profile_mismatch");
    this.accounts.assertCurrent(context); owned.assertOwned();
    const claim = owned.repository.claimPublicPage(input);
    const started = performance.now();
    const read = async () => {
      const channel = /^https:\/\/www\.youtube\.com\/channel\/([^/?#]+)$/u.exec(input.requested_url)?.[1];
      if (channel !== undefined) {
        const value = await this.reader.youtubeChannelContact(browser, channel, input.campaign_id);
        if (value.channelId !== channel) throw new OpenCliError("opencli_invalid_output");
        const status = value.hiddenEmailStatus === undefined ? "" : `\nYouTube hidden email status: ${value.hiddenEmailStatus}; task: ${input.campaign_id}`;
        const text = `${value.title}\n${value.description}\n${(value.publicLinks ?? []).map(link => `Website: ${link}`).join("\n")}${status}`;
        let excerpt = "", bytes = 0;
        for (const character of text) { const size = Buffer.byteLength(character); if (bytes + size > 20000) break; excerpt += character; bytes += size; }
        return { document_url: input.requested_url, title: value.title, markdown_excerpt: excerpt, truncated: bytes < Buffer.byteLength(text) || value.linksTruncated === true || value.descriptionStatus === undefined || value.descriptionStatus === "unknown", hidden: value.hiddenEmailAvailable };
      }
      const base = facebookNativeContactBase(input.requested_url);
      if (base !== null) {
        const author = discoveryFacebookContactAuthor(owned.repository.getProfile(input.campaign_id, claim.search_hit_id), claim, base);
        return { ...await this.reader.facebookNativeContactPage(browser, author, base, input.requested_url), hidden: false };
      }
      const facebook = facebookAuthor(input.requested_url);
      if (facebook !== null) return { ...await this.reader.facebookContactPage(browser, facebook, input.requested_url), hidden: false };
      const linkedin = linkedinHandle(input.requested_url);
      if (linkedin !== null) {
        const saved = owned.repository.getProfile(input.campaign_id, claim.search_hit_id), native = saved.native_profile;
        if (saved.status !== "completed" || saved.stop_code !== null || saved.claim.expected_revision !== input.expected_revision || saved.claim.profile_ref !== input.profile_ref || saved.claim.platform !== "linkedin" || saved.claim.author_id !== linkedin || saved.claim.author_url !== claim.source_url || saved.claim.call_number >= claim.call_number || !Array.isArray(native?.contact_links) || !native.contact_links.includes(input.requested_url) || typeof native.account_id !== "string") throw new OpenCliError("opencli_invalid_output");
        return { ...await this.reader.linkedinContactPage(browser, linkedin, native.account_id), hidden: false };
      }
      const value = await this.reader.readPublicWeb(browser, { url: input.requested_url, async assertApproved() { owned.assertOwned(); await browser.assertCurrent(); } });
      return { ...value, hidden: false };
    };
    const observed = await read().then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
    // 已扣除的读取始终结算回原账号；后续步骤再核对当前账号。
    owned.assertOwned();
    const elapsed = Math.floor(performance.now() - started);
    const complete = async (completion: Parameters<typeof owned.repository.finishPublicPage>[0]) => {
      const receipt = owned.repository.finishPublicPage(completion);
      await this.audit?.record(input.campaign_id, { kind: "page", candidate_id: input.candidate_id, call_number: claim.call_number, elapsed_ms: elapsed, provider_status: completion.stop_code === null ? "completed" : "failed", stop_code: completion.stop_code });
      return receipt;
    };
    if (observed.ok) return complete({ hidden_email_available: observed.value.hidden, claim, document_url: observed.value.document_url, title: observed.value.title, markdown_excerpt: observed.value.markdown_excerpt, truncated: observed.value.truncated, stop_code: null });
    if (observed.error instanceof OpenCliError && !["opencli_cancelled", "opencli_cancel_pending"].includes(observed.error.code)) return complete({ hidden_email_available: false, claim, document_url: null, title: "", markdown_excerpt: "", truncated: false, stop_code: observed.error.code === "opencli_read_command_unavailable" ? "opencli_command_unavailable" : observed.error.code });
    const interrupted = await Promise.resolve().then(() => owned.repository.interruptPublicPage(claim)).then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown }));
    if (!interrupted.ok) throw new AggregateError([observed.error, interrupted.error], "discovery_public_page_read_and_interrupt_failed");
    throw observed.error;
  }
}
