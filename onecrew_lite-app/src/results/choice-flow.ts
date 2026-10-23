import { z } from "zod";
import { isDeepStrictEqual } from "node:util";
import type { AccountContext } from "../identity/account-context.ts";
import { parseApplicationJson, stringifyApplicationJson } from "../contracts/application-json.ts";
import { resultChoiceQuestions, type ResultChoiceAnswer, type ResultChoiceQuestion } from "./choice-contracts.ts";
import { resultIdentifier, resultText, type ExecutedBatch, type PresentationProjection, type ResultBinding } from "./contracts.ts";
import { ResultChoiceStore } from "./choice-store.ts";
import { OwnerReviewContext } from "./owner-context.ts";
import { resultByteLength } from "./fields.ts";
import { trim } from "../creators/contracts.ts";

const requestText = resultText.refine(value => !/\p{Cc}/u.test(value));
export const askUserQuestionInput = z.object({ interaction_id: resultIdentifier(), title: requestText.refine(value => resultByteLength(value) <= 512), questions: resultChoiceQuestions.refine(value => value.length <= 4 && value.every(question => question.options.length >= 2 && question.options.length <= 4 && [question.header, question.prompt, ...question.options.flatMap(option => [option.label, ...(option.description === null ? [] : [option.description])])].every(text => !/\p{Cc}/u.test(text))), "new_choice_question_limits") }).strict();
const envelopeSchema = z.object({ version: z.literal("v2"), call_id: resultIdentifier(), operation_id: z.literal("onecrew.ask_user_question"), interaction_id: resultIdentifier(), questions: resultChoiceQuestions }).strict();
function bounded(value: string, fallback: string, limit = 512): string {
  const trimmed = trim(value) || fallback;
  let result = "";
  for (const character of trimmed) { if (resultByteLength(result + character) > limit) break; result += character; }
  return result;
}
function preview(value: string, fallback: string): string { const text = bounded(value, fallback); return resultByteLength(text) <= 256 ? text : `${bounded(text, fallback, 253)}…`; }
function choiceBatch(context: OwnerReviewContext, selected: boolean): ExecutedBatch {
  return { outcomes: [{ call_id: context.call_id, operation_id: context.operation_id, disposition: selected ? { kind: "succeeded" } : { kind: "not_run", reason: "choice_required" }, summary: selected ? "The user completed the requested choice." : "The user must answer each question using its selection mode or custom text.", resource_refs: [] }] };
}
function answerText(question: ResultChoiceQuestion, answer: ResultChoiceAnswer): string {
  if ("text" in answer) return answer.text;
  if ("option_id" in answer) {
    const option = question.options.find(value => value.id === answer.option_id);
    if (!option) throw new Error("choice_snapshot_mismatch"); return option.label;
  }
  return answer.option_ids.map(id => { const option = question.options.find(value => value.id === id); if (!option) throw new Error("choice_snapshot_mismatch"); return `${option.label} [${option.id}]`; }).join("; ");
}
export function prepareChoice(raw: unknown, context: OwnerReviewContext) {
  OwnerReviewContext.assertIssued(context);
  if (context.operation_id !== "onecrew.ask_user_question") throw new Error("choice_operation_mismatch");
  const input = askUserQuestionInput.parse(raw);
  const payload = Buffer.from(stringifyApplicationJson({ version: "v2", call_id: context.call_id, operation_id: context.operation_id, interaction_id: input.interaction_id, questions: input.questions }));
  const presentation: PresentationProjection = { kind: "choice", title: input.title, summary: "Answer each question, then review before submitting.", sections: [{ kind: "items", heading: "Questions", items: input.questions.map(question => ({ title: question.header, description: question.prompt })) }] };
  return { context, questions: input.questions, presentation, batch: choiceBatch(context, false), payload };
}
export function submitChoice(store: ResultChoiceStore, binding: ResultBinding, account: AccountContext, rawAnswers: unknown) {
  const current = store.read(binding, account);
  if (current.pending_action?.kind !== "choice") throw new Error("choice_required");
  const grant = store.answer(binding, account, rawAnswers), selected = grant.consume(account);
  let committed = false;
  using guard = { [Symbol.dispose]() {
    if (!committed) store.complete(grant.binding(), account, { batch: { outcomes: [{ ...current.batch.outcomes[0]!, disposition: { kind: "failed", code: "choice_snapshot_invalid" }, summary: "The choice could not be completed." }] }, presentation: current.presentation, terminal: { kind: "failed", code: "choice_snapshot_invalid", summary: "The choice could not be completed.", retryable: false }, continuation: null });
  } };
  const envelope = envelopeSchema.parse(parseApplicationJson(selected.payload));
  if (envelope.call_id !== grant.context().call_id || envelope.operation_id !== grant.context().operation_id || !isDeepStrictEqual(envelope.questions, current.pending_action.questions) || current.batch.outcomes.length !== 1) throw new Error("choice_snapshot_mismatch");
  const questions = envelope.questions, answers = selected.answers, chinese = current.locale === "zh-CN";
  const prefix = chinese ? "我已完成 OneCrew 选择：" : "I have completed the OneCrew choice:", separator = chinese ? "：" : ": ", suffix = chinese ? "\n请按这些已确认的选择继续当前任务；不要重复提交选择。" : "\nContinue the current task using these answers; do not submit the choice again.";
  let message = prefix, requiresModelContext = answers.some(answer => "option_ids" in answer);
  for (let index = 0; index < questions.length; index++) {
    const question = questions[index]!, answer = answers[index]!, text = answerText(question, answer), header = bounded(question.header, "选择"), value = "text" in answer ? text : bounded(text, "已选择");
    requiresModelContext ||= header !== question.header || value !== text;
    const line = `\n- ${header}${separator}${value}`;
    if (resultByteLength(message + line + suffix) > 4096) { requiresModelContext = true; break; }
    message += line;
  }
  message += suffix;
  const result = store.complete(grant.binding(), account, { batch: choiceBatch(grant.context(), true), presentation: { kind: "choice", title: bounded(current.presentation.title, "Choice completed"), summary: "All answers were recorded.", sections: [{ kind: "items", heading: "Submitted answers", items: questions.map((question, index) => { const answer = answers[index]!, text = answerText(question, answer); return { title: preview(question.header, "Question"), description: "text" in answer ? text : preview(text, "Selected option") }; }) }] }, terminal: { kind: "resolved", code: "choice_selected", summary: "The user completed the requested choice." }, continuation: { message, requires_model_context: requiresModelContext, context: { kind: "choice", answers } } });
  committed = true;
  return result;
}
