import type { AccountContext } from "../identity/account-context.ts";
import { OwnerReviewContext } from "../results/owner-context.ts";
import { SubmittedOwnerGrant, type OwnerReviewAuthority, type PreparedOwnerReview } from "../results/grants.ts";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import type { McpExecutionCall } from "./mcp-contracts.ts";
import type { McpOperationEntry } from "./mcp-catalogue.ts";
import type { McpRegisteredHandler } from "./mcp-handlers.ts";
import { mcpConfirmationProjection } from "./mcp-projection.ts";

// 密封实例只由已注册合同创建；确认只能消费共享 store 发行的单次 grant。
export class PreparedMcpOperationAction implements OwnerReviewAuthority {
  readonly kind = "operation" as const;
  readonly deadline = performance.now() + 15 * 60_000;
  readonly #call: McpExecutionCall;
  readonly #operation: McpOperationEntry;
  readonly #handler: McpRegisteredHandler;
  private constructor(call: McpExecutionCall, operation: McpOperationEntry, handler: McpRegisteredHandler) {
    this.#call = structuredClone(call); this.#operation = structuredClone(operation); this.#handler = handler; Object.freeze(this);
  }
  static prepare(call: McpExecutionCall, operation: McpOperationEntry, handler: McpRegisteredHandler, context: OwnerReviewContext): PreparedOwnerReview {
    OwnerReviewContext.assertIssued(context);
    if (operation.approval !== "attended_prompt" || operation.secret !== "none" || operation.sensitive_input_paths.length || typeof operation.approval_request !== "string" || !operation.approval_request.length) throw new Error("mcp_confirmation_unavailable");
    handler.validate(call);
    return { context, action: new PreparedMcpOperationAction(call, operation, handler), presentation: mcpConfirmationProjection(operation, call.arguments), prompt: operation.approval_request, label: "Confirm" };
  }
  encodedPayload(): Buffer { return Buffer.from(stringifyApplicationJson(["mcp_attended_operation_v1", this.#call, this.#operation])); }
  toJSON(): never { throw new Error("MCP operation authority must not be serialized"); }
  static async execute(grant: SubmittedOwnerGrant, account: AccountContext): Promise<unknown> {
    const action = grant.consume("operation", account);
    if (!(action instanceof PreparedMcpOperationAction) || action.#call.call_id !== grant.context().call_id || action.#call.operation_id !== grant.context().operation_id) throw new Error("mcp_operation_authority_mismatch");
    action.#handler.validate(action.#call);
    return action.#handler.invoke(action.#call, account);
  }
}
