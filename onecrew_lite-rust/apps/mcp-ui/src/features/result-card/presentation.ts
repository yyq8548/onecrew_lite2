import type { ResultSnapshot } from "@mcp-ui/lib/result-contract";
import type { ResultClientFailure, ContinuationPauseReason } from "@mcp-ui/lib/result-client";
import { translate, type AppLocale } from "@mcp-ui/lib/i18n";
import enErrors from "@mcp-ui/locales/en-US/errors";
import zhErrors from "@mcp-ui/locales/zh-CN/errors";

export type ResultStatusPresentation = {
  badge: { label: string; variant: "secondary" | "outline" | "destructive" };
  terminal: null | {
    label: string; detail: string; code: string; tone: "success" | "neutral" | "warning";
    icon: "success" | "cancelled" | "edit" | "pending" | "warning";
  };
};

/** Only an acknowledged confirmation may begin a new review. A read-current
 * conflict refresh must keep the existing draft visible until its own review. */
export function startsNextReview(previous: ResultSnapshot, next: ResultSnapshot): boolean {
  return previous.state.kind === "available" && previous.pending_action != null &&
    next.state.kind === "available" && next.pending_action != null &&
    next.binding.result_ref === previous.binding.result_ref &&
    next.binding.revision > previous.binding.revision;
}

export function reviewRevisionAfterSnapshot(current: number, previous: ResultSnapshot, next: ResultSnapshot, confirmed: boolean): number {
  return startsNextReview(previous, next) && (confirmed || next.completed_sections_count > previous.completed_sections_count)
    ? next.binding.revision : current;
}

export function resultStatusPresentation(snapshot: ResultSnapshot): ResultStatusPresentation {
  const t = (key: Parameters<typeof translate>[1]) => translate(snapshot.locale, key);
  const state = snapshot.state;
  if (state.kind === "available") {
    return { badge: { label: t(snapshot.binding.view.kind === "read_only" ? "statusReadOnly" : "statusConfirm"), variant: "secondary" }, terminal: null };
  }
  if (state.kind === "submitted") {
    return { badge: { label: t("statusProcessing"), variant: "outline" }, terminal: {
      label: t("statusProcessing"), detail: t("statusSubmitted"), code: state.action.kind, tone: "neutral", icon: "pending",
    } };
  }
  if (state.kind === "resolved" && (state.code === "user_cancelled" || state.code === "edit_requested")) {
    const cancelled = state.code === "user_cancelled";
    const label = t(cancelled ? "statusCancelled" : "statusEditRequested");
    return { badge: { label, variant: "outline" }, terminal: { label, detail: t(cancelled ? "cancelledDetail" : "editRequestedDetail"), code: state.code, tone: "neutral", icon: cancelled ? "cancelled" : "edit" } };
  }
  if (snapshot.presentation.kind === "creator_save_batch_confirmation" || snapshot.presentation.kind === "creator_save_batch_outcome") {
    const rows = snapshot.presentation.sections.flatMap(section => section.kind === "creator_save_rows" ? section.rows : []);
    if (rows.length > 0) {
      const saved = rows.filter(row => row.status === "saved").length;
      const unknown = state.kind === "unknown" || rows.some(row => row.status === "unknown");
      const incomplete = rows.some(row => row.status === "failed" || row.status === "not_run");
      const success = state.kind === "resolved" && state.code === "creator_save_completed" && saved > 0 && !unknown && !incomplete && rows.every(row => row.status === "saved" || row.status === "not_selected");
      const label = success ? translate(snapshot.locale, "creatorBatchSavedCount", { count: saved }) : t(unknown ? "statusUnknown" : saved > 0 ? "creatorBatchPartial" : "statusFailed");
      return { badge: { label, variant: success ? "secondary" : "outline" }, terminal: {
        label, detail: success ? "" : (state.kind === "failed" || state.kind === "unknown") && !rows.some(row => row.status === "failed" || row.status === "unknown" || row.status === "not_run") ? state.summary : ([
          ["creatorBatchSaved", saved], ["statusFailed", rows.filter(row => row.status === "failed").length],
          ["statusUnknown", rows.filter(row => row.status === "unknown").length], ["creatorBatchNotRun", rows.filter(row => row.status === "not_run").length],
          ["creatorBatchSkipped", rows.filter(row => row.status === "not_selected").length],
        ] as const).filter(([, count]) => count > 0).map(([key, count]) => `${t(key)} ${count}`).join(" · "), code: state.code, tone: success ? "success" : "warning", icon: success ? "success" : "warning",
      } };
    }
  }
  if (state.kind === "resolved") {
    if (snapshot.presentation.kind === "search_plan_editor" && state.code === "search_plan_confirmed") return { badge: { label: t("searchPlanConfirmed"), variant: "secondary" }, terminal: {
      label: t("searchPlanConfirmed"), detail: t("searchPlanConfirmedHelp"), code: state.code, tone: "success", icon: "success",
    } };
    const cancelled = state.code === "user_cancelled";
    const editing = state.code === "edit_requested";
    const label = t(cancelled ? "statusCancelled" : editing ? "statusEditRequested" : "statusCompleted");
    return { badge: { label, variant: cancelled || editing ? "outline" : "secondary" }, terminal: {
      label, detail: state.code === "choice_selected" ? t("completedChoice") : state.summary, code: state.code,
      tone: cancelled || editing ? "neutral" : "success", icon: cancelled ? "cancelled" : editing ? "edit" : "success",
    } };
  }
  if ((state.kind === "failed" || state.kind === "unknown") && state.code === "presentation_unavailable") {
    const unknown = state.kind === "unknown";
    const label = t(unknown ? "presentationUnavailableUnknownTitle" : "presentationUnavailableTitle");
    return { badge: { label, variant: "outline" }, terminal: {
      label, detail: t(unknown ? "presentationUnavailableUnknownDetail" : "presentationUnavailableDetail"), code: state.code, tone: "warning", icon: "warning",
    } };
  }
  const failed = state.kind === "failed";
  return { badge: { label: t(failed ? "statusFailed" : "statusUnknown"), variant: failed ? "destructive" : "outline" }, terminal: {
    label: t(failed ? "statusFailedTitle" : "statusUnknown"), detail: state.summary, code: state.code, tone: "warning", icon: "warning",
  } };
}

