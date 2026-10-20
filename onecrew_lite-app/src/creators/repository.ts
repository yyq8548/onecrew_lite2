import { randomUUID } from "node:crypto";
import { stringify } from "lossless-json";
import type { DatabaseSync, SQLInputValue } from "node:sqlite";
import { validateCreatorSchema } from "../storage/creator-schema.ts";
import { creatorOutput, manualCreatorInput, updateCreatorInput, trim, text, type ManualCreatorInput } from "./contracts.ts";
import { normalizeContact } from "./normalization.ts";
import { creatorDetail, contactRoutes } from "./detail.ts";
import { listCreators } from "./list.ts";
import { createGroup, deleteGroup, groupSetInput, listGroups, moveGroup, updateGroup } from "./groups.ts";
import { pinCreatorInput, stageCreatorInput } from "./contracts.ts";
import { addContact, resolveIdentity, updateContact } from "./contacts.ts";
import { confirmContactDelete, prepareContactDelete } from "./contact-delete.ts";
import { exportSnapshot } from "./transfer-export.ts";
import { cancelImport, confirmImport, prepareImport } from "./import.ts";
import type { TransferBundle } from "./transfer-contracts.ts";
import { batchExisting, batchFingerprint, batchInput, preflightBatch } from "./batch.ts";
import { prepareCandidateRows } from "./candidate-prepare.ts";
import type { CandidateIntent } from "./candidate-contracts.ts";
import { executeCandidateRows } from "./candidate-execute.ts";
import { decodeCandidateReceipt } from "./candidate-receipt.ts";
import type { SelectedCandidateBatch } from "./candidate-selection.ts";

