import type en from "@mcp-ui/locales/en-US/errors";
const errors: Record<keyof typeof en, string> = {
  missing_structured_content: "服务端没有返回 structuredContent。",
  structured_content_not_object: "structuredContent 不是对象。",
  error_without_typed_failure: "服务端标记了错误，但未返回可识别的失败结构。",
  success_marked_error: "服务端返回了成功结构，但同时将调用标记为错误。",
  snapshot_root_rejected: "snapshot 顶层键集合、语言或 presented 标记不符合协议。",
  snapshot_binding_rejected: "snapshot 的 result binding 未通过格式校验。",
  snapshot_batch_rejected: "snapshot 的执行结果批次未通过格式校验。",
  snapshot_presentation_rejected: "snapshot 的展示投影未通过格式校验。",
  snapshot_pending_action_rejected: "snapshot 的待处理操作未通过格式校验。",
  snapshot_state_rejected: "snapshot 的状态字段未通过格式校验。",
  snapshot_continuation_rejected: "snapshot 的后续消息状态未通过格式校验。",
  snapshot_consistency_rejected: "snapshot 各部分分别有效，但视图、状态与待处理操作彼此不一致。",
  claimed_contract_rejected: "返回的是 continuation claim，但租约结构未通过校验。",
  completed_contract_rejected: "返回的是 continuation completion，但完成结构未通过校验。",
  failure_contract_rejected: "返回了失败码，但失败结构未通过当前协议校验。",
  unrecognized_root: "structuredContent 的顶层类型不属于当前卡片协议。",
};
export default errors;
