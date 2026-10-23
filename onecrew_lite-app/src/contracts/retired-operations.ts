// GitHub 0.7.1 操作目录；用户明确移除的功能保留明确退役响应。
export const retiredOperations = Object.freeze([
  "email.credential.connect", "email.credential.delete", "email.credential.rotate",
  "email.direct.prepare", "email.direct.send",
  "email.draft.restore", "email.draft.save", "email.draft.send", "email.draft.trash",
  "email.manual.prepare", "email.manual.send",
  "email.send_many", "email.send_many.reconcile",
  "email.template.delete", "email.template.render", "email.template.save",
  "instagram.conversation.sync", "instagram.message.send",
  "onecrew.creator.content_reply.prepare", "onecrew.creator.content_reply.send",
  "onecrew.outreach.message.prepare", "onecrew.outreach.message.send",
  "onecrew.outreach.relationship.prepare", "onecrew.outreach.relationship.update",
  "onecrew.outreach.contact_target.resolve", "onecrew.outreach.relationship.get",
  "telegram.conversation.sync", "telegram.message.send",
  "x.chat.passcode.clear", "x.chat.passcode.set", "x.conversation.sync", "x.message.send",
] as const);

export type RetiredOperation = typeof retiredOperations[number];
const retiredOperationSet = new Set<string>(retiredOperations);

export const retiredWebCommands = Object.freeze([
  "onecrew_operator_account_x_chat_passcode_set_in_app", "onecrew_operator_account_x_chat_passcode_clear_in_app",
  "list_resend_credentials", "connect_resend_credential", "delete_resend_credential", "test_resend_credential",
  "onecrew_relationship_update_prepare",
  "onecrew_contact_target_resolve",
  "onecrew_outreach_message_prepare", "onecrew_outreach_message_send_prepare",
  "onecrew_outreach_confirm", "onecrew_outreach_cancel", "onecrew_conversation_sync",
  "onecrew_email_template_save", "onecrew_email_template_delete", "onecrew_email_template_render",
  "onecrew_email_draft_save", "onecrew_email_draft_trash", "onecrew_email_draft_restore", "onecrew_email_draft_send",
] as const);
const retiredWebCommandSet = new Set<string>(retiredWebCommands);

export class OperationRetiredError extends Error {
  readonly code = "operation.retired";
  readonly status = 410;
  readonly operation: string;
  constructor(operation: string) {
    super("This operation was removed. User outbound sending is available only through Mail GmailBulk.");
    this.operation = operation;
  }
}

export function assertOperationAvailable(operation: string): void {
  if (retiredOperationSet.has(operation) || retiredWebCommandSet.has(operation)) throw new OperationRetiredError(operation);
}
