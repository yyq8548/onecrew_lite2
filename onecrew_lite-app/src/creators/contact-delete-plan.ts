import type { DatabaseSync, SQLInputValue } from "node:sqlite";
import { z } from "zod";
import { asciiLower, channel, integer, text, trim } from "./contracts.ts";
import type { ContactRoute } from "./contacts.ts";
import { blockingArtifacts } from "./contact-delete-blocking.ts";

export const impactKeys = ["contact_routes", "dependencies", "email_threads", "email_messages", "email_recipients", "email_drafts", "direct_message_conversations", "direct_messages", "outreach_touchpoints", "audit_events", "idempotency_records", "target_resolutions", "identity_conflicts", "observations", "contact_locators", "observation_sources", "resolved_targets", "creator_origins_detached", "local_records_total"] as const;
export type DeleteImpact = Record<typeof impactKeys[number], number>;
export const impactSchema = z.object(Object.fromEntries(impactKeys.map(key => [key, integer])) as Record<typeof impactKeys[number], typeof integer>).strict();
export function count(db: DatabaseSync, sql: string, ...values: SQLInputValue[]): number { return integer.parse(Object.values(db.prepare(sql).get(...values)!)[0]); }
export function strings(db: DatabaseSync, sql: string, ...values: SQLInputValue[]): string[] { return db.prepare(sql).all(...values).map(row => text.parse(Object.values(row)[0])); }
export function emailAddress(raw: string): string | null { const value = trim(raw), start = value.lastIndexOf("<"), end = value.lastIndexOf(">"); const address = asciiLower(trim(start >= 0 && end >= 0 && start < end ? value.slice(start + 1, end) : value)); const parts = address.split("@"); return parts.length === 2 && parts[0] && parts[1] ? address : null; }
export function emailPlan(db: DatabaseSync, route: ContactRoute) {
  const target = route.locator.channel === "email" ? emailAddress(route.locator.normalized_value) : null;
  if (route.locator.channel === "email" && target === null) throw new Error("stored email contact route is invalid");
  const plan = { thread_ids: [] as string[], email_ids: [] as string[], thread_fingerprints: [] as string[], email_fingerprints: [] as string[], recipient_fingerprints: [] as string[], shared_threads: 0 };
  const threads = db.prepare("SELECT thread.id,thread.creator_id,thread.contact_route_id,mailbox.email_address,thread.updated_at FROM email_threads thread JOIN mailbox_refs mailbox ON mailbox.id=thread.mailbox_ref_id ORDER BY thread.id").all();
  for (const value of threads) {
    const thread = z.object({ id: text, creator_id: text.nullable(), contact_route_id: text.nullable(), email_address: text, updated_at: integer }).parse(value);
    const participants = db.prepare("SELECT recipient.address,recipient.locator_id FROM emails email JOIN email_recipients recipient ON recipient.email_id=email.id WHERE email.thread_id=? ORDER BY email.thread_id,recipient.email_id,recipient.recipient_kind,recipient.address").all(thread.id).map(row => z.object({ address: text, locator_id: text.nullable() }).parse(row));
    const references = thread.contact_route_id === route.route.id || participants.some(participant => participant.locator_id === route.locator.id || target !== null && emailAddress(participant.address) === target);
    if (!references) continue;
    const mailbox = emailAddress(thread.email_address);
    let sawTarget = false;
    const participantsExclusive = target !== null && mailbox !== null && participants.every(participant => { const address = emailAddress(participant.address); if (address === null || participant.locator_id !== null && (participant.locator_id !== route.locator.id || address !== target)) return false; if (address === mailbox) return true; if (address === target) { sawTarget = true; return true; } return false; }) && sawTarget;
    if (thread.creator_id !== route.route.creator_id || thread.contact_route_id !== null && thread.contact_route_id !== route.route.id || !participantsExclusive) { plan.shared_threads++; continue; }
    plan.thread_fingerprints.push(`${thread.id}|${thread.creator_id ?? ""}|${thread.contact_route_id ?? ""}|${thread.updated_at}`);
    plan.email_ids.push(...strings(db, "SELECT id FROM emails WHERE thread_id=? ORDER BY id", thread.id));
    plan.email_fingerprints.push(...strings(db, "SELECT printf('%s|%s|%s',id,delivery_status,occurred_at) FROM emails WHERE thread_id=? ORDER BY id", thread.id));
    plan.recipient_fingerprints.push(...strings(db, "SELECT printf('%s|%s|%s|%s',recipient.email_id,recipient.recipient_kind,recipient.address,COALESCE(recipient.locator_id,'')) FROM email_recipients recipient JOIN emails email ON email.id=recipient.email_id WHERE email.thread_id=? ORDER BY recipient.email_id,recipient.recipient_kind,recipient.address", thread.id));
    plan.thread_ids.push(thread.id);
  }
  return plan;
}
export type EmailPlan = ReturnType<typeof emailPlan>;
function orphanLocator(db: DatabaseSync, route: ContactRoute, email: EmailPlan) {
  if (count(db, "SELECT COUNT(*) FROM creator_contact_routes WHERE locator_id=? AND id!=?", route.locator.id, route.route.id)) return 0;
  if (strings(db, "SELECT DISTINCT email.thread_id FROM email_recipients recipient JOIN emails email ON email.id=recipient.email_id WHERE recipient.locator_id=? ORDER BY email.thread_id", route.locator.id).some(id => !email.thread_ids.includes(id))) return 0;
  return Number(count(db, "SELECT COUNT(*) FROM audit_events WHERE contact_locator_id=? AND contact_route_id IS NOT ?", route.locator.id, route.route.id) === 0);
}
function orphanSources(db: DatabaseSync, locator: string) { return count(db, `SELECT COUNT(*) FROM observation_sources source WHERE EXISTS(SELECT 1 FROM locator_observations owned WHERE owned.source_id=source.id AND owned.locator_id=?1) AND NOT EXISTS(SELECT 1 FROM locator_observations sibling WHERE sibling.source_id=source.id AND sibling.locator_id!=?1) AND NOT EXISTS(SELECT 1 FROM platform_account_observations platform_observation WHERE platform_observation.source_id=source.id) AND NOT EXISTS(SELECT 1 FROM creator_origins origin WHERE origin.source_id=source.id)`, locator); }
function orphanTargets(db: DatabaseSync, route: ContactRoute) {
  return strings(db, "SELECT DISTINCT resolved_target_id FROM locator_resolutions WHERE locator_id=? ORDER BY resolved_target_id", route.locator.id).reduce((total, target) => total + count(db, `SELECT NOT EXISTS(SELECT 1 FROM locator_resolutions sibling WHERE sibling.resolved_target_id=?1 AND sibling.locator_id!=?2)
    AND NOT EXISTS(SELECT 1 FROM conversations conversation WHERE conversation.resolved_target_id=?1 AND NOT EXISTS(SELECT 1 FROM conversation_contact_routes owner WHERE owner.conversation_id=conversation.id AND owner.contact_route_id=?3))
    AND NOT EXISTS(SELECT 1 FROM messages message WHERE message.sender_target_id=?1 AND NOT EXISTS(SELECT 1 FROM conversation_contact_routes owner WHERE owner.conversation_id=message.conversation_id AND owner.contact_route_id=?3))
    AND NOT EXISTS(SELECT 1 FROM identity_conflicts conflict WHERE conflict.resolved_target_id=?1 AND (conflict.locator_id IS NULL OR conflict.locator_id!=?2))
    AND NOT EXISTS(SELECT 1 FROM audit_events audit WHERE audit.resolved_target_id=?1 AND (audit.contact_route_id IS NULL OR audit.contact_route_id!=?3))
    AND NOT EXISTS(SELECT 1 FROM creator_origins origin WHERE origin.source_resolved_target_id=?1)`, target, route.locator.id, route.route.id), 0);
}
function impact(db: DatabaseSync, route: ContactRoute, email: EmailPlan): DeleteImpact {
  const r = route.route.id, l = route.locator.id;
  const result: DeleteImpact = {
    contact_routes: 1, dependencies: count(db, "SELECT COUNT(*) FROM creator_contact_route_dependencies WHERE child_contact_route_id=?", r),
    email_threads: email.thread_ids.length, email_messages: email.email_ids.length, email_recipients: email.recipient_fingerprints.length,
    email_drafts: count(db, "SELECT COUNT(*) FROM email_drafts WHERE contact_route_id=?", r),
    direct_message_conversations: count(db, "SELECT COUNT(*) FROM conversation_contact_routes WHERE contact_route_id=?", r),
    direct_messages: count(db, "SELECT COUNT(*) FROM messages WHERE conversation_id IN(SELECT conversation_id FROM conversation_contact_routes WHERE contact_route_id=?)", r),
    outreach_touchpoints: count(db, "SELECT COUNT(*) FROM touchpoints WHERE contact_route_id=?", r), audit_events: count(db, "SELECT COUNT(*) FROM audit_events WHERE contact_route_id=?", r),
    idempotency_records: count(db, "SELECT COUNT(*) FROM idempotency_records WHERE contact_route_id=?", r), target_resolutions: count(db, "SELECT COUNT(*) FROM locator_resolutions WHERE locator_id=?", l),
    identity_conflicts: count(db, "SELECT COUNT(*) FROM identity_conflicts WHERE locator_id=?", l), observations: count(db, "SELECT COUNT(*) FROM locator_observations WHERE locator_id=?", l),
    contact_locators: orphanLocator(db, route, email), observation_sources: orphanSources(db, l), resolved_targets: orphanTargets(db, route), creator_origins_detached: count(db, "SELECT COUNT(*) FROM creator_origins WHERE source_contact_route_id=?", r), local_records_total: 0,
  };
  result.local_records_total = Object.values(result).reduce((sum, count) => sum + count, 0);
  return impactSchema.parse(result);
}
function fingerprint(db: DatabaseSync, route: ContactRoute, impact: DeleteImpact, email: EmailPlan) {
  const r = route.route.id, l = route.locator.id;
  return JSON.stringify({ creator_id: route.route.creator_id, contact_route_id: r, locator_id: l, route_updated_at: route.route.updated_at, locator_updated_at: route.locator.updated_at, impact,
    observations: strings(db, "SELECT printf('%s|%s|%s|%s',observation.id,observation.source_id,observation.observed_at,source.observed_at) FROM locator_observations observation JOIN observation_sources source ON source.id=observation.source_id WHERE observation.locator_id=? ORDER BY observation.id", l),
    dependencies: strings(db, "SELECT printf('%s|%s|%s',parent_contact_route_id,child_contact_route_id,kind) FROM creator_contact_route_dependencies WHERE parent_contact_route_id=?1 OR child_contact_route_id=?1 ORDER BY parent_contact_route_id,child_contact_route_id", r),
    email_threads: email.thread_fingerprints, emails: email.email_fingerprints, email_recipients: email.recipient_fingerprints,
    email_drafts: strings(db, "SELECT printf('%s|%s|%s|%s',id,revision,updated_at,COALESCE(sending_idempotency_key,'')) FROM email_drafts WHERE contact_route_id=? ORDER BY id", r),
    conversations: strings(db, "SELECT printf('%s|%s',conversation.id,conversation.updated_at) FROM conversations conversation JOIN conversation_contact_routes owner ON owner.conversation_id=conversation.id WHERE owner.contact_route_id=? ORDER BY conversation.id", r),
    messages: strings(db, "SELECT printf('%s|%s|%s',message.id,message.status,message.sent_at) FROM messages message JOIN conversation_contact_routes owner ON owner.conversation_id=message.conversation_id WHERE owner.contact_route_id=? ORDER BY message.id", r),
    touchpoints: strings(db, "SELECT printf('%s|%s',id,occurred_at) FROM touchpoints WHERE contact_route_id=? ORDER BY id", r), audit_events: strings(db, "SELECT printf('%s|%s',id,created_at) FROM audit_events WHERE contact_route_id=? ORDER BY id", r),
    idempotency_records: strings(db, "SELECT printf('%s|%s|%s',key,state,updated_at) FROM idempotency_records WHERE contact_route_id=? ORDER BY key", r),
    resolutions: strings(db, "SELECT printf('%s|%s|%s|%s',id,state,resolved_at,COALESCE(invalidated_at,'')) FROM locator_resolutions WHERE locator_id=? ORDER BY id", l), identity_conflicts: strings(db, "SELECT printf('%s|%s|%s',id,state,created_at) FROM identity_conflicts WHERE locator_id=? ORDER BY id", l),
    observation_sources: strings(db, "SELECT printf('%s|%s|%s',source.id,source.kind,source.observed_at) FROM observation_sources source WHERE EXISTS(SELECT 1 FROM locator_observations observation WHERE observation.source_id=source.id AND observation.locator_id=?) ORDER BY source.id", l),
    resolved_targets: strings(db, "SELECT DISTINCT printf('%s|%s',target.id,target.updated_at) FROM resolved_targets target JOIN locator_resolutions resolution ON resolution.resolved_target_id=target.id WHERE resolution.locator_id=? ORDER BY target.id", l), creator_origins: strings(db, "SELECT printf('%s|%s',id,discovered_at) FROM creator_origins WHERE source_contact_route_id=? ORDER BY id", r),
  });
}
export function buildDeletePlan(db: DatabaseSync, route: ContactRoute) {
  const email = emailPlan(db, route);
  const children = db.prepare("SELECT child.creator_id,dependency.child_contact_route_id AS contact_route_id,dependency.kind AS dependency_kind,locator.channel,locator.display_value FROM creator_contact_route_dependencies dependency JOIN creator_contact_routes child ON child.id=dependency.child_contact_route_id JOIN contact_locators locator ON locator.id=child.locator_id WHERE dependency.parent_contact_route_id=? ORDER BY dependency.child_contact_route_id").all(route.route.id).map(row => z.object({ creator_id: text, contact_route_id: text, dependency_kind: z.enum(["link_hub", "community_administrator"]), channel, display_value: text }).parse(row));
  const blocking_artifacts = blockingArtifacts(db, route, email), affected = impact(db, route, email);
  return { route, email, children, blocking_artifacts, impact: affected, fingerprint: fingerprint(db, route, affected, email) };
}
export type DeletePlan = ReturnType<typeof buildDeletePlan>;
