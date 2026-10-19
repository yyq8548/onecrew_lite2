import { describe, expect, it } from "vitest";
import { canSubmitChoice, choiceAnswers, detailPages, draftFor, initialChoiceState, reduceChoice } from "@mcp-ui/features/result-card/choice-state";
import type { ResultChoiceQuestion } from "@mcp-ui/lib/result-contract";
function questions(count: number, options = 3, multiple = false): ResultChoiceQuestion[] {
  return Array.from({ length: count }, (_, index) => ({ id: `q${index}`, header: `Question ${index}`, prompt: `Prompt ${index}`, selection_mode: multiple ? "multiple" : "single", options: Array.from({ length: options }, (_, option) => ({ id: `o${option}`, label: `Option ${option}`, description: null })) }));
}
describe("tabbed Choice draft", () => {
  it("preserves selections across tabs and reviews every question in one batch", () => {
    const items = questions(7, 32); let state = initialChoiceState();
    state = reduceChoice(state, { type: "review" }, items); expect(canSubmitChoice(items, state)).toBe(false);
    for (let index = 0; index < items.length; index++) { state = reduceChoice(state, { type: "question", index }, items); state = reduceChoice(state, { type: "option", id: "o31" }, items); }
    state = reduceChoice(state, { type: "review" }, items); expect(canSubmitChoice(items, state)).toBe(true); expect(choiceAnswers(items, state)).toHaveLength(7);
    state = reduceChoice(state, { type: "question", index: 0 }, items); expect(draftFor(state, "q0").optionIds).toEqual(["o31"]);
    state = reduceChoice(state, { type: "option", id: "o2" }, items); state = reduceChoice(state, { type: "review" }, items); expect(canSubmitChoice(items, state)).toBe(true); expect(choiceAnswers(items, state)[0]).toEqual({ question_id: "q0", option_id: "o2" });
  });
  it("canonicalizes multiple selections in option order and removes unchecked values", () => {
    const items = questions(1, 32, true); let state = initialChoiceState();
    for (const id of ["o31", "o2", "o0", "o2"]) state = reduceChoice(state, { type: "option", id }, items);
    expect(choiceAnswers(items, state)).toEqual([{ question_id: "q0", option_ids: ["o0", "o2", "o31"] }]);
    state = reduceChoice(state, { type: "option", id: "o2", checked: false }, items); expect(draftFor(state, "q0").optionIds).toEqual(["o0", "o31"]);
  });
  it("treats custom as an exclusive alternative and preserves custom text when returning", () => {
    const items = questions(1, 3, true); let state = reduceChoice(initialChoiceState(), { type: "option", id: "o1" }, items);
    state = reduceChoice(state, { type: "custom" }, items);
    expect(state.editor).toEqual({ questionId: "q0", text: "", returnToReview: false });
    expect(draftFor(state, "q0")).toEqual({ mode: "option", optionIds: ["o1"], text: "" });
    state = reduceChoice(state, { type: "text", text: "My answer\n第二行" }, items);
    // Typing is local to the editor; the answer remains the pre-edit answer
    // until the user explicitly adopts it.
    expect(choiceAnswers(items, state)).toEqual([{ question_id: "q0", option_ids: ["o1"] }]);
    state = reduceChoice(state, { type: "close_custom" }, items);
    expect(state.editor).toBeNull();
    expect(choiceAnswers(items, state)).toEqual([{ question_id: "q0", option_ids: ["o1"] }]);

    state = reduceChoice(state, { type: "custom" }, items);
    state = reduceChoice(state, { type: "text", text: "My answer\n第二行" }, items);
    state = reduceChoice(state, { type: "apply_custom" }, items);
    expect(choiceAnswers(items, state)).toEqual([{ question_id: "q0", text: "My answer\n第二行" }]);
    state = reduceChoice(state, { type: "custom" }, items);
    expect(state.editor?.text).toBe("My answer\n第二行");
    state = reduceChoice(state, { type: "option", id: "o0" }, items);
    expect(state.editor?.text).toBe("My answer\n第二行");
    expect(choiceAnswers(items, state)).toEqual([{ question_id: "q0", text: "My answer\n第二行" }]);
  });
  it("accepts wire IDs custom and __custom__ as normal options", () => {
    for (const id of ["custom", "__custom__", "option:0"]) { const items = questions(1); items[0]!.options[0]!.id = id; const state = reduceChoice(initialChoiceState(), { type: "option", id }, items); expect(choiceAnswers(items, state)).toEqual([{ question_id: "q0", option_id: id }]); }
  });
  it("rejects unknown options and invalid text without discarding drafts", () => {
    const items = questions(1); const initial = initialChoiceState(); expect(reduceChoice(initial, { type: "option", id: "invented" }, items)).toBe(initial);
    for (const text of ["  ", "字".repeat(683), "text\u0000value", "text\u0085value"]) { let state = reduceChoice(initial, { type: "custom" }, items); state = reduceChoice(state, { type: "text", text }, items); state = reduceChoice(state, { type: "review" }, items); expect(choiceAnswers(items, state)).toEqual([]); expect(canSubmitChoice(items, state)).toBe(false); }
  });
  it("bounds combined submissions while retaining all 32 drafts", () => {
    const items = questions(32); let state = initialChoiceState();
    for (let index = 0; index < items.length; index++) { state = reduceChoice(state, { type: "question", index }, items); state = reduceChoice(state, { type: "custom" }, items); state = reduceChoice(state, { type: "text", text: "x".repeat(1024) }, items); state = reduceChoice(state, { type: "apply_custom" }, items); }
    state = reduceChoice(state, { type: "review" }, items); expect(choiceAnswers(items, state)).toHaveLength(32); expect(canSubmitChoice(items, state)).toBe(false);
  });
  it("handles prototype question IDs and retains generic full text pagination", () => {
    const items = [{ ...questions(1)[0]!, id: "__proto__" }]; const state = reduceChoice(initialChoiceState(), { type: "option", id: "o0" }, items); expect(choiceAnswers(items, state)).toEqual([{ question_id: "__proto__", option_id: "o0" }]);
    const text = "一🖥️ two\n".repeat(400); expect(detailPages(text).join("")).toBe(text);
  });
});
