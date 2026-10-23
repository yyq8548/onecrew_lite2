import assert from "node:assert/strict";
import test from "node:test";
import { AccountContextService } from "../src/identity/account-context.ts";
import { prepareChoice, submitChoice } from "../src/results/choice-flow.ts";
import { ResultChoiceStore } from "../src/results/choice-store.ts";
import { OwnerReviewContext } from "../src/results/owner-context.ts";

test("Choice operation preserves actual envelope, localized complete answers and single selection authority", () => {
  const accounts = new AccountContextService(), account = accounts.activate({ user: { id: "owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
  const store = new ResultChoiceStore(accounts), context = new OwnerReviewContext("mcp:full", "question", "onecrew.ask_user_question");
  const input = { interaction_id: "preference", title: "选择筛选条件", questions: [{ id: "channels", selection_mode: "multiple", header: "平台", prompt: "选择检索平台", options: [{ id: "youtube", label: "YouTube" }, { id: "instagram", label: "Instagram" }] }, { id: "language", header: "语言", prompt: "选择内容语言", options: [{ id: "zh", label: "中文" }, { id: "en", label: "英文" }] }] };
  try {
    const prepared = prepareChoice(input, context);
    assert.equal(JSON.parse(prepared.payload.toString()).version, "v2");
    const reference = store.mint(prepared, account), shown = store.present(reference, context.origin, account, "zh-CN");
    assert.throws(() => submitChoice(store, shown.binding, account, [{ question_id: "channels", option_ids: ["unknown"] }, { question_id: "language", option_id: "zh" }]), /unknown_choice_option/);
    assert.equal(store.read(shown.binding, account).state.kind, "available");
    const result = submitChoice(store, shown.binding, account, [{ question_id: "language", text: "保持原文\n支持中文和英文" }, { question_id: "channels", option_ids: ["instagram", "youtube"] }]);
    assert.equal(result.state.kind, "resolved");
    assert.equal(result.batch.outcomes[0]!.disposition.kind, "succeeded");
    const continuation = store.claimContinuation(result.binding, account, result.continuation.continuation_revision, "pending");
    assert.deepEqual(continuation.request.context, { kind: "choice", answers: [{ question_id: "channels", option_ids: ["youtube", "instagram"] }, { question_id: "language", text: "保持原文\n支持中文和英文" }] });
    assert.equal(continuation.request.message, "我已完成 OneCrew 选择：\n- 平台：YouTube [youtube]; Instagram [instagram]\n- 语言：保持原文\n支持中文和英文\n请按这些已确认的选择继续当前任务；不要重复提交选择。");
    assert.equal(continuation.request.requires_model_context, true);
    assert.throws(() => submitChoice(store, result.binding, account, []), /choice_required/);
    assert.throws(() => prepareChoice({ ...input, questions: [{ ...input.questions[0], options: [{ id: "only", label: "只有一个" }] }] }, context), /new_choice_question_limits/);
    assert.throws(() => prepareChoice({ ...input, title: "非法\n标题" }, context));
  } finally { store.close(); }
});
