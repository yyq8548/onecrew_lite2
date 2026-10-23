import type { DatabaseSync } from "node:sqlite";
import type { ContactRoute } from "./contacts.ts";
import { count, emailAddress } from "./contact-delete-plan.ts";
import type { EmailPlan } from "./contact-delete-plan.ts";
import { integer, text } from "./contracts.ts";

export type ArtifactKind = "in_flight_email" | "in_flight_direct_message" | "in_flight_relationship_mutation" | "ambiguous_direct_message_history" | "shared_email_history" | "shared_direct_message_history" | "shared_contact_locator" | "ambiguous_local_audit_history";
export function blockingArtifacts(db: DatabaseSync, route: ContactRoute, email: EmailPlan) {
  const artifacts: { kind: ArtifactKind; count: number }[] = [];
  const push = (kind: ArtifactKind, count: number) => { if (!count) return; const existing = artifacts.find(item => item.kind === kind); if (existing) existing.count += count; else artifacts.push({ kind, count }); };
  const r = route.route.id, l = route.locator.id, c = route.route.creator_id, address = route.locator.normalized_value;
  push("in_flight_direct_message", count(db, `SELECT COUNT(*) FROM idempotency_records WHERE action IN ('relationship_update_v1','content_reply_send_v1','direct_message_send_v1') AND state IN ('in_progress','remote_started','remote_accepted','remote_verified') AND CASE WHEN json_valid(result_json) AND json_extract(result_json,'$.version')=1 AND json_type(result_json,'$.state.identity.creator_id')='text' AND json_type(result_json,'$.state.identity.operator_account_ref_id')='text' AND length(trim(json_extract(result_json,'$.state.identity.creator_id')))>0 AND length(trim(json_extract(result_json,'$.state.identity.operator_account_ref_id')))>0 AND json_extract(result_json,'$.state.fingerprint')=request_fingerprint AND request_fingerprint GLOB 'sha256:v1:*' THEN contact_route_id IS NULL AND json_extract(result_json,'$.state.identity.contact_route_id')=?1 ELSE 1 END`, r));
  push("shared_contact_locator", count(db, `SELECT COUNT(*) FROM locator_observations observation WHERE observation.locator_id=?1 AND ((observation.creator_id IS NOT NULL AND observation.creator_id!=?2) OR (observation.platform_account_id IS NOT NULL AND (NOT EXISTS(SELECT 1 FROM creator_account_links owner WHERE owner.platform_account_id=observation.platform_account_id AND owner.creator_id=?2) OR EXISTS(SELECT 1 FROM creator_account_links other_owner WHERE other_owner.platform_account_id=observation.platform_account_id AND other_owner.creator_id!=?2))))`, l, c));
  push("shared_contact_locator", count(db, `SELECT COUNT(*) FROM identity_conflicts WHERE locator_id=?1 AND ((creator_id IS NULL OR creator_id!=?2) OR (platform_account_id IS NOT NULL AND (NOT EXISTS(SELECT 1 FROM creator_account_links owner WHERE owner.platform_account_id=identity_conflicts.platform_account_id AND owner.creator_id=?2) OR EXISTS(SELECT 1 FROM creator_account_links other_owner WHERE other_owner.platform_account_id=identity_conflicts.platform_account_id AND other_owner.creator_id!=?2))))`, l, c));
  push("in_flight_email", count(db, "SELECT COUNT(*) FROM email_drafts WHERE contact_route_id=? AND (sending_idempotency_key IS NOT NULL OR send_reserved_at IS NOT NULL)", r));
  for (const row of db.prepare("SELECT action,COUNT(*) AS n FROM idempotency_records WHERE contact_route_id=? AND state IN ('in_progress','remote_started','remote_accepted','remote_verified') GROUP BY action ORDER BY action").all(r)) { const action = text.parse(row.action); push(action.includes("email") ? "in_flight_email" : action.includes("relationship") ? "in_flight_relationship_mutation" : "in_flight_direct_message", integer.parse(row.n)); }
  push("shared_email_history", email.shared_threads);
  push("shared_email_history", email.email_ids.reduce((sum, id) => sum + count(db, "SELECT COUNT(*) FROM email_drafts WHERE reply_to_email_id=? AND contact_route_id IS NOT ?", id, r), 0));
  push("shared_email_history", count(db, "SELECT COUNT(*) FROM email_drafts WHERE contact_route_id=? AND creator_id IS NOT ?", r, c));
  if (route.locator.channel === "email") { const target = emailAddress(address); if (target === null) throw new Error("stored email contact route is invalid"); push("shared_email_history", db.prepare("SELECT to_address FROM email_drafts WHERE contact_route_id IS NULL ORDER BY id").all().filter(row => emailAddress(text.parse(row.to_address)) === target).length); }
  push("ambiguous_direct_message_history", count(db, `SELECT COUNT(DISTINCT conversation.id) FROM conversations conversation JOIN locator_resolutions resolution ON resolution.resolved_target_id=conversation.resolved_target_id AND resolution.account_ref_id=conversation.account_ref_id WHERE resolution.locator_id=? AND NOT EXISTS(SELECT 1 FROM conversation_contact_routes owner WHERE owner.conversation_id=conversation.id)`, l));
  push("shared_direct_message_history", count(db, "SELECT COUNT(*) FROM conversation_contact_routes owner WHERE owner.contact_route_id=? AND (SELECT COUNT(*) FROM conversation_contact_routes sibling WHERE sibling.conversation_id=owner.conversation_id)>1", r));
  push("shared_contact_locator", count(db, "SELECT CASE WHEN COUNT(*)>1 THEN COUNT(*) ELSE 0 END FROM creator_contact_routes WHERE locator_id=?", l));
  push("ambiguous_local_audit_history", count(db, `SELECT COUNT(*) FROM audit_events audit
    WHERE (audit.contact_route_id=?1 AND ((audit.creator_id IS NULL OR audit.creator_id!=?3)
      OR (audit.contact_locator_id IS NOT NULL AND audit.contact_locator_id!=?2)
      OR (audit.email_id IS NOT NULL AND NOT EXISTS(
        SELECT 1 FROM emails audit_email JOIN email_threads audit_thread ON audit_thread.id=audit_email.thread_id
        WHERE audit_email.id=audit.email_id AND (audit_thread.contact_route_id=?1 OR (audit_thread.contact_route_id IS NULL AND audit_thread.creator_id=?3 AND EXISTS(
          SELECT 1 FROM email_recipients audit_recipient WHERE audit_recipient.email_id=audit_email.id AND (audit_recipient.locator_id=?2 OR (audit_recipient.locator_id IS NULL AND lower(trim(audit_recipient.address))=lower(trim(?4))))
        )))
      ))))
      OR (audit.contact_locator_id=?2 AND (audit.contact_route_id IS NULL OR audit.contact_route_id!=?1))
      OR (audit.email_id IN(
        SELECT email.id FROM emails email JOIN email_threads thread ON thread.id=email.thread_id
        WHERE thread.contact_route_id=?1 OR (thread.contact_route_id IS NULL AND thread.creator_id=?3 AND EXISTS(
          SELECT 1 FROM emails candidate_email JOIN email_recipients recipient ON recipient.email_id=candidate_email.id
          WHERE candidate_email.thread_id=thread.id AND (recipient.locator_id=?2 OR (recipient.locator_id IS NULL AND lower(trim(recipient.address))=lower(trim(?4))))
        ))
      ) AND (audit.contact_route_id IS NULL OR audit.contact_route_id!=?1))`, r, l, c, address));
  push("ambiguous_local_audit_history", count(db, `SELECT COUNT(*) FROM touchpoints WHERE contact_route_id=?1 AND (creator_id!=?2
    OR (message_id IS NOT NULL AND NOT EXISTS(
      SELECT 1 FROM messages touchpoint_message JOIN conversation_contact_routes message_owner ON message_owner.conversation_id=touchpoint_message.conversation_id
      WHERE touchpoint_message.id=touchpoints.message_id AND message_owner.contact_route_id=?1
    ))
    OR (email_id IS NOT NULL AND NOT EXISTS(
      SELECT 1 FROM emails touchpoint_email JOIN email_threads touchpoint_thread ON touchpoint_thread.id=touchpoint_email.thread_id
      WHERE touchpoint_email.id=touchpoints.email_id AND (touchpoint_thread.contact_route_id=?1 OR (touchpoint_thread.contact_route_id IS NULL AND touchpoint_thread.creator_id=?2 AND EXISTS(
        SELECT 1 FROM email_recipients touchpoint_recipient WHERE touchpoint_recipient.email_id=touchpoint_email.id AND (touchpoint_recipient.locator_id=?3 OR (touchpoint_recipient.locator_id IS NULL AND lower(trim(touchpoint_recipient.address))=lower(trim(?4))))
      )))
    ))
    OR (account_relationship_id IS NOT NULL AND NOT EXISTS(
      SELECT 1 FROM account_relationships touchpoint_relationship JOIN resolved_targets relationship_target ON relationship_target.platform_account_id=touchpoint_relationship.target_platform_account_id AND relationship_target.account_ref_id=touchpoint_relationship.actor_account_ref_id
      JOIN locator_resolutions relationship_resolution ON relationship_resolution.resolved_target_id=relationship_target.id AND relationship_resolution.account_ref_id=touchpoint_relationship.actor_account_ref_id
      WHERE touchpoint_relationship.id=touchpoints.account_relationship_id AND relationship_resolution.locator_id=?3
    )))`, r, c, l, address));
  push("ambiguous_local_audit_history", count(db, `SELECT COUNT(DISTINCT touchpoint.id) FROM touchpoints touchpoint
    LEFT JOIN messages message ON message.id=touchpoint.message_id
    LEFT JOIN conversations conversation ON conversation.id=message.conversation_id
    LEFT JOIN locator_resolutions message_resolution ON message_resolution.resolved_target_id=conversation.resolved_target_id AND message_resolution.account_ref_id=conversation.account_ref_id
    LEFT JOIN emails email ON email.id=touchpoint.email_id LEFT JOIN email_recipients recipient ON recipient.email_id=email.id
    LEFT JOIN account_relationships relationship ON relationship.id=touchpoint.account_relationship_id
    LEFT JOIN resolved_targets relationship_target ON relationship_target.platform_account_id=relationship.target_platform_account_id AND relationship_target.account_ref_id=relationship.actor_account_ref_id
    LEFT JOIN locator_resolutions relationship_resolution ON relationship_resolution.resolved_target_id=relationship_target.id AND relationship_resolution.account_ref_id=relationship.actor_account_ref_id
    WHERE (touchpoint.contact_route_id IS NULL OR touchpoint.contact_route_id!=?2) AND (message_resolution.locator_id=?1 OR recipient.locator_id=?1 OR relationship_resolution.locator_id=?1)`, l, r));
  return artifacts;
}
