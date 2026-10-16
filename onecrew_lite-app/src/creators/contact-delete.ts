import { randomUUID } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { integer, parseJson, text } from "./contracts.ts";
import { contactConfirmInput, contactDeleteInput, creatorActive, hasRootInitial, routeById } from "./contacts.ts";
import { buildDeletePlan, impactKeys, impactSchema } from "./contact-delete-plan.ts";
import type { DeletePlan } from "./contact-delete-plan.ts";
import { purgeContact } from "./contact-delete-purge.ts";

const prepareTool = "onecrew.creator.contact_route.delete.prepare", confirmTool = "onecrew.creator.contact_route.delete.confirm";
const deletedSchema = z.object({ status: z.literal("deleted"), operation_id: text, creator_id: text, contact_route_id: text, deleted: impactSchema, remote_not_deleted: z.boolean() }).strict();
const terminalSchema = z.discriminatedUnion("status", [deletedSchema, z.object({ status: z.literal("stale"), operation_id: text, reason: z.literal("plan_changed"), next_tool: text }).strict(), z.object({ status: z.literal("expired"), operation_id: text, next_tool: text }).strict()]);
function blockReason(plan: DeletePlan, active: boolean) {
  if (!active) return "archived_creator" as const;
  if (hasRootInitial(plan.route)) return "root_initial_platform_account" as const;
  if (plan.children.length) return "has_derived_contact_routes" as const;
  if (plan.blocking_artifacts.some(artifact => ["in_flight_email", "in_flight_direct_message", "in_flight_relationship_mutation"].includes(artifact.kind))) return "in_flight_external_action" as const;
  if (plan.blocking_artifacts.some(artifact => ["ambiguous_direct_message_history", "ambiguous_local_audit_history"].includes(artifact.kind))) return "ambiguous_legacy_history" as const;
  return plan.blocking_artifacts.length ? "shared_local_history" as const : null;
}
export function prepareContactDelete(db: DatabaseSync, raw: unknown) {
  const input = contactDeleteInput.parse(raw), active = creatorActive(db, input.creator_id);
  const plan = buildDeletePlan(db, routeById(db, input.creator_id, input.contact_route_id));
  const reason = blockReason(plan, active);
  if (reason !== null) return { status: "blocked" as const, creator_id: input.creator_id, contact_route_id: input.contact_route_id, reason, children: plan.children, blocking_artifacts: plan.blocking_artifacts };
  const operation_id = randomUUID(), at = Math.floor(Date.now() / 1000), expires_at = at + 600;
  db.prepare("INSERT INTO creator_contact_route_delete_operations (id,creator_id,contact_route_id,plan_fingerprint,impact_json,status,result_json,expires_at,created_at,updated_at) VALUES (?,?,?,?,?,'prepared',NULL,?,?,?)").run(operation_id, input.creator_id, input.contact_route_id, plan.fingerprint, JSON.stringify(plan.impact), expires_at, at, at);
  return { status: "confirmation_required" as const, operation_id, creator_id: input.creator_id, contact_route_id: input.contact_route_id, impact: plan.impact, remote_not_deleted: true, expires_at, next_tool: confirmTool };
}
export function confirmContactDelete(db: DatabaseSync, raw: unknown) {
  const input = contactConfirmInput.parse(raw), row = db.prepare("SELECT * FROM creator_contact_route_delete_operations WHERE id=?").get(input.operation_id);
  if (!row) throw new Error("contact route delete operation not found");
  const stored = z.object({ creator_id: text, contact_route_id: text, plan_fingerprint: text, impact_json: text, status: z.enum(["prepared", "completed"]), result_json: text.nullable(), expires_at: integer }).parse(row);
  const impact = impactSchema.parse(parseJson(stored.impact_json));
  if (stored.status === "completed") { if (stored.result_json === null) throw new Error("completed contact route delete operation is corrupt"); return { result: terminalSchema.parse(parseJson(stored.result_json)), changed: false }; }
  const persist = (result: z.infer<typeof terminalSchema>, changed: boolean) => { const affected = db.prepare("UPDATE creator_contact_route_delete_operations SET status='completed',result_json=?,updated_at=? WHERE id=? AND status='prepared'").run(JSON.stringify(result), Math.floor(Date.now() / 1000), input.operation_id).changes; if (affected !== 1) throw new Error("contact route delete operation changed while being confirmed"); return { result, changed }; };
  if (Math.floor(Date.now() / 1000) > stored.expires_at) return persist({ status: "expired", operation_id: input.operation_id, next_tool: prepareTool }, false);
  const stale = () => persist({ status: "stale", operation_id: input.operation_id, reason: "plan_changed", next_tool: prepareTool }, false);
  if (!db.prepare("SELECT 1 FROM creator_contact_routes WHERE id=? AND creator_id=?").get(stored.contact_route_id, stored.creator_id)) return stale();
  const plan = buildDeletePlan(db, routeById(db, stored.creator_id, stored.contact_route_id));
  if (blockReason(plan, creatorActive(db, stored.creator_id)) !== null || plan.fingerprint !== stored.plan_fingerprint || impactKeys.some(key => plan.impact[key] !== impact[key])) return stale();
  purgeContact(db, plan);
  return persist({ status: "deleted", operation_id: input.operation_id, creator_id: stored.creator_id, contact_route_id: stored.contact_route_id, deleted: plan.impact, remote_not_deleted: true }, true);
}
