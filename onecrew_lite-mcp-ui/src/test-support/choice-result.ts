import { choiceAnswersValid, type ResultChoiceAnswer, type ResultSnapshot } from "@mcp-ui/lib/result-contract";

/** Preview-only local completion. No Host transport, OneCrew write or continuation dispatch. */
export function simulateChoiceResult(initial: ResultSnapshot, answers: ResultChoiceAnswer[]): ResultSnapshot {
  const pending = initial.pending_action;
  if (pending?.kind !== "choice" || !choiceAnswersValid(pending.questions, answers)) throw new Error("Invalid preview choice submission");
  const result = structuredClone(initial);
  const summary = initial.locale === "zh-CN" ? "模拟：全部答案已提交" : "Simulation: all answers submitted";
  result.binding.revision++;
  result.pending_action = null;
  result.state = { kind: "resolved", code: "choice_selected", summary };
  result.presentation = { ...result.presentation, kind: "choice", summary: null, sections: [{ kind: "items", heading: null, items: pending.questions.map((question, index) => {
    const answer = answers[index]!;
    const selected = "option_ids" in answer ? answer.option_ids : "option_id" in answer ? [answer.option_id] : [];
    const text = "text" in answer ? answer.text : question.options.filter(option => selected.includes(option.id)).map(option => option.label).join("\n");
    return { title: question.header, description: text };
  }) }] };
  result.batch.outcomes = result.batch.outcomes.map(row => ({ ...row, summary, disposition: { kind: "succeeded" }, resource_refs: [] }));
  result.continuation = { ...result.continuation, state: "not_requested", dispatch_started_at_ms: null, dispatch_not_after_ms: null };
  return result;
}