export class CreatorRepository {
  readonly #db: DatabaseSync;
  constructor(db: DatabaseSync) { validateCreatorSchema(db); this.#db = db; }
  #transaction<T>(write: boolean, operation: () => T): T {
    this.#db.exec(write ? "BEGIN IMMEDIATE" : "BEGIN DEFERRED");
    let committed = false;
    try { const result = operation(); this.#db.exec("COMMIT"); committed = true; return result; }
    finally { if (!committed && this.#db.isTransaction) this.#db.exec("ROLLBACK"); }
  }
  get(creatorId: string) { const id = text.parse(creatorId); return this.#transaction(false, () => creatorDetail(this.#db, id)); }
  list(input: unknown) { return this.#transaction(false, () => listCreators(this.#db, input)); }
  groupList(input: unknown) { return this.#transaction(false, () => listGroups(this.#db, input)); }
  groupCreate(input: unknown) { return this.#transaction(true, () => createGroup(this.#db, input)); }
  groupUpdate(input: unknown) { return this.#transaction(true, () => updateGroup(this.#db, input)); }
  groupDelete(input: unknown) { return this.#transaction(true, () => deleteGroup(this.#db, input)); }
  groupMove(input: unknown) { return this.#transaction(true, () => moveGroup(this.#db, input)); }
  groupSet(input: unknown) { const parsed = groupSetInput.parse(input); return this.groupMove({ creator_ids: [parsed.creator_id], destination: parsed.destination }); }
  contactAdd(input: unknown) { return this.#transaction(true, () => addContact(this.#db, input)); }
  contactUpdate(input: unknown) { return this.#transaction(true, () => updateContact(this.#db, input)); }
  contactDeletePrepare(input: unknown) { return this.#transaction(true, () => prepareContactDelete(this.#db, input)); }
  contactDeleteConfirm(input: unknown) { return this.#transaction(true, () => confirmContactDelete(this.#db, input)); }
  identityResolve(input: unknown) { return this.#transaction(false, () => resolveIdentity(this.#db, input)); }
  candidatePrepare(input: readonly unknown[], intent: CandidateIntent) { return this.#transaction(false, () => prepareCandidateRows(this.#db, input, intent)); }
  candidateExecute(prepared: SelectedCandidateBatch, executionKey: string) { return executeCandidateRows(this.#db, prepared, executionKey, decodeCandidateReceipt); }
  exportSnapshot() { return this.#transaction(false, () => exportSnapshot(this.#db)); }
  importPrepare(bundle: TransferBundle) { return this.#transaction(true, () => prepareImport(this.#db, bundle)); }
  importConfirm(input: unknown) { return this.#transaction(true, () => confirmImport(this.#db, input)); }
  importCancel(input: unknown) { return this.#transaction(true, () => cancelImport(this.#db, input)); }
  batchCreate(input: unknown) {
    const parsed = batchInput.parse(input), key = trim(parsed.idempotency_key);
    if (!key || Buffer.byteLength(key) > 128) throw new Error("idempotency_key must contain 1-128 characters");
    const fingerprint = batchFingerprint(parsed);
    const claim = this.#transaction(true, () => {
      const preflight = preflightBatch(this.#db, parsed);
      const rejected = preflight.findIndex(item => !item.ok);
      if (parsed.validation === "strict" && rejected >= 0) { const item = preflight[rejected]!; if (!item.ok) throw new Error(`Creator batch item ${rejected} rejected: ${item.message}`); }
      const existing = batchExisting(this.#db, key, fingerprint); if (existing) return { existing, preflight };
      const at = Math.floor(Date.now() / 1000);
      this.#db.prepare("INSERT INTO idempotency_records (key,action,request_fingerprint,state,result_json,created_at,updated_at) VALUES (?,'creator_batch_create',?,'in_progress',NULL,?,?)").run(key, fingerprint, at, at);
      return { existing: null, preflight };
    });
    if (claim.existing) return claim.existing;
    let completed = false;
    // using 在写失败收据也失败时保留原异常，并通过原生 SuppressedError 暴露两项失败。
    using failedReceipt = { [Symbol.dispose]: () => { if (!completed) { const written = this.#db.prepare("UPDATE idempotency_records SET state='failed',result_json=?,updated_at=? WHERE key=? AND action='creator_batch_create' AND state='in_progress'").run(JSON.stringify({ message: "Creator batch database write did not complete" }), Math.floor(Date.now() / 1000), key).changes; if (written !== 1) throw new Error("Creator batch failed receipt was not persisted"); } } };
    const outcomes = this.#transaction(true, () => claim.preflight.map((item, input_index) => item.ok ? { kind: "accepted" as const, input_index, creator: this.#insertManual(item.item) } : { kind: "rejected" as const, input_index, message: item.message }));
    const result = { outcomes, replayed: false };
    const written = this.#db.prepare("UPDATE idempotency_records SET state='completed',result_json=?,updated_at=? WHERE key=? AND action='creator_batch_create' AND state='in_progress'").run(JSON.stringify(result), Math.floor(Date.now() / 1000), key).changes;
    if (written !== 1) throw new Error("Creator batch completion receipt was not persisted");
    completed = true;
    return result;
  }
  updateStage(input: unknown) {
    const parsed = stageCreatorInput.parse(input);
    if (parsed.stage === "archived") throw new Error("use onecrew_creator_archive to archive a Creator");
    return this.#transaction(true, () => {
      const current = this.#active(parsed.creator_id);
      if (current.relationship.stage === parsed.stage) return { relationship: current.relationship, changed: false };
      this.#db.prepare("UPDATE creator_relationships SET stage=?,updated_at=? WHERE creator_id=?").run(parsed.stage, Math.floor(Date.now() / 1000), parsed.creator_id);
      return { relationship: creatorDetail(this.#db, parsed.creator_id).relationship, changed: true };
    });
  }
  pin(input: unknown) {
    const parsed = pinCreatorInput.parse(input);
    return this.#transaction(true, () => {
      this.#active(parsed.creator_id);
      this.#db.prepare("UPDATE creators SET pinned_at=CASE WHEN ? THEN ? ELSE NULL END WHERE id=?").run(Number(parsed.pinned), Math.floor(Date.now() / 1000), parsed.creator_id);
      return null;
    });
  }
  archive(creatorId: string) {
    text.parse(creatorId);
    return this.#transaction(true, () => {
      const current = creatorDetail(this.#db, creatorId);
      if (current.creator.archived_at !== null && current.relationship.stage === "archived") return { creator: current.creator, changed: false };
      const at = Math.floor(Date.now() / 1000);
      this.#db.prepare("UPDATE creators SET archived_at=?,updated_at=? WHERE id=?").run(at, at, creatorId);
      this.#db.prepare("UPDATE creator_relationships SET stage='archived',updated_at=? WHERE creator_id=?").run(at, creatorId);
      return { creator: creatorOutput(this.#db.prepare("SELECT * FROM creators WHERE id=?").get(creatorId)), changed: true };
    });
  }
  #active(creatorId: string) {
    const current = creatorDetail(this.#db, creatorId);
    if (current.creator.archived_at !== null || current.relationship.stage === "archived") throw new Error("archived Creator cannot be used for outreach; delete it or use an active Creator");
    return current;
  }
  create(input: unknown) {
    const parsed = manualCreatorInput.parse(input);
    return this.#transaction(true, () => this.#insertManual(parsed));
  }
  #insertManual(parsed: ManualCreatorInput) {
    const canonical = parsed.contact_routes.map(route => ({ route, locator: normalizeContact(route.channel, route.value) }));
    const keys = canonical.map(({ locator }) => JSON.stringify([locator.channel, locator.kind, locator.normalized_value]));
    if (new Set(keys).size !== keys.length) throw new Error("duplicate contact route in Creator create request");
      const run = (sql: string, ...values: SQLInputValue[]) => this.#db.prepare(sql).run(...values);
      const creatorId = randomUUID(), sourceId = randomUUID(), at = Math.floor(Date.now() / 1000);
      run("INSERT INTO creators (id,display_name,notes,created_at,updated_at,archived_at,tags_json) VALUES (?,?,?,?,?,NULL,?)", creatorId, parsed.display_name, parsed.notes ?? null, at, at, JSON.stringify(parsed.tags ?? []));
      run("INSERT INTO creator_relationships (creator_id,stage,last_touch_at,created_at,updated_at) VALUES (?,'prospect',NULL,?,?)", creatorId, at, at);
      run("INSERT INTO observation_sources (id,kind,platform,source_url,external_ref,account_ref_id,metadata_json,observed_at) VALUES (?,'manual_entry',NULL,NULL,NULL,NULL,'{}',?)", sourceId, at);
      run("INSERT INTO creator_origins (id,kind,creator_id,source_id,discovered_at) VALUES (?,'manual_entry',?,?,?)", randomUUID(), creatorId, sourceId, at);
      for (const { route, locator } of canonical) {
        const owned = this.#db.prepare("SELECT 1 FROM contact_locators locator JOIN creator_contact_routes route ON route.locator_id=locator.id WHERE locator.channel=? AND locator.kind=? AND locator.normalized_value=? LIMIT 1").get(locator.channel, locator.kind, locator.normalized_value);
        if (owned) throw new Error("contact route already belongs to another Creator");
        const existing = this.#db.prepare("SELECT id FROM contact_locators WHERE channel=? AND kind=? AND normalized_value=?").get(locator.channel, locator.kind, locator.normalized_value);
        const locatorId = existing ? String(existing.id) : randomUUID();
        run("INSERT INTO contact_locators (id,channel,kind,normalized_value,display_value,created_at,updated_at) VALUES (?,?,?,?,?,?,?) ON CONFLICT(channel,kind,normalized_value) DO UPDATE SET display_value=excluded.display_value,updated_at=excluded.updated_at", locatorId, locator.channel, locator.kind, locator.normalized_value, locator.display_value, at, at);
        const routeSource = randomUUID(), routeId = randomUUID();
        const routePlatform = ["x", "instagram", "telegram", "youtube", "tiktok"].includes(route.channel) ? route.channel : null;
        run("INSERT INTO observation_sources (id,kind,platform,source_url,external_ref,account_ref_id,metadata_json,observed_at) VALUES (?,'manual_entry',?,?,NULL,NULL,'{}',?)", routeSource, routePlatform, route.source_url ?? null, at);
        run("INSERT INTO creator_contact_routes (id,creator_id,locator_id,confidence,created_at,updated_at) VALUES (?,?,?,'asserted',?,?)", routeId, creatorId, locatorId, at, at);
        run("INSERT INTO locator_observations (id,locator_id,source_id,creator_id,platform_account_id,observed_value,observed_at) VALUES (?,?,?,?,NULL,?,?)", randomUUID(), locatorId, routeSource, creatorId, trim(route.value), at);
        run("INSERT INTO audit_events (id,kind,creator_id,contact_locator_id,contact_route_id,payload_json,created_at) VALUES (?,'creator_contact_route_added',?,?,?,?,?)", randomUUID(), creatorId, locatorId, routeId, JSON.stringify({ route_id: routeId, source: "manual_entry" }), at);
      }
      return { creator_id: creatorId, display_name: parsed.display_name, tags: parsed.tags ?? [], stage: "prospect" as const, created_at: at, contact_routes: contactRoutes(this.#db, creatorId, true) };
  }
  update(input: unknown) {
    const parsed = updateCreatorInput.parse(input);
    return this.#transaction(true, () => {
      const current = this.#db.prepare("SELECT * FROM creators WHERE id=?").get(parsed.creator_id);
      if (!current) throw new Error("creator not found");
      const creator = creatorOutput(current);
      const relationship = this.#db.prepare("SELECT stage FROM creator_relationships WHERE creator_id=?").get(parsed.creator_id);
      if (creator.archived_at !== null || relationship?.stage === "archived") throw new Error("archived Creator cannot be used for outreach; delete it or use an active Creator");
      this.#db.prepare("UPDATE creators SET display_name=?,notes=?,tags_json=?,score=?,assessment=?,follower_counts_json=?,updated_at=? WHERE id=?").run(parsed.display_name ?? creator.display_name, parsed.notes ?? creator.notes, JSON.stringify(parsed.tags ?? creator.tags), parsed.score ?? creator.score, parsed.assessment ?? creator.assessment, stringify({ ...creator.follower_counts, ...parsed.follower_counts })!, Math.floor(Date.now() / 1000), parsed.creator_id);
      return creatorDetail(this.#db, parsed.creator_id);
    });
  }
  delete(creatorId: string) {
    text.parse(creatorId);
    return this.#transaction(true, () => {
      if (!this.#db.prepare("SELECT 1 FROM creators WHERE id=?").get(creatorId)) throw new Error("creator not found");
      const pending = this.#db.prepare(`SELECT EXISTS(SELECT 1 FROM idempotency_records WHERE action IN ('relationship_update_v1','content_reply_send_v1','direct_message_send_v1') AND state IN ('in_progress','remote_started','remote_accepted','remote_verified') AND CASE WHEN json_valid(result_json) AND json_extract(result_json,'$.version')=1 AND json_type(result_json,'$.state.identity.creator_id')='text' AND json_type(result_json,'$.state.identity.operator_account_ref_id')='text' AND length(trim(json_extract(result_json,'$.state.identity.creator_id')))>0 AND length(trim(json_extract(result_json,'$.state.identity.operator_account_ref_id')))>0 AND json_extract(result_json,'$.state.fingerprint')=request_fingerprint AND request_fingerprint GLOB 'sha256:v1:*' THEN json_extract(result_json,'$.state.identity.creator_id')=? ELSE 1 END) AS pending`).get(creatorId)!;
      if (pending.pending !== 0) throw new Error("Creator has an unfinished outreach action; reconcile it before deletion");
      const run = (sql: string) => Number(this.#db.prepare(sql).run(creatorId).changes);
      const routes = "SELECT id FROM creator_contact_routes WHERE creator_id=?1";
      run(`DELETE FROM conversation_contact_routes WHERE contact_route_id IN (${routes})`);
      run(`DELETE FROM creator_contact_route_dependencies WHERE parent_contact_route_id IN (${routes}) OR child_contact_route_id IN (${routes})`);
      let cleared = 0;
      for (const table of ["email_drafts", "email_threads", "audit_events"]) cleared += run(`UPDATE ${table} SET creator_id=NULL,contact_route_id=NULL WHERE creator_id=?1 OR contact_route_id IN (${routes})`);
      cleared += run(`UPDATE idempotency_records SET contact_route_id=NULL WHERE contact_route_id IN (${routes})`);
      cleared += run(`UPDATE creator_origins SET source_creator_id=NULL,source_contact_route_id=NULL WHERE source_creator_id=?1 OR source_contact_route_id IN (${routes})`);
      cleared += run("UPDATE locator_observations SET creator_id=NULL WHERE creator_id=?1 AND platform_account_id IS NOT NULL");
      run(`UPDATE touchpoints SET contact_route_id=NULL WHERE contact_route_id IN (${routes})`);
      for (const table of ["locator_observations", "identity_conflicts", "creator_contact_route_delete_operations"]) run(`DELETE FROM ${table} WHERE creator_id=?1`);
      const removed_contact_routes = run("DELETE FROM creator_contact_routes WHERE creator_id=?1");
      const removed_touchpoints = run("DELETE FROM touchpoints WHERE creator_id=?1");
      const removed_origins = run("DELETE FROM creator_origins WHERE creator_id=?1");
      const removed_account_links = run("DELETE FROM creator_account_links WHERE creator_id=?1");
      run("DELETE FROM creator_relationships WHERE creator_id=?1");
      run("DELETE FROM creators WHERE id=?1");
      return { creator_id: creatorId, removed_contact_routes, removed_touchpoints, removed_origins, removed_account_links, cleared_references: cleared };
    });
  }
}
