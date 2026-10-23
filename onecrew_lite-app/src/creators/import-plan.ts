import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { createHash, randomUUID } from "node:crypto";
import { isLosslessNumber, stringify } from "lossless-json";
import { parseStoredJson } from "../contracts/stored-json.ts";
import { asciiLower, parseJson, platform, tags, text } from "./contracts.ts";
import { transferFollowers as followers } from "./transfer-contracts.ts";
import { compareText, type TransferAccount, type TransferBundle, type TransferContact, type TransferCreator } from "./transfer-contracts.ts";

type AccountKey = Pick<TransferAccount, "platform" | "target_kind" | "external_id">;
type ContactKey = { channel: string; kind: string; normalized_value: string };
export interface BasisAccount { key: AccountKey; account_id: string; creator_id: string | null; link_state: string | null; creator_score: number | null; tags?: string[]; follower_counts_json: string | null; profile_handle: string | null; profile_display_name: string | null; profile_url: string | null; profile_public_json: string | null }
export interface ImportBasis { domain_empty: boolean; accounts: BasisAccount[]; handle_owners: { platform: string; target_kind: string; handle: string; creator_id: string }[]; contacts: { key: ContactKey; locator_id: string; creator_id: string | null }[] }
export interface ProfileFacts { handle: string | null; display_name: string | null; profile_url: string | null; public_data: unknown }
export interface AccountPlan { incoming: TransferAccount; existing_account_id: string | null; existing_link_state: string | null; create_link: boolean; promote_to_confirmed: boolean; profile_to_write: ProfileFacts | null }
export interface ContactPlan { incoming: TransferContact; target_locator_id: string; create_locator: boolean; create_route: boolean }
export interface PlanItem { incoming: TransferCreator; target_creator_id: string; resolution: "new" | "public_identity"; create_creator: boolean; tags_after?: string[]; fill_score: number | null; follower_counts_after: TransferCreator["follower_counts"] | null; accounts: AccountPlan[]; contacts: ContactPlan[]; changed: boolean }
export const emptyImpact = () => ({ creator_inputs: 0, creators_created: 0, creators_merged_by_public_identity: 0, creator_scores_filled: 0, follower_counts_filled: 0, platform_accounts_created: 0, claimed_links_created: 0, confirmed_links_created: 0, rejected_links_created: 0, links_promoted_to_confirmed: 0, profile_observations_created: 0, contact_routes_created: 0, creators_unchanged: 0, creator_tags_filled: 0 });
export type ImportImpact = ReturnType<typeof emptyImpact>;
export interface ImportPlan { content_sha256: string; exact_restore: boolean; creators: PlanItem[]; impact: ImportImpact }
export interface ImportConflict { kind: string; incoming_creator_key: string; account: AccountKey | null; local_creator_ids: string[] }
export const changesGraph = (impact: ImportImpact) => Object.entries(impact).some(([key, value]) => !["creator_inputs", "creators_merged_by_public_identity", "creators_unchanged"].includes(key) && value > 0);
export const accountKey = (a: AccountKey) => JSON.stringify([a.platform, a.target_kind, a.external_id]);
export const contactKey = (r: TransferContact | ContactKey) => JSON.stringify([r.channel, "kind" in r ? r.kind : r.locator_kind, r.normalized_value]);
const nullableText = (value: unknown) => value === null ? null : text.parse(value);
export function loadImportBasis(db: DatabaseSync, bundle: TransferBundle): ImportBasis {
  const domain_empty = db.prepare("SELECT (SELECT count(*) FROM creators)=0 AND (SELECT count(*) FROM platform_accounts)=0 AND (SELECT count(*) FROM creator_account_links)=0 AND (SELECT count(*) FROM platform_account_observations)=0 AND (SELECT count(*) FROM contact_locators)=0 AND (SELECT count(*) FROM creator_contact_routes)=0 AS empty").get()!.empty === 1;
  const inputAccounts = [...new Map(bundle.creators.flatMap(c => c.accounts).map(a => [accountKey(a), { platform: a.platform, target_kind: a.target_kind, external_id: a.external_id }])).values()];
  const accounts = db.prepare(`WITH input AS (SELECT json_extract(value,'$.platform') AS platform,json_extract(value,'$.target_kind') AS target_kind,json_extract(value,'$.external_id') AS external_id FROM json_each(?))
    SELECT account.platform,account.target_kind,account.external_id,account.id AS account_id,link.creator_id,link.state AS link_state,creator.score AS creator_score,creator.follower_counts_json,creator.tags_json,observation.handle AS profile_handle,observation.display_name AS profile_display_name,observation.profile_url,observation.public_json AS profile_public_json
    FROM input CROSS JOIN platform_accounts account LEFT JOIN creator_account_links link ON link.platform_account_id=account.id LEFT JOIN creators creator ON creator.id=link.creator_id LEFT JOIN platform_account_observations observation ON observation.rowid=(SELECT latest.rowid FROM platform_account_observations latest WHERE latest.platform_account_id=account.id ORDER BY latest.observed_at DESC,latest.rowid DESC LIMIT 1)
    WHERE account.platform=input.platform AND account.target_kind=input.target_kind AND account.external_id=input.external_id ORDER BY account.platform,account.target_kind,account.external_id,link.creator_id`).all(JSON.stringify(inputAccounts)).map(row => {
      const values = row.tags_json === null ? [] : tags.parse(parseJson(text.parse(row.tags_json)));
      return { key: { platform: row.platform as TransferAccount["platform"], target_kind: row.target_kind as TransferAccount["target_kind"], external_id: text.parse(row.external_id) }, account_id: text.parse(row.account_id), creator_id: nullableText(row.creator_id), link_state: nullableText(row.link_state), creator_score: row.creator_score === null ? null : Number(row.creator_score), ...(values.length ? { tags: values } : {}), follower_counts_json: nullableText(row.follower_counts_json), profile_handle: nullableText(row.profile_handle), profile_display_name: nullableText(row.profile_display_name), profile_url: nullableText(row.profile_url), profile_public_json: nullableText(row.profile_public_json) };
    });
  const handles = [...new Map(bundle.creators.flatMap(c => c.accounts).filter(a => a.link_state !== "rejected" && a.latest_profile?.handle != null).map(a => [JSON.stringify([a.platform, a.target_kind, asciiLower(a.latest_profile!.handle!)]), { platform: a.platform, target_kind: a.target_kind, handle: asciiLower(a.latest_profile!.handle!) }])).values()];
  const handle_owners = db.prepare("WITH input AS (SELECT json_extract(value,'$.platform') AS platform,json_extract(value,'$.target_kind') AS target_kind,json_extract(value,'$.handle') AS handle FROM json_each(?)) SELECT DISTINCT account.platform,account.target_kind,lower(observation.handle) AS handle,link.creator_id FROM creator_account_links link JOIN platform_accounts account ON account.id=link.platform_account_id JOIN platform_account_observations observation ON observation.platform_account_id=account.id WHERE link.state!='rejected' AND observation.handle IS NOT NULL AND account.platform||':'||account.target_kind||':'||lower(observation.handle) IN (SELECT input.platform||':'||input.target_kind||':'||lower(input.handle) FROM input) ORDER BY 1,2,3,4").all(JSON.stringify(handles)).map(row => ({ platform: text.parse(row.platform), target_kind: text.parse(row.target_kind), handle: text.parse(row.handle), creator_id: text.parse(row.creator_id) }));
  const contactInputs = [...new Map(bundle.creators.flatMap(c => c.contact_routes).map(r => [contactKey(r), { channel: r.channel, kind: r.locator_kind, normalized_value: r.normalized_value }])).values()];
  const contacts = db.prepare("WITH input AS (SELECT json_extract(value,'$.channel') AS channel,json_extract(value,'$.kind') AS kind,json_extract(value,'$.normalized_value') AS normalized_value FROM json_each(?)) SELECT locator.channel,locator.kind,locator.normalized_value,locator.id AS locator_id,route.creator_id FROM input CROSS JOIN contact_locators locator LEFT JOIN creator_contact_routes route ON route.locator_id=locator.id WHERE locator.channel=input.channel AND locator.kind=input.kind AND locator.normalized_value=input.normalized_value ORDER BY 1,2,3,5").all(JSON.stringify(contactInputs)).map(row => ({ key: { channel: text.parse(row.channel), kind: text.parse(row.kind), normalized_value: text.parse(row.normalized_value) }, locator_id: text.parse(row.locator_id), creator_id: nullableText(row.creator_id) }));
  return { domain_empty, accounts, handle_owners, contacts };
}
export const avatarKey = { x: "profile_image_url", instagram: "profile_pic_url", telegram: "photo_url", youtube: "thumbnail_url", tiktok: "profile_image" } as const;
export function incomingProfile(a: TransferAccount): ProfileFacts | null { return a.latest_profile === null ? null : { handle: a.latest_profile.handle, display_name: a.latest_profile.display_name, profile_url: a.latest_profile.profile_url, public_data: a.latest_profile.avatar_url === null ? {} : { [avatarKey[a.platform]]: a.latest_profile.avatar_url } }; }
function mergeProfile(account: TransferAccount, row: BasisAccount | undefined): ProfileFacts | null {
  const incoming = incomingProfile(account);
  if (!incoming || incoming.handle === null && incoming.display_name === null && incoming.profile_url === null && JSON.stringify(incoming.public_data) === "{}") return !row && account.source_kind !== "initial_platform_account" ? { handle: null, display_name: null, profile_url: null, public_data: {} } : null;
  const current = row?.profile_public_json == null ? null : { handle: row.profile_handle, display_name: row.profile_display_name, profile_url: row.profile_url, public_data: parseStoredJson(row.profile_public_json) };
  let public_data = current?.public_data ?? incoming.public_data;
  if (current) { const key = avatarKey[account.platform], object = z.object({}).catchall(z.unknown()), old = object.safeParse(isLosslessNumber(current.public_data) ? null : current.public_data), added = object.safeParse(incoming.public_data); if (old.success && typeof old.data[key] === "string") public_data = current.public_data; else if (added.success && typeof added.data[key] === "string") public_data = { ...(old.success ? old.data : {}), [key]: added.data[key] }; }
  const merged = { handle: current?.handle ?? incoming.handle, display_name: current?.display_name ?? incoming.display_name, profile_url: current?.profile_url ?? incoming.profile_url, public_data };
  return JSON.stringify(merged) === JSON.stringify(current) ? null : merged;
}
export function planImport(basis: ImportBasis, bundle: TransferBundle): { plan: ImportPlan | null; conflicts: ImportConflict[] } {
  const impact = emptyImpact(); impact.creator_inputs = bundle.creator_count;
  const conflicts: ImportConflict[] = [], items: PlanItem[] = [], activeOwners = new Map<string, string>(), targets = new Map<string, string>();
  const conflict = (kind: string, c: TransferCreator, ids: string[], account: TransferAccount | null = null) => { conflicts.push({ kind, incoming_creator_key: c.record_key, account: account === null ? null : { platform: account.platform, target_kind: account.target_kind, external_id: account.external_id }, local_creator_ids: [...new Set(ids)].sort(compareText) }); };
  if (basis.domain_empty) {
    impact.creators_created = bundle.creator_count; impact.platform_accounts_created = new Set(bundle.creators.flatMap(c => c.accounts).map(accountKey)).size;
    for (const c of bundle.creators) { impact.contact_routes_created += c.contact_routes.length; for (const a of c.accounts) { impact[`${a.link_state}_links_created`]++; if (a.latest_profile !== null || a.source_kind !== "initial_platform_account") impact.profile_observations_created++; } items.push({ incoming: c, target_creator_id: c.record_key, resolution: "new", create_creator: true, fill_score: null, follower_counts_after: null, accounts: [], contacts: [], changed: true }); }
    return { plan: { content_sha256: bundle.content_sha256, exact_restore: true, creators: items, impact }, conflicts };
  }
  for (const c of bundle.creators) for (const a of c.accounts.filter(a => a.link_state !== "rejected")) { const key = accountKey(a), owner = activeOwners.get(key); if (owner && owner !== c.record_key) conflict("incoming_account_claimed_by_multiple_creators", c, [], a); activeOwners.set(key, c.record_key); }
  if (conflicts.length) return { plan: null, conflicts };
  const resolved: { c: TransferCreator; id: string; owner: string | undefined }[] = [];
  for (const c of bundle.creators) {
    const keys = new Set(c.accounts.filter(a => a.link_state !== "rejected").map(accountKey)), owners = [...new Set(basis.accounts.filter(a => keys.has(accountKey(a.key)) && a.link_state !== "rejected" && a.creator_id !== null).map(a => a.creator_id!))].sort(compareText);
    if (owners.length > 1) { conflict("incoming_creator_resolves_to_multiple_local_creators", c, owners); continue; }
    const owner = owners[0];
    if (!owner) {
      const handles = new Set(c.accounts.filter(a => a.link_state !== "rejected" && a.latest_profile?.handle != null).map(a => JSON.stringify([a.platform, a.target_kind, asciiLower(a.latest_profile!.handle!)])));
      const possible = basis.handle_owners.filter(h => handles.has(JSON.stringify([h.platform, h.target_kind, h.handle]))).map(h => h.creator_id);
      if (possible.length) { conflict("possible_handle_match", c, possible); continue; }
      const contacts = new Set(c.contact_routes.map(contactKey)), possibleContacts = basis.contacts.filter(r => contacts.has(contactKey(r.key)) && r.creator_id !== null).map(r => r.creator_id!);
      if (possibleContacts.length) { conflict("possible_contact_match", c, possibleContacts); continue; }
    }
    resolved.push({ c, id: owner ?? c.record_key, owner });
  }
  for (const { c, id } of resolved) { const previous = targets.get(id); targets.set(id, c.record_key); if (previous && previous !== c.record_key) conflict("incoming_creator_resolves_to_multiple_local_creators", c, [id]); }
  if (conflicts.length) return { plan: null, conflicts };
  for (const { c, id, owner } of resolved) {
    const existing = owner ? basis.accounts.find(a => a.creator_id === owner) : undefined;
    if (owner && !existing) throw new Error("Resolved Creator facts missing");
    const existingTags = existing?.tags ?? [], oldFollowers = existing?.follower_counts_json == null ? {} : followers.parse(parseJson(existing.follower_counts_json));
    let tags_after: string[] | undefined;
    if (existing && c.tags.length && JSON.stringify(existingTags) !== JSON.stringify(c.tags)) { if (!existingTags.length) { tags_after = c.tags; impact.creator_tags_filled++; } else conflict("creator_tags_conflict", c, [id]); }
    const fill_score = existing && existing.creator_score === null ? c.score : null; if (fill_score !== null) impact.creator_scores_filled++;
    let follower_counts_after: TransferCreator["follower_counts"] | null = null;
    if (existing) { const merged = { ...oldFollowers }; for (const p of Object.keys(c.follower_counts) as (keyof typeof c.follower_counts)[]) if (merged[p] === undefined) { merged[p] = c.follower_counts[p]!; impact.follower_counts_filled++; follower_counts_after = merged; } if (follower_counts_after) follower_counts_after = Object.fromEntries(platform.options.filter(p => merged[p] !== undefined).map(p => [p, merged[p]])); }
    const accounts: AccountPlan[] = [], contacts: ContactPlan[] = [];
    for (const a of c.accounts) {
      const rows = basis.accounts.filter(row => accountKey(row.key) === accountKey(a)), first = rows[0], links = rows.filter(r => r.creator_id !== null), same = links.find(r => r.creator_id === id), rejected = links.find(r => r.link_state === "rejected");
      if (a.link_state !== "rejected" && rejected) { conflict("rejected_link_would_be_revived", c, [rejected.creator_id!], a); continue; }
      const foreign = links.filter(r => r.creator_id !== id && r.link_state !== "rejected"); if (a.link_state !== "rejected" && foreign.length) { conflict("account_owned_by_another_creator", c, foreign.map(r => r.creator_id!), a); continue; }
      if (a.link_state === "rejected" && same && same.link_state !== "rejected") { conflict("incoming_rejected_link_conflicts_with_active_local_link", c, [id], a); continue; }
      const profile_to_write = mergeProfile(a, first), create_link = !same, promote_to_confirmed = same?.link_state === "claimed" && a.link_state === "confirmed";
      if (!first) impact.platform_accounts_created++; if (create_link) impact[`${a.link_state}_links_created`]++; if (promote_to_confirmed) impact.links_promoted_to_confirmed++; if (profile_to_write) impact.profile_observations_created++;
      accounts.push({ incoming: a, existing_account_id: first?.account_id ?? null, existing_link_state: same?.link_state ?? null, create_link, promote_to_confirmed, profile_to_write });
    }
    for (const route of c.contact_routes) { const rows = basis.contacts.filter(r => contactKey(r.key) === contactKey(route)), foreign = rows.filter(r => r.creator_id !== null && r.creator_id !== id); if (foreign.length) { conflict("contact_route_owned_by_another_creator", c, foreign.map(r => r.creator_id!)); continue; } if (rows.some(r => r.creator_id === id)) continue; impact.contact_routes_created++; contacts.push({ incoming: route, target_locator_id: rows[0]?.locator_id ?? "", create_locator: !rows.length, create_route: true }); }
    const create_creator = !owner, changed = create_creator || tags_after !== undefined || fill_score !== null || follower_counts_after !== null || accounts.some(a => a.existing_account_id === null || a.create_link || a.promote_to_confirmed || a.profile_to_write !== null) || contacts.some(r => r.create_locator || r.create_route);
    if (create_creator) impact.creators_created++; else impact.creators_merged_by_public_identity++; if (!changed) impact.creators_unchanged++;
    items.push({ incoming: c, target_creator_id: id, resolution: owner ? "public_identity" : "new", create_creator, ...(tags_after ? { tags_after } : {}), fill_score, follower_counts_after, accounts, contacts, changed });
  }
  return { plan: conflicts.length ? null : { content_sha256: bundle.content_sha256, exact_restore: false, creators: items, impact }, conflicts };
}
export function planFingerprint(plan: ImportPlan, basis: ImportBasis): string {
  const original = JSON.stringify({ plan, basis }), encoded = new Map<string, string>();
  let raw = JSON.stringify({ plan, basis }, (key, value: unknown) => {
    if (key === "notes" && value == null) return undefined;
    if (key !== "public_data") return value;
    const keys = new Set<string>();
    const collect = (item: unknown): void => { if (Array.isArray(item)) item.forEach(collect); else if (item && typeof item === "object") for (const [name, child] of Object.entries(item)) { keys.add(name); collect(child); } };
    collect(value);
    const serialized = stringify(value, [...keys].sort(compareText)); if (serialized === undefined) throw new Error("Profile public data cannot be encoded");
    let token = randomUUID(); while (original.includes(token)) token = randomUUID(); encoded.set(token, serialized); return token;
  });
  for (const [token, value] of encoded) raw = raw.replace(JSON.stringify(token), value);
  raw = raw.replaceAll(',"tags":[]', "").replaceAll(',"creator_tags_filled":0', "");
  return digestRaw(raw);
}
const digestRaw = (raw: string) => `sha256:${createHash("sha256").update(raw).digest("hex")}`;
