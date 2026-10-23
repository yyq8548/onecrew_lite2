import { z } from "zod";
import type { ReadCreatorProfile } from "../../creators/candidate-input.ts";
import type { CandidateEvidence } from "../../creators/candidate-contracts.ts";

export class CreatorProviderReadError extends Error {
  readonly kind: "provider_read" | "identity_mismatch" | "source_mismatch";
  constructor(kind: "provider_read" | "identity_mismatch" | "source_mismatch", code: string) { super(code); this.kind = kind; }
}
export const record = (value: unknown): Record<string, unknown> => z.record(z.string(), z.unknown()).parse(value);
const string = (value: unknown) => typeof value === "string" && value.isWellFormed() ? value : "";
const optional = (value: unknown) => { const text = string(value); return text ? text : null; };
export function observedCount(value: unknown): number | bigint | null {
  if (typeof value === "number" && Number.isSafeInteger(value) && value >= 0) return value;
  if (typeof value === "bigint" && value >= 0n && value <= 18446744073709551615n) return value;
  return null;
}
function identity(selected: string, observed: string) { if (selected !== observed) throw new CreatorProviderReadError("identity_mismatch", "creator_provider_identity_mismatch"); }
function field(source: "profile_bio" | "profile_location", source_url: string, text: string): CandidateEvidence { return { kind: "profile_field", source, source_url, text }; }
function firstXUser(value: unknown, depth = 0): Record<string, unknown> | null {
  if (depth > 64) throw new CreatorProviderReadError("provider_read", "x_profile_invalid");
  if (Array.isArray(value)) { for (const item of value) { const found = firstXUser(item, depth + 1); if (found) return found; } return null; }
  if (!value || typeof value !== "object") return null;
  const row = record(value); if (string(row.rest_id) && row.__typename !== "UserUnavailable") return row;
  for (const item of Object.values(row)) { const found = firstXUser(item, depth + 1); if (found) return found; }
  return null;
}
export function projectXProfile(payload: unknown, selected: string): ReadCreatorProfile {
  const row = firstXUser(payload); if (!row) throw new CreatorProviderReadError("provider_read", "x_profile_missing");
  const id = string(row.rest_id); identity(selected, id);
  const legacy = row.legacy === undefined ? {} : record(row.legacy); const core = row.core === undefined ? {} : record(row.core);
  const username = string(core.screen_name) || string(legacy.screen_name); const name = string(core.name) || string(legacy.name);
  if (!/^[A-Za-z\d_]{1,15}$/u.test(username) || !name) throw new CreatorProviderReadError("provider_read", "x_profile_invalid");
  const bio = string(legacy.description); const location = row.location && typeof row.location === "object" ? string(record(row.location).location) || string(legacy.location) : string(legacy.location);
  const source = `https://x.com/${username}`; const evidence: CandidateEvidence[] = [field("profile_bio", source, bio)];
  const links: { display_text: string; target_url: string }[] = [];
  const entities = legacy.entities === undefined ? {} : record(legacy.entities);
  for (const kind of ["url", "description"]) {
    const section = entities[kind] === undefined ? {} : record(entities[kind]);
    if (section.urls !== undefined && !Array.isArray(section.urls)) throw new CreatorProviderReadError("provider_read", "x_profile_links_invalid");
    for (const entry of (section.urls as unknown[] | undefined) ?? []) {
      const link = record(entry); const target_url = string(link.expanded_url) || string(link.url); const display_text = string(link.display_url) || target_url;
      if (target_url && !links.some(value => value.target_url === target_url && value.display_text === display_text)) links.push({ display_text, target_url });
    }
  }
  evidence.push(...links.map(link => ({ kind: "profile_link" as const, source: "profile_website" as const, source_url: source, ...link })));
  if (location.trim()) evidence.push(field("profile_location", source, location));
  const avatar = row.avatar && typeof row.avatar === "object" ? string(record(row.avatar).image_url) || string(legacy.profile_image_url_https) : string(legacy.profile_image_url_https);
  const followers = observedCount(legacy.followers_count);
  if (followers === null) throw new CreatorProviderReadError("provider_read", "x_profile_followers_unavailable");
  return { observed_at: Math.floor(Date.now() / 1000), account: { platform: "x", external_id: id, target_kind: "user", handle: username, display_name: name, profile_url: source }, profile: { avatar_url: avatar || null, description: bio, location: location || null, website_urls: links.map(link => link.target_url), follower_count: followers }, evidence, warnings: [], administrator: null };
}
export function projectInstagramProfile(payload: unknown, selected: string): ReadCreatorProfile {
  const envelope = record(payload); const user = record(envelope.user);
  const id = typeof user.pk === "bigint" || typeof user.pk === "number" ? String(user.pk) : string(user.pk); identity(selected, id);
  const username = string(user.username); const name = string(user.full_name);
  if (!/^[A-Za-z\d_.]{1,30}$/u.test(username)) throw new CreatorProviderReadError("provider_read", "instagram_profile_invalid");
  const source = `https://www.instagram.com/${username}/`; const links: string[] = [];
  if (user.bio_links !== undefined && !Array.isArray(user.bio_links)) throw new CreatorProviderReadError("provider_read", "instagram_profile_links_invalid");
  for (const entry of (user.bio_links as unknown[] | undefined) ?? []) { const row = record(entry); const link = optional(row.url) ?? optional(row.lynx_url); if (link && !links.includes(link)) links.push(link); }
  const external = optional(user.external_url); if (external && !links.includes(external)) links.push(external);
  const evidence: CandidateEvidence[] = links.map(target_url => ({ kind: "profile_link", source: "profile_website", source_url: source, display_text: target_url, target_url }));
  const bio = optional(user.biography); if (bio !== null) evidence.push(field("profile_bio", source, bio));
  const email = optional(user.public_email); if (email !== null) evidence.push(field("profile_bio", source, email));
  for (const value of [optional(user.city_name), optional(user.address_street)]) if (value?.trim()) evidence.push(field("profile_location", source, value));
  return { observed_at: Math.floor(Date.now() / 1000), account: { platform: "instagram", external_id: id, target_kind: "user", handle: username, display_name: name, profile_url: source }, profile: { avatar_url: optional(user.profile_pic_url), description: bio, location: optional(user.city_name), website_urls: links, follower_count: observedCount(user.follower_count) }, evidence, warnings: [], administrator: null };
}
