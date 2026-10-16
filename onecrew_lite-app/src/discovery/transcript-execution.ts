import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";
import type { OpenCliReader } from "../platforms/opencli/reader.ts";
import type { DiscoveryBrowserResolver, DiscoveryRepositoryResolver } from "./search-execution.ts";

export function boundedDiscoveryTranscript(lines: readonly { timestamp: string; speaker: string; text: string }[]) {
  const segments: { timestamp: string; speaker: string; text: string }[] = [];
  for (const line of lines.slice(0, 200)) {
    segments.push({ timestamp: line.timestamp, speaker: line.speaker, text: line.text });
    if (Buffer.byteLength(stringifyApplicationJson(segments)) > 24000) {
      segments.pop(); if (!segments.length) throw new OpenCliError("opencli_output_too_large"); break;
    }
  }
  return { segments, truncated: segments.length < lines.length };
}
export class DiscoveryTranscriptExecutor {
  readonly accounts: AccountContextService; readonly repositories: DiscoveryRepositoryResolver; readonly browsers: DiscoveryBrowserResolver; readonly reader: OpenCliReader;
  constructor(accounts: AccountContextService, repositories: DiscoveryRepositoryResolver, browsers: DiscoveryBrowserResolver, reader: OpenCliReader) { this.accounts = accounts; this.repositories = repositories; this.browsers = browsers; this.reader = reader; }
  async readOnce(context: AccountContext, input: { campaign_id: string; expected_revision: number; profile_ref: string; search_hit_id: string }) {
    this.accounts.assertCurrent(context); const owned = await this.repositories.resolve(context); owned.assertOwned(); this.accounts.assertCurrent(context);
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_transcript_repository_owner_mismatch");
    const campaign = owned.repository.getPlan(input.campaign_id);
    if (campaign.opencli_version !== "1.8.8" || this.reader.version !== campaign.opencli_version) throw new OpenCliError("opencli_version_changed");
    const browser = await this.browsers.resolve(context, input.profile_ref); await browser.assertCurrent();
    if (browser.profile !== input.profile_ref) throw new Error("discovery_browser_profile_mismatch");
    this.accounts.assertCurrent(context); owned.assertOwned();
    const claim = owned.repository.claimTranscript(input.campaign_id, input.expected_revision, input.profile_ref, input.search_hit_id);
    const observed = await this.reader.youtubeTranscript(browser, claim.source_url).then(boundedDiscoveryTranscript).then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
    owned.assertOwned();
    if (observed.ok) return owned.repository.finishTranscript(claim, { ...observed.value, stop_code: null });
    if (observed.error instanceof OpenCliError && observed.error.code !== "opencli_cancelled") return owned.repository.finishTranscript(claim, { segments: [], truncated: false, stop_code: observed.error.code === "opencli_read_command_unavailable" ? "opencli_command_unavailable" : observed.error.code });
    const interrupted = await Promise.resolve().then(() => owned.repository.interruptTranscript(claim)).then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown }));
    if (!interrupted.ok) throw new AggregateError([observed.error, interrupted.error], "discovery_transcript_read_and_interrupt_failed");
    throw observed.error;
  }
}
