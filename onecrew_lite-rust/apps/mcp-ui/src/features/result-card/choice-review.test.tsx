import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { ChoiceAction, ChoiceActions, ChoiceReview } from "@mcp-ui/features/result-card/components/choice-action";
import { canSubmitChoice, choiceAnswers, draftFor, initialChoiceState, reduceChoice } from "@mcp-ui/features/result-card/choice-state";
import { ResultCardView } from "@mcp-ui/features/result-card/view";
import { AppLocaleProvider } from "@mcp-ui/lib/i18n";
import { parseResultSnapshot, type ResultChoiceQuestion } from "@mcp-ui/lib/result-contract";
import { simulateChoiceResult } from "@mcp-ui/test-support/choice-result";
import { choiceSnapshot } from "@mcp-ui/test-support/fixtures";

const questions: ResultChoiceQuestion[] = [
  { id: "market", header: "Target market", prompt: "Where should cookware creators reach customers?", selection_mode: "single", options: [{ id: "us", label: "United States", description: null }, { id: "jp", label: "Japan", description: null }] },
  { id: "goals", header: "Collaboration goals", prompt: "Which goals should the partnership support?", selection_mode: "multiple", options: [{ id: "review", label: "Product reviews", description: null }, { id: "sales", label: "Drive sales", description: null }] },
  { id: "notes", header: "Additional requirements", prompt: "What else should we consider?", selection_mode: "single", options: [{ id: "none", label: "No extra requirements", description: null }] },
];

function completedDraft() {
  let state = initialChoiceState();
  for (const action of [
    { type: "option", id: "us" }, { type: "next" },
    { type: "option", id: "sales" }, { type: "option", id: "review" }, { type: "next" },
    { type: "custom" }, { type: "text", text: "Use real kitchens.\nKeep the full answer." }, { type: "apply_custom" },
  ] as const) state = reduceChoice(state, action, questions);
  return state;
}

