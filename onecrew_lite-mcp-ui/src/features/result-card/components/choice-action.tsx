import { useEffect, useRef } from "react";
import { ExpandableText as SharedExpandableText } from "@mcp-ui/features/result-card/components/expandable-text";
import { Button } from "@mcp-ui/components/ui/button";
import { Checkbox } from "@mcp-ui/components/ui/checkbox";
import { Label } from "@mcp-ui/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@mcp-ui/components/ui/radio-group";
import { Textarea } from "@mcp-ui/components/ui/textarea";
import { useAppLocale } from "@mcp-ui/lib/i18n";
import type { ResultChoiceAnswer, ResultChoiceQuestion, PresentationProjection } from "@mcp-ui/lib/result-contract";
import { choiceAnswersWithinBudget, customAnswerTextValid } from "@mcp-ui/lib/result-contract/invariants";
import { answerLabel, canSubmitChoice, choiceAnswers, draftFor, type ChoiceState, type ChoiceAction as DraftAction } from "@mcp-ui/features/result-card/choice-state";

function ExpandableText({ text }: { text: string }) {
  return <SharedExpandableText text={text} maxCharacters={100} maxLines={3} />;
}

/** The wizard's actions, rendered in the shared card footer.
 *
 * Question stage: leave the step (previous question, or cancel the whole
 * request) and advance; review stage: cancel and submit. The labels, the
 * enabling rules, and the once-only submission are the wizard's, unchanged by
 * living in the footer. */
export function ChoiceActions({ questions, state, disabled, dispatch, onCancel, onSubmit }: {
  questions: ResultChoiceQuestion[]; state: ChoiceState; disabled: boolean;
  dispatch: (action: DraftAction) => void; onCancel: () => void; onSubmit: (answers: ResultChoiceAnswer[]) => void;
}) {
  const { t } = useAppLocale();
  const answers = choiceAnswers(questions, state);
  if (state.editor) {
    const valid = customAnswerTextValid(state.editor.text);
    return <>
      <Button type="button" variant="ghost" className="mr-auto" disabled={disabled} onClick={() => dispatch({ type: "close_custom" })}>{t("closeChoiceEditor")}</Button>
      <Button type="button" disabled={disabled || !valid} onClick={() => { if (!disabled && valid) dispatch({ type: "apply_custom" }); }}>{disabled ? t("processing") : t("useCustomAnswer")}</Button>
    </>;
  }
  if (state.stage === "review") {
    const canSubmit = canSubmitChoice(questions, state);
    return <>
      <Button type="button" variant="ghost" className="mr-auto" disabled={disabled} onClick={onCancel}>{t("cancel")}</Button>
      <Button type="button" disabled={disabled || !canSubmit} onClick={() => { if (!disabled && canSubmit) onSubmit(answers); }}>{disabled ? t("processing") : t("submit")}</Button>
    </>;
  }
  const question = questions[Math.min(state.questionIndex, questions.length - 1)];
  if (!question) return null;
  const draft = draftFor(state, question.id);
  const last = state.questionIndex === questions.length - 1;
  return <>
    {!state.returnToReview && <Button type="button" variant="ghost" className="mr-auto" disabled={disabled} onClick={() => state.questionIndex ? dispatch({ type: "question", index: state.questionIndex - 1 }) : onCancel()}>{state.questionIndex ? t("previousQuestion") : t("cancel")}</Button>}
    <Button type="button" disabled={disabled || answerLabel(question, draft) === null} onClick={() => { if (!disabled) dispatch({ type: "next" }); }}>{disabled ? t("processing") : state.returnToReview ? t("saveEdit") : last ? t("review") : t("choiceNext")}</Button>
  </>;
}

