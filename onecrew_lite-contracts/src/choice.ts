import { z } from "zod";
import { resultByteLength, resultIdentifier, resultText } from "./fields.ts";
import { stringifyApplicationJson } from "./json.ts";

export const resultChoiceQuestion = z.object({
  selection_mode: z.enum(["single", "multiple"]).default("single"),
  id: resultIdentifier(), header: resultText, prompt: resultText,
  options: z.array(z.object({ id: resultIdentifier(), label: resultText, description: resultText.nullish().transform(value => value ?? null) }).strict()).min(1).max(32).refine(value => new Set(value.map(option => option.id)).size === value.length, "duplicate_choice_option"),
}).strict();
export const resultChoiceQuestions = z.array(resultChoiceQuestion).min(1).max(32).refine(value => new Set(value.map(question => question.id)).size === value.length, "duplicate_choice_question");
export const resultChoiceAnswer = z.union([
  z.object({ question_id: resultIdentifier(), option_ids: z.array(resultIdentifier()).min(1).max(32).refine(value => new Set(value).size === value.length, "duplicate_choice_answer_option") }).strict(),
  z.object({ question_id: resultIdentifier(), option_id: resultIdentifier() }).strict(),
  z.object({ question_id: resultIdentifier(), text: resultText.refine(value => resultByteLength(value) <= 2048 && !/[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/u.test(value), "choice_answer_text") }).strict(),
]);
export const resultChoiceAnswers = z.array(resultChoiceAnswer).min(1).max(32).refine(value => new Set(value.map(answer => answer.question_id)).size === value.length, "duplicate_choice_answer").refine(value => resultByteLength(stringifyApplicationJson(value)) <= 24 * 1024, "choice_answers_too_large");
export type ResultChoiceQuestion = z.infer<typeof resultChoiceQuestion>;
export type ResultChoiceAnswer = z.infer<typeof resultChoiceAnswer>;

export function validateChoiceAnswers(questionInput: readonly ResultChoiceQuestion[], answerInput: unknown): ResultChoiceAnswer[] {
  const questions = resultChoiceQuestions.parse(questionInput), answers = resultChoiceAnswers.parse(answerInput);
  if (questions.length !== answers.length) throw new Error("choice_answer_count");
  for (const answer of answers) if (!questions.some(question => question.id === answer.question_id)) throw new Error("unknown_choice_question");
  return questions.map(question => {
    const answer = answers.find(candidate => candidate.question_id === question.id);
    if (!answer) throw new Error("missing_choice_answer");
    if ("option_ids" in answer) {
      if (question.selection_mode !== "multiple" || answer.option_ids.some(id => !question.options.some(option => option.id === id))) throw new Error("unknown_choice_option");
      return { question_id: answer.question_id, option_ids: question.options.filter(option => answer.option_ids.includes(option.id)).map(option => option.id) };
    }
    if ("option_id" in answer && (question.selection_mode !== "single" || !question.options.some(option => option.id === answer.option_id))) throw new Error("unknown_choice_option");
    return answer;
  });
}