// The wizard's step and review actions now live in the card's one shared
// footer, so each stage is rendered here as the card renders it: the stage body
// plus the action bar built from the same state.
describe("Choice review before submission", () => {
  it("moves the last answer into review and only then becomes submittable", () => {
    const answered = completedDraft();
    expect(answered.stage).toBe("question");
    expect(answered.questionIndex).toBe(2);
    expect(choiceAnswers(questions, answered)).toHaveLength(3);
    expect(canSubmitChoice(questions, answered)).toBe(false);
    const reviewed = reduceChoice(answered, { type: "next" }, questions);
    expect(reviewed.stage).toBe("review");
    expect(canSubmitChoice(questions, reviewed)).toBe(true);
    expect(reviewed.drafts).toEqual(answered.drafts);
  });

  it("never labels the last question as a formal submission", () => {
    const only = [questions[0]!];
    const state = initialChoiceState();
    const html = renderToStaticMarkup(<AppLocaleProvider locale="en-US">
      <ChoiceAction questions={only} state={state} dispatch={vi.fn()} disabled={false} />
      <ChoiceActions questions={only} state={state} disabled={false} dispatch={vi.fn()} onCancel={vi.fn()} onSubmit={vi.fn()} />
    </AppLocaleProvider>);
    expect(html).toContain("Review answers");
    expect(html).not.toContain("Submit answers");
    expect(html).toContain('data-choice-stage="question"');
  });

  it("shows a compact header and answer list without repeating prompts or per-question cards", () => {
    const state = reduceChoice(completedDraft(), { type: "next" }, questions);
    const html = renderToStaticMarkup(<AppLocaleProvider locale="en-US">
      <ChoiceReview questions={questions} state={state} disabled={false} onEditQuestion={vi.fn()} />
      <ChoiceActions questions={questions} state={state} disabled={false} dispatch={vi.fn()} onCancel={vi.fn()} onSubmit={vi.fn()} />
    </AppLocaleProvider>);
    for (const question of questions) { expect(html).toContain(question.header); expect(html).not.toContain(question.prompt); }
    for (const answer of ["United States", "Product reviews\nDrive sales", "Use real kitchens.\nKeep the full answer."]) expect(html).toContain(answer);
    expect(html.match(/aria-label="Edit answer to/g)).toHaveLength(3);
    expect(html).toContain("Nothing is sent until you submit");
    expect(html).toContain("Submit answers");
    expect(html).not.toContain("<details");
    expect(html).not.toContain("rounded-xl border p-3");
    expect(html).not.toContain("bg-muted/40");
  });

  it("edits one reviewed answer and returns directly to review while preserving other drafts", () => {
    const before = reduceChoice(completedDraft(), { type: "next" }, questions);
    let state = reduceChoice(before, { type: "edit_question", index: 0 }, questions);
    expect(state.returnToReview).toBe(true);
    expect(draftFor(state, "market").optionIds).toEqual(["us"]);
    expect(canSubmitChoice(questions, state)).toBe(false);
    state = reduceChoice(state, { type: "option", id: "jp" }, questions);
    state = reduceChoice(state, { type: "next" }, questions);
    expect(state.stage).toBe("review");
    expect(choiceAnswers(questions, state)[0]).toEqual({ question_id: "market", option_id: "jp" });
    expect(draftFor(state, "goals")).toEqual(draftFor(before, "goals"));
    expect(draftFor(state, "notes")).toEqual(draftFor(before, "notes"));
    expect(canSubmitChoice(questions, state)).toBe(true);
  });

  it("replaces the option body with an independent custom editor and puts adopt/close in the shared footer", () => {
    const selected = reduceChoice(initialChoiceState(), { type: "option", id: "us" }, questions);
    const editing = reduceChoice(selected, { type: "custom" }, questions);
    const body = renderToStaticMarkup(<AppLocaleProvider locale="en-US">
      <ChoiceAction questions={questions} state={editing} dispatch={vi.fn()} disabled={false} />
    </AppLocaleProvider>);
    const footer = renderToStaticMarkup(<AppLocaleProvider locale="en-US">
      <ChoiceActions questions={questions} state={editing} disabled={false} dispatch={vi.fn()} onCancel={vi.fn()} onSubmit={vi.fn()} />
    </AppLocaleProvider>);
    expect(body).toContain('data-choice-editor="true"');
    expect(body).toContain('id="choice-custom-answer"');
    expect(body).not.toContain("United States");
    expect(footer).toContain("Use this answer");
    expect(footer).toContain("Close editor");
    expect(footer).not.toContain("Submit answers");
    expect(draftFor(editing, "market").optionIds).toEqual(["us"]);
  });

  it("keeps incomplete review and busy review from enabling formal submission", () => {
    for (const [state, disabled] of [[{ ...initialChoiceState(), stage: "review" as const }, false], [reduceChoice(completedDraft(), { type: "next" }, questions), true]] as const) {
      const html = renderToStaticMarkup(<AppLocaleProvider locale="en-US"><ChoiceActions questions={questions} state={state} disabled={disabled} dispatch={vi.fn()} onCancel={vi.fn()} onSubmit={vi.fn()} /></AppLocaleProvider>);
      const buttons = [...html.matchAll(/<button\b[^>]*>[\s\S]*?<\/button>/g)].map(match => match[0]);
      expect(buttons.at(-1)).toContain("disabled=");
    }
  });

  it("renders a protocol-shaped simulated completion with all question and answer results", () => {
    const initial = choiceSnapshot(); initial.locale = "en-US"; initial.pending_action = { kind: "choice", questions };
    const before = structuredClone(initial);
    const result = simulateChoiceResult(initial, choiceAnswers(questions, completedDraft()));
    expect(initial).toEqual(before);
    expect(parseResultSnapshot(result)).toEqual(result);
    expect(result.state).toMatchObject({ kind: "resolved", code: "choice_selected" });
    expect(result.pending_action).toBeNull();
    expect(result.continuation.state).toBe("not_requested");
    const html = renderToStaticMarkup(<ResultCardView snapshot={result} busy={false} onConfirm={vi.fn()} onCancel={vi.fn()} onRequestEdit={vi.fn()} onSelect={vi.fn()} />);
    for (const text of ["Submitted answers", "Target market", "United States", "Collaboration goals", "Product reviews\nDrive sales", "Additional requirements", "Use real kitchens.\nKeep the full answer."]) expect(html).toContain(text);
    expect(html).not.toContain("Submit answers</button>");
    expect(html).not.toContain("rounded-xl border p-3");
    if (result.state.kind !== "resolved") throw new Error("Expected completed preview");
    expect(result.state.summary).toContain("Simulation");
  });
});
