import type { ResultSnapshot } from "@mcp-ui/lib/result-contract";

export const RETIRED_PRESENTATIONS: ReadonlySet<string> = new Set([
  "outreach_confirmation", "outreach_outcome", "outreach_draft",
  "email_confirmation", "email_outcome",
]);

const RETIRED_OPERATIONS = new Set([
  "onecrew.outreach.message.send", "onecrew.creator.content_reply.send",
  "x.message.send", "instagram.message.send", "telegram.message.send",
  "x.conversation.sync", "instagram.conversation.sync", "telegram.conversation.sync",
  "email.draft.send", "email.manual.send", "email.direct.send",
  "email.send_many", "email.send_many.reconcile",
]);

export function isRetiredSendingResult(snapshot: ResultSnapshot): boolean {
  if (["creator_save_batch_confirmation", "creator_save_batch_outcome", "creator_batch_confirmation", "creator_batch_outcome", "search_plan_editor"].includes(snapshot.presentation.kind)) return false;
  return RETIRED_PRESENTATIONS.has(snapshot.presentation.kind)
    || snapshot.batch.outcomes.some(outcome => RETIRED_OPERATIONS.has(outcome.operation_id));
}