export function continuationStatusCopy(state: ResultSnapshot["continuation"]["state"], locale: AppLocale = "en-US", reason?: ContinuationPauseReason | null): string | null {
  if (state === "not_requested" || state === "delivered") return null;
  if (state === "paused" && reason) return translate(locale, { text_unsupported: "continuationTextUnsupported", text_rejected: "continuationTextRejected", message_rejected: "continuationMessageRejected" }[reason] as Parameters<typeof translate>[1]);
  return translate(locale, {
    pending: "continuationPending", dispatching: "continuationDispatching", delivered: "continuationDelivered",
    paused: "continuationPaused", delivery_unknown: "continuationUnknown",
  }[state] as Parameters<typeof translate>[1]);
}

export function resultClientFailureCopy(failure: ResultClientFailure, locale: AppLocale = "en-US"): { title: string; detail: string; code: string; diagnostics?: string } {
  const t = (key: Parameters<typeof translate>[1]) => translate(locale, key);
  if (failure.kind === "transport") return { title: t("transportTitle"), detail: t("transportDetail"), code: "transport" };
  if (failure.kind === "invalid_response") {
    const diagnostics = failure.diagnostics?.map((item) => `schemaPath=${item.schemaPath}; keyword=${item.keyword}${item.missingProperty ? `; missingProperty=${item.missingProperty}` : ""}`).join("\n");
    return { title: t("invalidTitle"), detail: t("invalidDetail"), diagnostics: [(locale === "zh-CN" ? zhErrors : enErrors)[failure.reason], diagnostics].filter(Boolean).join("\n"), code: `invalid_response.${failure.reason}` };
  }
  const code = failure.failure.code;
  const keys = {
    absent: ["absentTitle", "absentDetail"], expired: ["expiredTitle", "expiredDetail"],
    view_mismatch: ["mismatchTitle", "mismatchDetail"], origin_mismatch: ["mismatchTitle", "mismatchDetail"],
    fingerprint_mismatch: ["mismatchTitle", "mismatchDetail"], revision_conflict: ["revisionTitle", "revisionDetail"],
    continuation_revision_conflict: ["revisionTitle", "revisionDetail"], continuation_lease_mismatch: ["mismatchTitle", "mismatchDetail"],
    invalid_input: ["invalidInputTitle", "invalidInputDetail"], invalid_state: ["invalidStateTitle", "invalidStateDetail"],
    search_plan_payload_too_large: ["searchPlanTooLargeTitle", "searchPlanTooLargeDetail"],
    replay: ["replayTitle", "replayDetail"], capacity: ["capacityTitle", "capacityDetail"], unavailable: ["unavailableTitle", "unavailableDetail"],
  } as const;
  return { title: t(keys[code][0]), detail: t(keys[code][1]), code };
}
