import type { ClaimedResultContinuation, ResultContinuationContext } from "@mcp-ui/lib/result-contract";
import { translate, type AppLocale } from "@mcp-ui/lib/i18n";

function visibleControls(text: string): string {
  return text.replace(/[\u0000-\u0009\u000b-\u001f\u007f-\u009f]/gu, (character) => `\\u${character.charCodeAt(0).toString(16).padStart(4, "0")}`);
}

// Strings are explicitly data: a single-line string is one JSON string literal
// (its escaping covers quotes and control characters), and every line of a
// multi-line string stays inside a quote block. Scalars retain their type;
// empty arrays/objects and false/0/null never vanish.
function dataField(label: string, value: unknown, locale: AppLocale, depth = 0): string[] {
  const pad = "  ".repeat(depth);
  const t = (key: Parameters<typeof translate>[1]) => translate(locale, key);
  if (value === null) return [`${pad}- ${label}: null`];
  if (value === undefined) return [`${pad}- ${label}: ${t("contextAbsent")}`];
  if (typeof value === "string") {
    if (value.length === 0) return [`${pad}- ${label}: ${t("contextEmptyText")}`];
    if (!value.includes("\n")) return [`${pad}- ${label}: ${visibleControls(JSON.stringify(value))}`];
    return [`${pad}- ${label} (${t("contextTextData")}):`, ...visibleControls(value).split("\n").map((line) => `${pad}  | ${line}`)];
  }
  if (Array.isArray(value)) return value.length
    ? [`${pad}- ${label} (${value.length}):`, ...value.flatMap((item, index) => dataField(String(index + 1), item, locale, depth + 1))]
    : [`${pad}- ${label}: ${t("contextEmptyList")}`];
  if (typeof value === "object") {
    const fields = Object.entries(value);
    return fields.length ? [`${pad}- ${label}:`, ...fields.flatMap(([key, item]) => dataField(key, item, locale, depth + 1))] : [`${pad}- ${label}: ${t("contextEmptyObject")}`];
  }
  return [`${pad}- ${label}: ${String(value)}`];
}

function contextLines(context: ResultContinuationContext, locale: AppLocale): string[] {
  const t = (key: Parameters<typeof translate>[1]) => translate(locale, key);
  switch (context.kind) {
    case "choice":
      return [t("contextChoice"), ...context.answers.flatMap((answer, index) => [
        `${index + 1}. question_id: ${answer.question_id}${"option_id" in answer ? `; option_id: ${answer.option_id}` : "option_ids" in answer ? `; option_ids: ${answer.option_ids.join(", ")}` : ""}`,
        ...("text" in answer ? dataField("custom_answer", answer.text, locale, 1) : []),
      ])];
    case "search_plan_confirmed":
      return [t("contextSearchPlan"), ...dataField("confirmed_plan_ref", context.confirmed_plan_ref, locale), ...dataField("description", context.description, locale), ...dataField("keywords", context.keywords, locale)];
    case "creator_save_batch":
      return [t("contextCreatorSaveBatch"), ...dataField("outcomes", context.outcomes, locale)];
    case "execution":
      return [t("contextExecution"), ...dataField("outcomes", context.outcomes, locale)];
    case "cancelled": return [t("contextCancelled")];
    case "edit_requested": return [t("contextEditRequested")];
    default: context satisfies never; return [];
  }
}

export function formatContinuationText(claimed: ClaimedResultContinuation, locale: AppLocale): string {
  return [translate(locale, "contextDataBoundary"), `result_ref: ${claimed.binding.result_ref}`, "", ...contextLines(claimed.request.context, locale)].join("\n");
}
