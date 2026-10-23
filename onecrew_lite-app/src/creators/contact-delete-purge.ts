import type { DatabaseSync } from "node:sqlite";
import { strings } from "./contact-delete-plan.ts";
import type { DeletePlan } from "./contact-delete-plan.ts";

export function purgeContact(db: DatabaseSync, plan: DeletePlan): void {
  const r = plan.route.route.id, l = plan.route.locator.id;
  const conversations = strings(db, "SELECT conversation_id FROM conversation_contact_routes WHERE contact_route_id=? ORDER BY conversation_id", r);
  const sources = strings(db, "SELECT DISTINCT source_id FROM locator_observations WHERE locator_id=? ORDER BY source_id", l);
  const targets = strings(db, "SELECT DISTINCT resolved_target_id FROM locator_resolutions WHERE locator_id=? ORDER BY resolved_target_id", l);
  for (const table of ["idempotency_records", "touchpoints", "audit_events", "email_drafts"]) db.prepare(`DELETE FROM ${table} WHERE contact_route_id=?`).run(r);
  for (const id of plan.email.thread_ids) {
    db.prepare("DELETE FROM email_recipients WHERE email_id IN(SELECT id FROM emails WHERE thread_id=?)").run(id);
    db.prepare("DELETE FROM emails WHERE thread_id=?").run(id);
    db.prepare("DELETE FROM email_threads WHERE id=?").run(id);
  }
  for (const id of conversations) {
    db.prepare("DELETE FROM messages WHERE conversation_id=?").run(id);
    db.prepare("DELETE FROM conversation_contact_routes WHERE conversation_id=?").run(id);
    db.prepare("DELETE FROM conversations WHERE id=?").run(id);
  }
  for (const table of ["identity_conflicts", "locator_resolutions", "locator_observations"]) db.prepare(`DELETE FROM ${table} WHERE locator_id=?`).run(l);
  const detached = Number(db.prepare("UPDATE creator_origins SET source_contact_route_id=NULL WHERE source_contact_route_id=?").run(r).changes);
  if (detached !== plan.impact.creator_origins_detached) throw new Error(`contact route delete creator origin detach count changed: expected ${plan.impact.creator_origins_detached}, found ${detached}`);
  db.prepare("DELETE FROM creator_contact_routes WHERE id=?").run(r);
  for (const id of sources) db.prepare("DELETE FROM observation_sources WHERE id=?1 AND NOT EXISTS(SELECT 1 FROM locator_observations WHERE source_id=?1) AND NOT EXISTS(SELECT 1 FROM platform_account_observations WHERE source_id=?1) AND NOT EXISTS(SELECT 1 FROM creator_origins WHERE source_id=?1)").run(id);
  db.prepare("DELETE FROM contact_locators WHERE id=?1 AND NOT EXISTS(SELECT 1 FROM creator_contact_routes WHERE locator_id=?1) AND NOT EXISTS(SELECT 1 FROM locator_observations WHERE locator_id=?1) AND NOT EXISTS(SELECT 1 FROM locator_resolutions WHERE locator_id=?1) AND NOT EXISTS(SELECT 1 FROM identity_conflicts WHERE locator_id=?1) AND NOT EXISTS(SELECT 1 FROM email_recipients WHERE locator_id=?1) AND NOT EXISTS(SELECT 1 FROM audit_events WHERE contact_locator_id=?1)").run(l);
  for (const id of targets) db.prepare("DELETE FROM resolved_targets WHERE id=?1 AND NOT EXISTS(SELECT 1 FROM locator_resolutions WHERE resolved_target_id=?1) AND NOT EXISTS(SELECT 1 FROM conversations WHERE resolved_target_id=?1) AND NOT EXISTS(SELECT 1 FROM messages WHERE sender_target_id=?1) AND NOT EXISTS(SELECT 1 FROM identity_conflicts WHERE resolved_target_id=?1) AND NOT EXISTS(SELECT 1 FROM audit_events WHERE resolved_target_id=?1) AND NOT EXISTS(SELECT 1 FROM creator_origins WHERE source_resolved_target_id=?1)").run(id);
}
