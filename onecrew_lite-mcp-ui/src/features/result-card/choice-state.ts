import type { ResultChoiceAnswer, ResultChoiceQuestion } from "@mcp-ui/lib/result-contract";
import { choiceAnswersValid, customAnswerTextValid } from "@mcp-ui/lib/result-contract/invariants";
import { textPages } from "@mcp-ui/features/result-card/text-pages";

export type ChoiceDraft = { mode: "option" | "custom"; optionIds: string[]; text: string };
export type ChoiceEditor = { questionId: string; text: string; returnToReview: boolean };
export type ChoiceState = { stage: "question" | "review"; questionIndex: number; returnToReview: boolean; drafts: Record<string, ChoiceDraft>; editor: ChoiceEditor | null };
export const initialChoiceState = (): ChoiceState => ({ stage: "question", questionIndex: 0, returnToReview: false, drafts: {}, editor: null });
export type ChoiceAction =
  | { type: "option"; id: string; checked?: boolean }
  | { type: "custom" } | { type: "text"; text: string } | { type: "options" } | { type: "apply_custom" } | { type: "close_custom" }
  | { type: "question"; index: number } | { type: "edit_question"; index: number }
  | { type: "next" } | { type: "review" } | { type: "back" };

export function draftFor(state: ChoiceState, id: string): ChoiceDraft {
  return Object.hasOwn(state.drafts, id) ? state.drafts[id]! : { mode: "option", optionIds: [], text: "" };
}

export function answerFor(question: ResultChoiceQuestion, draft: ChoiceDraft): ResultChoiceAnswer | null {
  if (draft.mode === "custom") return customAnswerTextValid(draft.text) ? { question_id: question.id, text: draft.text } : null;
  const selected = question.options.filter(option => draft.optionIds.includes(option.id)).map(option => option.id);
  if (selected.length !== draft.optionIds.length || !selected.length) return null;
  return question.selection_mode === "multiple"
    ? { question_id: question.id, option_ids: selected }
    : selected.length === 1 ? { question_id: question.id, option_id: selected[0]! } : null;
}

export function choiceAnswers(questions: ResultChoiceQuestion[], state: ChoiceState): ResultChoiceAnswer[] {
  return questions.flatMap(question => {
    const answer = answerFor(question, draftFor(state, question.id));
    return answer ? [answer] : [];
  });
}

export function answerLabel(question: ResultChoiceQuestion, draft: ChoiceDraft): string | null {
  const answer = answerFor(question, draft);
  if (!answer) return null;
  if ("text" in answer) return answer.text;
  const ids = "option_ids" in answer ? answer.option_ids : [answer.option_id];
  return question.options.filter(option => ids.includes(option.id)).map(option => option.label).join("\n");
}

export function detailPages(text: string): string[] { return textPages(text); }

export function reduceChoice(state: ChoiceState, action: ChoiceAction, questions: ResultChoiceQuestion[]): ChoiceState {
  if (action.type === "close_custom") return state.editor ? { ...state, editor: null } : state;
  if (state.editor) {
    if (action.type === "text") return { ...state, editor: { ...state.editor, text: action.text } };
    if (action.type === "apply_custom") {
      if (!customAnswerTextValid(state.editor.text)) return state;
      const index = questions.findIndex(question => question.id === state.editor!.questionId);
      const question = index < 0 ? undefined : questions[index];
      if (!question) return state;
      return {
        ...state,
        stage: state.editor.returnToReview ? "review" : "question",
        questionIndex: index,
        returnToReview: false,
        editor: null,
        drafts: {
          ...state.drafts,
          [question.id]: { mode: "custom", optionIds: [], text: state.editor.text },
        },
      };
    }
    // Opening another question or changing a committed option while the
    // editor is open would discard the local draft or bypass its footer.
    return state;
  }
  if (action.type === "question" || action.type === "edit_question") return questions[action.index] ? { ...state, stage: "question", questionIndex: action.index, returnToReview: action.type === "edit_question" } : state;
  if (action.type === "review") return { ...state, stage: "review", returnToReview: false };
  if (action.type === "back") return { ...state, stage: "question", returnToReview: false };
  const question = questions[Math.min(state.questionIndex, questions.length - 1)];
  if (!question) return state;
  const draft = draftFor(state, question.id);
  if (action.type === "next") {
    if (!answerFor(question, draft)) return state;
    return state.returnToReview || state.questionIndex === questions.length - 1
      ? { ...state, stage: "review", returnToReview: false }
      : { ...state, questionIndex: state.questionIndex + 1 };
  }
  const update = (next: ChoiceDraft) => ({ ...state, drafts: { ...state.drafts, [question.id]: next } });
  switch (action.type) {
    case "option": {
      if (!question.options.some(option => option.id === action.id)) return state;
      const ids = question.selection_mode === "multiple"
        ? action.checked === false ? draft.optionIds.filter(id => id !== action.id) : [...new Set([...(draft.mode === "option" ? draft.optionIds : []), action.id])]
        : [action.id];
      return update({ ...draft, mode: "option", optionIds: question.options.filter(option => ids.includes(option.id)).map(option => option.id) });
    }
    case "custom": return { ...state, editor: { questionId: question.id, text: draft.mode === "custom" ? draft.text : "", returnToReview: state.returnToReview } };
    case "text": return state;
    case "options": return update({ ...draft, mode: "option", optionIds: [] });
  }
  return state;
}

export function canSubmitChoice(questions: ResultChoiceQuestion[], state: ChoiceState): boolean {
  return state.stage === "review" && choiceAnswersValid(questions, choiceAnswers(questions, state));
}