export function ChoiceAction({ questions, state, dispatch, disabled }: {
  questions: ResultChoiceQuestion[]; state: ChoiceState; dispatch: (action: DraftAction) => void; disabled: boolean;
}) {
  const { t } = useAppLocale();
  const question = questions[state.questionIndex]!;
  const draft = draftFor(state, question.id);
  const answers = choiceAnswers(questions, state);
  const multiple = question.selection_mode === "multiple";
  const root = useRef<HTMLElement>(null);
  const previousEditor = useRef<string | null>(null);
  const selectValue = (value: string) => {
    if (value === "custom-input") dispatch({ type: "custom" });
    else { const option = question.options[Number(value.slice(7))]; if (option) dispatch({ type: "option", id: option.id }); }
  };
  const options = question.options.map((option, index) => {
    const id = `choice-option-${state.questionIndex}-${index}`;
    return <div key={option.id} className="grid gap-1 rounded-xl border px-3 py-2 has-[[data-state=checked]]:border-primary has-[[data-state=checked]]:bg-muted/50">
      <Label htmlFor={id} className="flex cursor-pointer items-start gap-2 text-sm leading-5">
        {multiple ? <Checkbox id={id} className="mt-0.5 rounded-sm" disabled={disabled} checked={draft.mode === "option" && draft.optionIds.includes(option.id)} onCheckedChange={checked => dispatch({ type: "option", id: option.id, checked: checked === true })} /> : <RadioGroupItem id={id} className="mt-0.5" value={`option:${index}`} />}
        <span className="min-w-0 break-words">{option.label}</span>
      </Label>
      {option.description && <div className="pl-6 text-muted-foreground"><ExpandableText text={option.description} /></div>}
    </div>;
  });
  const heading = useRef<HTMLHeadingElement>(null);
  const stepKey = `${state.stage}:${state.questionIndex}`;
  const previousQuestion = useRef(stepKey);
  useEffect(() => {
    if (previousQuestion.current !== stepKey) heading.current?.focus();
    previousQuestion.current = stepKey;
  }, [stepKey]);
  useEffect(() => {
    const previous = previousEditor.current;
    if (state.editor) root.current?.querySelector<HTMLTextAreaElement>("textarea")?.focus();
    else if (previous != null) root.current?.querySelector<HTMLElement>("[data-choice-custom-trigger]")?.focus();
    previousEditor.current = state.editor?.questionId ?? null;
  }, [state.editor]);
  if (state.stage === "review") return <ChoiceReview questions={questions} state={state} disabled={disabled} onEditQuestion={index => dispatch({ type: "edit_question", index })} />;
  return <section ref={root} aria-labelledby="choice-question-heading" className="grid gap-4" data-choice-stage="question">
    <span className="text-xs text-muted-foreground">{t("questionProgress", { current: state.questionIndex + 1, total: questions.length })}</span>
    <div data-choice-scroll="true" className="grid min-h-0 gap-3">
        <div className="grid gap-2"><h2 ref={heading} tabIndex={-1} id="choice-question-heading" className="break-words text-sm font-semibold outline-none">{question.header}</h2><div id="choice-question-prompt" className="text-muted-foreground"><ExpandableText text={question.prompt} /></div></div>
        {state.editor ? <div data-choice-editor="true" className="grid gap-2"><Label htmlFor="choice-custom-answer">{t("customLabel")}</Label><Textarea id="choice-custom-answer" rows={4} className="field-sizing-fixed h-28 min-h-28 resize-none" maxLength={2048} value={state.editor.text} disabled={disabled} placeholder={t("customPlaceholder")} aria-describedby={state.editor.text.length > 0 && !customAnswerTextValid(state.editor.text) ? "choice-custom-help" : undefined} aria-invalid={state.editor.text.length > 0 && !customAnswerTextValid(state.editor.text)} onChange={event => dispatch({ type: "text", text: event.target.value })} />{state.editor.text.length > 0 && !customAnswerTextValid(state.editor.text) && <p id="choice-custom-help" role="alert" className="text-xs text-destructive">{t("customInvalid")}</p>}</div>
          : multiple ? <div role="group" aria-labelledby="choice-question-heading" aria-describedby="choice-question-prompt" className="grid gap-2">{options}<Label className="flex cursor-pointer items-center gap-2 rounded-xl border border-dashed px-3 py-2"><Checkbox data-choice-custom-trigger="true" className="rounded-sm" disabled={disabled} checked={draft.mode === "custom"} onCheckedChange={checked => dispatch({ type: checked === true ? "custom" : "options" })} /><span className="text-sm">{t("custom")}</span></Label></div>
          : <RadioGroup disabled={disabled} value={draft.mode === "custom" ? "custom-input" : draft.optionIds.length ? `option:${question.options.findIndex(option => option.id === draft.optionIds[0])}` : ""} onValueChange={selectValue} aria-labelledby="choice-question-heading" aria-describedby="choice-question-prompt" className="gap-2">{options}<Label htmlFor="choice-custom-option" className="flex cursor-pointer items-center gap-2 rounded-xl border border-dashed px-3 py-2"><RadioGroupItem data-choice-custom-trigger="true" id="choice-custom-option" value="custom-input" /><span className="text-sm">{t("custom")}</span></Label></RadioGroup>}
    </div>
    {!choiceAnswersWithinBudget(answers) && <p role="alert" className="text-sm text-destructive">{t("answersTooLarge")}</p>}
  </section>;

}

export function ChoiceReview({ questions, state, disabled, onEditQuestion }: {
  questions: ResultChoiceQuestion[]; state: ChoiceState; disabled: boolean; onEditQuestion: (index: number) => void;
}) {
  const { t } = useAppLocale();
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => { heading.current?.focus(); }, []);
  const answers = choiceAnswers(questions, state);
  return <section aria-labelledby="choice-review-heading" className="grid gap-4" data-choice-stage="review">
    <div className="grid gap-2"><h2 ref={heading} tabIndex={-1} id="choice-review-heading" className="text-sm font-semibold outline-none">{t("reviewTitle")}</h2><p className="text-sm text-muted-foreground">{t("reviewHelp")}</p></div>
    <ol className="divide-y">{questions.map((question, index) => {
      const draft = draftFor(state, question.id);
      const label = answerLabel(question, draft);
      return <li key={question.id} className="grid gap-1 py-2 first:pt-0 last:pb-0">
        <div className="flex items-start justify-between gap-3"><h3 className="min-w-0 whitespace-pre-wrap break-words text-sm font-medium">{question.header}</h3><Button type="button" variant="ghost" size="sm" className="h-6 shrink-0 px-2 text-xs" disabled={disabled} aria-label={t("choiceEditAnswerFor", { question: question.header })} onClick={() => onEditQuestion(index)}>{t("editAnswer")}</Button></div>
        <div className="min-w-0 text-muted-foreground"><ExpandableText text={label ?? t("unanswered")} /></div>
      </li>;
    })}</ol>
    {!choiceAnswersWithinBudget(answers) && <p role="alert" className="text-sm text-destructive">{t("answersTooLarge")}</p>}
  </section>;
}

export function SubmittedChoice({ projection, submitted }: { projection: PresentationProjection; submitted: boolean }) {
  const { t } = useAppLocale();
  const items = projection.sections.flatMap(section => section.kind === "items" ? section.items : []);
  return <section className="grid gap-3"><h2 className="text-sm font-medium">{t(submitted ? "submittedAnswers" : "questions")}</h2><ul data-choice-scroll="true" className="divide-y">{items.map((item, index) => <li key={index} className="grid gap-1 py-2 first:pt-0 last:pb-0"><h3 className="break-words text-sm font-medium">{item.title}</h3>{item.description && <div className="min-w-0 text-muted-foreground"><ExpandableText text={item.description} /></div>}</li>)}</ul></section>;
}
