import { randomUUID } from "node:crypto";
import { stringify } from "lossless-json";
import type { DatabaseSync, SQLInputValue } from "node:sqlite";
import { accountKey, incomingProfile, type AccountPlan, type ContactPlan, type ImportPlan } from "./import-plan.ts";
import { compareText, type TransferBundle } from "./transfer-contracts.ts";

export function applyImport(db: DatabaseSync, bundle: TransferBundle, plan: ImportPlan, at: number): void {
  const run = (sql: string, ...values: SQLInputValue[]) => db.prepare(sql).run(...values);
  const sourceIds = new Map<string, string>(), accountIds = new Map<string, string>(), profileWritten = new Set<string>();
  const source = (kind: string) => { let id = sourceIds.get(kind); if (!id) { id = randomUUID(); sourceIds.set(kind, id); } return id; };
  const sourceNotes: Record<string, string> = {}, creators = plan.creators.map(item => ({ item, id: item.create_creator ? randomUUID() : item.target_creator_id }));
  for (const c of creators) if (c.item.incoming.notes != null) { source("creator_import"); sourceNotes[c.id] = c.item.incoming.notes; }
  const maxCapture = Math.max(0, ...bundle.creators.flatMap(c => c.accounts).map(a => a.capture_order));
  const accountInserts: { id: string; incoming: AccountPlan["incoming"]; created: number }[] = [];
  const links: { id: string; creator: string; account: string; plan: AccountPlan }[] = [], profiles: { account: string; source: string; facts: NonNullable<AccountPlan["profile_to_write"]> }[] = [];
  for (const { item, id } of creators) {
    const c = item.incoming;
    if (item.create_creator) {
      run("INSERT INTO creators (id,display_name,notes,created_at,updated_at,archived_at,score,assessment,follower_counts_json,pinned_at,group_id,tags_json) VALUES (?,?,?,?,?,?,?,NULL,?,?,NULL,?)", id, c.display_name, c.notes ?? null, at, at, c.relationship.stage === "archived" ? at : null, c.score, JSON.stringify(c.follower_counts), c.pinned ? at : null, JSON.stringify(c.tags));
      run("INSERT INTO creator_relationships (creator_id,stage,last_touch_at,created_at,updated_at) VALUES (?,?,NULL,?,?)", id, c.relationship.stage, at, at);
    } else if (item.fill_score !== null || item.follower_counts_after !== null || item.tags_after !== undefined) run("UPDATE creators SET score=coalesce(score,?),tags_json=coalesce(?,tags_json),follower_counts_json=coalesce(?,follower_counts_json),updated_at=? WHERE id=?", item.fill_score, item.tags_after === undefined ? null : JSON.stringify(item.tags_after), item.follower_counts_after === null ? null : JSON.stringify(item.follower_counts_after), at, id);
    const accounts: AccountPlan[] = plan.exact_restore ? c.accounts.map(a => ({ incoming: a, existing_account_id: null, existing_link_state: null, create_link: true, promote_to_confirmed: false, profile_to_write: a.latest_profile !== null || a.source_kind !== "initial_platform_account" ? incomingProfile(a) ?? { handle: null, display_name: null, profile_url: null, public_data: {} } : null })) : item.accounts;
    for (const a of accounts) {
      const key = accountKey(a.incoming); let account = a.existing_account_id ?? accountIds.get(key);
      if (!account) { account = randomUUID(); accountInserts.push({ id: account, incoming: a.incoming, created: plan.exact_restore ? Math.max(0, at - Math.max(0, maxCapture - a.incoming.capture_order)) : at }); }
      accountIds.set(key, account); links.push({ id: randomUUID(), creator: id, account, plan: a });
      if (a.profile_to_write !== null && (!plan.exact_restore || !profileWritten.has(key))) { profiles.push({ account, source: source(a.incoming.source_kind), facts: a.profile_to_write }); profileWritten.add(key); }
    }
  }
  accountInserts.sort((a, b) => a.incoming.capture_order - b.incoming.capture_order || compareText(a.incoming.platform, b.incoming.platform) || compareText(a.incoming.target_kind, b.incoming.target_kind) || compareText(a.incoming.external_id, b.incoming.external_id));
  for (const a of accountInserts) run("INSERT INTO platform_accounts (id,platform,target_kind,external_id,created_at,updated_at) VALUES (?,?,?,?,?,?)", a.id, a.incoming.platform, a.incoming.target_kind, a.incoming.external_id, a.created, at);
  for (const l of links) { if (l.plan.create_link) run("INSERT INTO creator_account_links (id,creator_id,platform_account_id,state,confidence,created_at,updated_at) VALUES (?,?,?,?,?,?,?)", l.id, l.creator, l.account, l.plan.incoming.link_state, l.plan.incoming.confidence, at, at); if (l.plan.promote_to_confirmed) run("UPDATE creator_account_links SET state='confirmed',confidence=?,updated_at=? WHERE creator_id=? AND platform_account_id=? AND state='claimed'", l.plan.incoming.confidence, at, l.creator, l.account); }
  const observations: { id: string; locator: string; source: string; creator: string; value: string; observed: number }[] = [];
  for (const { item, id } of creators) {
    const contacts: ContactPlan[] = plan.exact_restore ? item.incoming.contact_routes.map(r => ({ incoming: r, target_locator_id: "", create_locator: true, create_route: true })) : item.contacts;
    const maxSave = Math.max(0, ...item.incoming.contact_routes.map(r => r.save_order));
    for (const r of contacts) {
      const locator = r.create_locator ? randomUUID() : r.target_locator_id, created = Math.max(0, at - Math.max(0, maxSave - r.incoming.save_order));
      if (r.create_locator) run("INSERT INTO contact_locators (id,channel,kind,normalized_value,display_value,created_at,updated_at) VALUES (?,?,?,?,?,?,?)", locator, r.incoming.channel, r.incoming.locator_kind, r.incoming.normalized_value, r.incoming.display_value, created, at);
      if (db.prepare("SELECT 1 FROM creator_contact_routes WHERE locator_id=? AND creator_id!=? LIMIT 1").get(locator, id)) throw new Error("Materialized contact route ownership changed before apply");
      if (r.create_route) run("INSERT INTO creator_contact_routes (id,creator_id,locator_id,confidence,created_at,updated_at) VALUES (?,?,?,?,?,?)", randomUUID(), id, locator, r.incoming.confidence, created, at);
      for (const [index, kind] of r.incoming.source_kinds.entries()) observations.push({ id: randomUUID(), locator, source: source(kind), creator: id, value: r.incoming.display_value, observed: Math.max(0, at - index) });
    }
  }
  const metadata = JSON.stringify({ format: bundle.format, format_version: bundle.format_version, source_notes: Object.fromEntries(Object.entries(sourceNotes).sort((a, b) => compareText(a[0], b[0]))) });
  for (const [kind, id] of sourceIds) run("INSERT INTO observation_sources (id,kind,platform,source_url,external_ref,account_ref_id,metadata_json,observed_at) VALUES (?,?,NULL,NULL,?,NULL,?,?)", id, kind, bundle.content_sha256, metadata, at);
  for (const p of profiles) { const publicJson = stringify(p.facts.public_data); if (publicJson === undefined) throw new Error("Imported profile public data cannot be encoded"); run("INSERT INTO platform_account_observations (id,platform_account_id,source_id,handle,display_name,profile_url,public_json,observed_at) VALUES (?,?,?,?,?,?,?,?)", randomUUID(), p.account, p.source, p.facts.handle, p.facts.display_name, p.facts.profile_url, publicJson, at); }
  for (const o of observations) run("INSERT INTO locator_observations (id,locator_id,source_id,creator_id,platform_account_id,observed_value,observed_at) VALUES (?,?,?,?,NULL,?,?)", o.id, o.locator, o.source, o.creator, o.value, o.observed);
}
