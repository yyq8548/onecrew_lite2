import type { AccountContext } from "../../identity/account-context.ts";
import { parseCreatorSaveCandidates } from "../../creators/candidate-input.ts";
import type { CreatorSaveCandidateInput, CreatorSaveProfileRead, CreatorSaveProfileResolver, ReadCreatorProfile } from "../../creators/candidate-input.ts";
import type { CandidateEvidence } from "../../creators/candidate-contracts.ts";
import { discoverCandidateContacts } from "../../creators/candidate-extraction.ts";
import type { OwnedPlatformAccountResolver } from "../browser/owned-accounts.ts";
import { CreatorProviderReadError } from "./projection.ts";
import { readXCreator, readInstagramCreator } from "./social-read.ts";
import { readYoutubeCreator } from "./youtube-read.ts";
import { YoutubeApiReadError } from "../browser/youtube-health.ts";
import { readTelegramAdministrator } from "./telegram-read.ts";
import type { SavedTelegramCommunityResolver } from "./telegram-read.ts";
import { readHeldLinkHub, LinkHubReadError } from "./link-hub.ts";

export function creatorLinkHubs(profile: ReadCreatorProfile): string[] {
  const fields = profile.evidence.filter((value): value is Extract<CandidateEvidence, { kind: "profile_field" }> => value.kind === "profile_field");
  const links = profile.evidence.filter((value): value is Extract<CandidateEvidence, { kind: "profile_link" }> => value.kind === "profile_link");
  const hubs = new Set(discoverCandidateContacts(profile.account.platform, fields, links).filter(value => value.channel === "link_hub").map(value => value.value));
  for (const value of profile.evidence) if (value.kind === "observation" && value.channel === "link_hub") hubs.add(value.value);
  return [...hubs].sort();
}
async function addHubs(profile: ReadCreatorProfile, batchDeadline: number, signal: AbortSignal): Promise<void> {
  const hubs = creatorLinkHubs(profile); let used = 0;
  for (const [index, url] of hubs.entries()) {
    const remaining = Math.min(batchDeadline - performance.now(), 30000 - used);
    if (index >= 2 || remaining <= 0) { profile.warnings.push("Additional link-hub pages were not read because this preview's page or time limit was reached."); break; }
    signal.throwIfAborted(); const started = performance.now(); const budget = AbortSignal.timeout(Math.max(1, Math.floor(remaining)));
    const result = await readHeldLinkHub(url, AbortSignal.any([signal, budget])).then(value => ({ value }), error => ({ error }));
    used += performance.now() - started; signal.throwIfAborted();
    if ("error" in result) {
      if (!(result.error instanceof LinkHubReadError) && !(result.error instanceof SyntaxError) && !(result.error instanceof DOMException) && !budget.aborted) throw result.error;
      profile.warnings.push("A linked contact page could not be read; its unverified contacts are excluded from this preview.");
    } else profile.evidence.push({ kind: "held_hub", source_url: result.value.source_url, text: result.value.emails.map(value => value.value).join(" "), links: result.value.links.map(value => ({ display_text: value.text ?? "", target_url: value.url })) });
  }
}

// 每次请求由服务传入唯一的已验证 AccountContext；不从候选请求创建身份或选择另一个 active account。
export class NativeCreatorSaveProfileResolver implements CreatorSaveProfileResolver {
  readonly #context: AccountContext;
  readonly #accounts: OwnedPlatformAccountResolver;
  readonly #savedRoutes: SavedTelegramCommunityResolver | undefined;
  constructor(context: AccountContext, accounts: OwnedPlatformAccountResolver, savedRoutes?: SavedTelegramCommunityResolver) { this.#context = context; this.#accounts = accounts; this.#savedRoutes = savedRoutes; }
  async #resolve(candidate: CreatorSaveCandidateInput, signal: AbortSignal): Promise<ReadCreatorProfile> {
    const target = candidate.target; const connection = candidate.connection;
    if (target.platform === "x" && connection.platform === "x") return this.#accounts.withProfile(this.#context, connection.account_ref_id, "x", (_profile, secret, assertCurrent) => readXCreator(secret, target.handle, target.user_id, signal, assertCurrent));
    if (target.platform === "instagram" && connection.platform === "instagram") return this.#accounts.withProfile(this.#context, connection.account_ref_id, "instagram", (_profile, secret, assertCurrent) => readInstagramCreator(secret, target.handle, target.user_id, signal, assertCurrent));
    if (target.platform === "youtube" && connection.platform === "youtube") return this.#accounts.withYoutubeCredential(this.#context, connection.service_credential_ref_id, (_credential, key, assertCurrent, request) => readYoutubeCreator(key, target.channel_id, signal, assertCurrent, request)).then(value => value, error => { if (error instanceof YoutubeApiReadError) throw new CreatorProviderReadError("provider_read", error.message); throw error; });
    if (target.platform === "telegram_administrator" && connection.platform === "telegram") return this.#accounts.withProfile(this.#context, connection.account_ref_id, "telegram", (profile, secret, assertCurrent) => readTelegramAdministrator(secret, profile.provider_account_id, candidate, signal, assertCurrent, this.#savedRoutes));
    throw new CreatorProviderReadError("source_mismatch", "creator_provider_connection_mismatch");
  }
  async resolveMany(input: readonly CreatorSaveCandidateInput[]): Promise<readonly CreatorSaveProfileRead[]> {
    const candidates = parseCreatorSaveCandidates(input); const deadline = performance.now() + 120000;
    const stop = new AbortController(); const signal = AbortSignal.any([this.#context.signal, stop.signal]);
    const output: CreatorSaveProfileRead[] = new Array(candidates.length); let next = 0;
    const worker = async () => {
      while (next < candidates.length) {
        signal.throwIfAborted(); const index = next++; const candidate = candidates[index]!;
        const result = await this.#resolve(candidate, signal).then(value => ({ value }), error => ({ error }));
        if ("error" in result) {
          if (!(result.error instanceof CreatorProviderReadError)) { stop.abort(result.error); throw result.error; }
          output[index] = { candidate, result: { ok: false, error: { kind: result.error.kind, message: result.error.message } } };
        } else { await addHubs(result.value, deadline, signal); output[index] = { candidate, result: { ok: true, value: result.value } }; }
      }
    };
    const completion = await Promise.allSettled(Array.from({ length: Math.min(4, candidates.length) }, () => worker().then(value => value, error => { stop.abort(error); throw error; })));
    const failures = completion.filter((value): value is PromiseRejectedResult => value.status === "rejected").map(value => value.reason as unknown);
    if (failures.length) throw new AggregateError(failures, "Creator provider reads did not complete");
    return output;
  }
}
