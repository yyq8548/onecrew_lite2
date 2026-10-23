import type {
  ResultChoiceAnswer,
  ResultChoiceQuestion,
  SearchPlanEditorDraft,
} from "./types";

export const SEARCH_PLAN_MAX_BYTES = 512 * 1024;
export const SEARCH_PLAN_DESCRIPTION_MAX_BYTES = 8192;
export const SEARCH_PLAN_KEYWORD_MAX_COUNT = 50;
export const SEARCH_PLAN_KEYWORD_MAX_BYTES = 512;
export const SEARCH_PLAN_SCOPE_MAX_COUNT = 200;

const utf8Bytes = (text: string) => new TextEncoder().encode(text).length;
const CONTROL_CHARACTER = /[\u0000-\u001f\u007f-\u009f]/u;

// These mirror SearchPlanEditorDraft::validate_typed (platform-runtime
// search_plan.rs): description, keywords, and scope count. Scope arguments are
// operation-owned and validated by the registry, never here.
export function searchPlanDescriptionValid(description: string): boolean {
  return description.trim().length > 0 && utf8Bytes(description) <= SEARCH_PLAN_DESCRIPTION_MAX_BYTES;
}

export function searchPlanKeywordsValid(keywords: string[]): boolean {
  return keywords.length <= SEARCH_PLAN_KEYWORD_MAX_COUNT && new Set(keywords).size === keywords.length &&
    keywords.every((word) => word.length > 0 && word === word.trim() && utf8Bytes(word) <= SEARCH_PLAN_KEYWORD_MAX_BYTES && !CONTROL_CHARACTER.test(word));
}

export function searchPlanScopesValid(scopes: SearchPlanEditorDraft["scopes"]): boolean {
  return scopes.length >= 1 && scopes.length <= SEARCH_PLAN_SCOPE_MAX_COUNT &&
    scopes.every((scope) => scope.operation_id.trim().length > 0 && scope.schema_revision.trim().length > 0);
}

export function searchPlanEditorDraftValid(draft: SearchPlanEditorDraft): boolean {
  return utf8Bytes(JSON.stringify(draft)) <= SEARCH_PLAN_MAX_BYTES && searchPlanDescriptionValid(draft.description) &&
    searchPlanKeywordsValid(draft.keywords) && searchPlanScopesValid(draft.scopes);
}

export function choiceAnswersValid(questions: ResultChoiceQuestion[], answers: ResultChoiceAnswer[]): boolean {
  if (answers.length !== questions.length || !choiceAnswersWithinBudget(answers)) return false;
  return questions.every((question, index) => {
    const answer = answers[index];
    if (answer?.question_id !== question.id || !choiceAnswerShapeValid(answer)) return false;
    if ("text" in answer) return true;
    if (question.selection_mode === "multiple") return "option_ids" in answer && answer.option_ids.every(id => question.options.some(option => option.id === id));
    return "option_id" in answer && question.options.some(option => option.id === answer.option_id);
  });
}

export function customAnswerTextValid(text: string): boolean {
  return text.trim().length > 0 && new TextEncoder().encode(text).length <= 2048 &&
    !/[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/u.test(text);
}

export function choiceAnswerShapeValid(answer: ResultChoiceAnswer): boolean {
  return Object.keys(answer).length === 2 && ["option_id", "option_ids", "text"].filter(key => Object.hasOwn(answer, key)).length === 1 && /^[A-Za-z0-9._:-]{1,128}$/u.test(answer.question_id) &&
    ("option_id" in answer ? /^[A-Za-z0-9._:-]{1,128}$/u.test(answer.option_id) : "option_ids" in answer
      ? answer.option_ids.length > 0 && answer.option_ids.length <= 32 && new Set(answer.option_ids).size === answer.option_ids.length && answer.option_ids.every(id => /^[A-Za-z0-9._:-]{1,128}$/u.test(id))
      : customAnswerTextValid(answer.text));
}

export function choiceAnswersWithinBudget(answers: ResultChoiceAnswer[]): boolean {
  return new TextEncoder().encode(JSON.stringify(answers)).length <= 24 * 1024;
}

export function normalizeEditInstruction(value: string): string | null {
  const normalized = value.trim();
  if (!normalized || new TextEncoder().encode(normalized).length > 2_048 || /[\u0000-\u001F\u007F-\u009F]/u.test(normalized)) {
    return null;
  }
  return normalized;
}
