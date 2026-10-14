import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";

import { validateResultSnapshot } from "@mcp-ui/lib/generated/contracts/validators/ResultSnapshot.js";
import { validateDmFailure } from "@mcp-ui/lib/generated/contracts/validators/DmFailure.js";
import { validateClaimedResultContinuation } from "@mcp-ui/lib/generated/contracts/validators/ClaimedResultContinuation.js";
import { choiceAnswerShapeValid, choiceAnswersWithinBudget, searchPlanDescriptionValid, searchPlanEditorDraftValid, searchPlanKeywordsValid } from "@mcp-ui/lib/result-contract/invariants";

import type {
  ClaimedResultContinuation,
  CreatorSavePreview,
  CreatorSaveBatchOutcome,
  CallDisposition,
  InitialResultParse,
  PendingActionView,
  PresentationKind,
  ResultContinuationSnapshot,
  ResultContinuationContext,
  ResultBinding,
  ResultChoiceAnswer,
  ResultSnapshot,
  ResultSnapshotRejectReason,
  ResultState,
  ResultStateFailure,
  ResultStateOutput,
  ResultViewKind,
  ResultSchemaDiagnostic,
  ResultDmFailure,
} from "./types";

export const RESULT_META_KEY = "onecrew/result" as const;
const RESULT_REF = /^rr_[0-9a-f]{32}$/u;
const VIEW_IDENTITY = /^rv_[0-9a-f]{32}$/u;
const LEASE_ID = /^rcl_[0-9a-f]{32}$/u;
const FINGERPRINT = /^sha256:[0-9a-f]{64}$/u;
const IDENTIFIER = /^[A-Za-z0-9._:-]{1,128}$/u;

export function parseInitialResult(payload: CallToolResult): InitialResultParse {
  const meta = asRecord(payload._meta);
  if (!meta || !(RESULT_META_KEY in meta)) return { status: "missing" };
  const snapshot = parseResultSnapshot(meta[RESULT_META_KEY]);
  return snapshot && meta.viewUUID === snapshot.binding.view.identity
    ? { status: "valid", snapshot }
    : { status: "invalid" };
}

export function parseResultSnapshot(value: unknown): ResultSnapshot | null {
  const parsed = parseResultSnapshotDetailed(value);
  return "snapshot" in parsed ? parsed.snapshot : null;
}

export function diagnoseResultSnapshot(value: unknown): ResultSnapshotRejectReason | null {
  const parsed = parseResultSnapshotDetailed(value);
  return "reason" in parsed ? parsed.reason : null;
}

export function resultSnapshotDiagnostics(value: unknown): ResultSchemaDiagnostic[] {
  const normalized = normalizeSnapshotWire(value);
  if (!validateResultSnapshot(normalized)) {
    const errors = (validateResultSnapshot.errors ?? []).map(asRecord).filter((item) => item != null);
    const leaves = errors.filter((item) => item.keyword !== "oneOf" && item.keyword !== "anyOf");
    return (leaves.length ? leaves : errors).slice(0, 3).flatMap((item) => {
      // Only generated-schema locations and rule names, never instance values,
      // additionalProperty names, allowedValues, messages, or raw tool content.
      if (typeof item.schemaPath !== "string" || !/^[A-Za-z0-9_$~/#.:-]{1,512}$/.test(item.schemaPath) ||
          typeof item.keyword !== "string" || !/^[A-Za-z]{1,40}$/.test(item.keyword)) return [];
      const missing = asRecord(item.params)?.missingProperty;
      return [{ schemaPath: item.schemaPath, keyword: item.keyword,
        ...(typeof missing === "string" && /^[A-Za-z_][A-Za-z0-9_]{0,63}$/.test(missing) ? { missingProperty: missing } : {}),
      }];
    });
  }
  const reason = diagnoseResultSnapshot(normalized);
  return reason ? [{ schemaPath: `#/${reason}`, keyword: "semantic" }] : [];
}

export function parseResultStateOutput(value: unknown): ResultStateOutput | null {
  const normalized = normalizeStateOutputWire(value);
  const item = asRecord(normalized);
  if (!item || typeof item.kind !== "string") return null;
  if (item.kind === "snapshot" && exactKeys(item, "dm_failure" in item ? ["kind", "snapshot", "dm_failure"] : ["kind", "snapshot"])) {
    const snapshot = parseResultSnapshot(item.snapshot);
    const dmFailure = item.dm_failure == null ? null : parseDmFailure(item.dm_failure);
    if (!snapshot || (item.dm_failure != null && !dmFailure)) return null;
    return { kind: "snapshot", snapshot, ...("dm_failure" in item ? { dm_failure: dmFailure } : {}) };
  }
  if (item.kind === "claimed_continuation" && exactKeys(item, ["kind", "claimed"])) {
    const claimed = parseClaimedContinuation(item.claimed);
    return claimed ? { kind: "claimed_continuation", claimed } : null;
  }
  if (item.kind === "completed_continuation" && exactKeys(item, ["kind", "completed"])) {
    const completed = asRecord(item.completed);
    const binding = parseBinding(completed?.binding);
    const continuation = parseContinuation(completed?.continuation);
    return completed && exactKeys(completed, ["binding", "continuation"]) && binding && continuation
      ? { kind: "completed_continuation", completed: { binding, continuation } }
      : null;
  }
  return null;
}

function parseDmFailure(value: unknown): ResultDmFailure | null {
  return validateDmFailure(value) ? value : null;
}

export function parseResultStateFailure(value: unknown): ResultStateFailure | null {
  const item = asRecord(value);
  if (!item || typeof item.code !== "string") return null;
  if (item.code === "revision_conflict" || item.code === "continuation_revision_conflict") {
    return exactKeys(item, ["code", "current_revision"]) &&
        Number.isSafeInteger(item.current_revision) &&
        (item.current_revision as number) > 0
      ? item as ResultStateFailure
      : null;
  }
  const closedCodes: ReadonlySet<string> = new Set([
    "absent",
    "expired",
    "view_mismatch",
    "origin_mismatch",
    "fingerprint_mismatch",
    "continuation_lease_mismatch",
    "invalid_input",
    "search_plan_payload_too_large",
    "invalid_state",
    "replay",
    "capacity",
    "unavailable",
  ]);
  return closedCodes.has(item.code) && exactKeys(item, ["code"])
    ? item as ResultStateFailure
    : null;
}

function parseResultSnapshotDetailed(value: unknown):
  | { snapshot: ResultSnapshot }
  | { reason: ResultSnapshotRejectReason } {
  const normalized = normalizeSnapshotWire(value);
  if (!validateResultSnapshot(normalized)) {
    return { reason: classifySnapshotErrors(validateResultSnapshot.errors) };
  }
  if (normalized.presented !== true) return { reason: "root" };
  const snapshot = normalized as ResultSnapshot;
  const semanticReason = snapshotSemanticRejectReason(snapshot);
  return semanticReason ? { reason: semanticReason } : { snapshot };
}

function snapshotSemanticRejectReason(
  snapshot: ResultSnapshot,
): ResultSnapshotRejectReason | null {
  if (!Number.isSafeInteger(snapshot.completed_sections_count) || snapshot.completed_sections_count < 0 || snapshot.completed_sections_count > snapshot.presentation.sections.length) return "presentation";
  if (!batchSemanticsValid(snapshot)) return "batch";
  if (!presentationSemanticsValid(snapshot)) return "presentation";
  if (snapshot.pending_action && !pendingActionSemanticsValid(snapshot.pending_action)) return "pending_action";
  if (!stateSemanticsValid(snapshot.state)) return "state";
  if (!continuationConsistent(snapshot.continuation)) return "continuation";
  return consistentSnapshot(
    snapshot.binding.view.kind,
    snapshot.pending_action,
    snapshot.state,
    snapshot.presentation.kind,
  ) ? null : "consistency";
}

type PendingActionKind = PendingActionView["kind"];

// One semantic check per generated pending-action kind; the generated
// validator has already enforced the wire shape.
const PENDING_ACTION_SEMANTICS: { [K in PendingActionKind]: (action: Extract<PendingActionView, { kind: K }>) => boolean } = {
  choice: (action) => {
    const questionIds = action.questions.map((question) => question.id);
    return new Set(questionIds).size === questionIds.length && action.questions.every((question) => {
      const optionIds = question.options.map((option) => option.id);
      return identifier(question.id, 128) &&
        boundedText(question.header) &&
        boundedText(question.prompt) &&
        new Set(optionIds).size === optionIds.length &&
        question.options.every((option) =>
          identifier(option.id, 128) &&
          boundedText(option.label) &&
          optionalText(option.description)
        );
    });
  },
  confirm: (action) => boundedText(action.prompt) && boundedText(action.label),
  search_plan: (action) => FINGERPRINT.test(action.draft_digest) && searchPlanEditorDraftValid(action.draft),
  creator_save_batch: (action) => {
    const rows = action.review.rows;
    const failures = action.review.failures;
    const summary = action.review.summary;
    const rowIds = [...rows.map(row => row.row_id), ...failures.map(failure => failure.row_id)];
    const counts = rows.reduce((counts, row) => {
      const category = row.preview.status === "identity_pending" || row.preview.status === "contact_conflict" ? "blocked" : row.preview.status;
      return { ...counts, [category]: counts[category] + 1 };
    }, { eligible_new: 0, eligible_update: 0, existing_skipped: 0, duplicate_skipped: 0, blocked: 0, failed: failures.length });
    return rowIds.length >= 1 && rowIds.length <= 50 && new Set(rowIds).size === rowIds.length &&
      failures.every(failure => identifier(failure.row_id, 128) && optionalText(failure.display_name) && optionalText(failure.handle) && optionalText(failure.profile_url) && failure.reason === "preparation_failed") &&
      summary.total === rowIds.length &&
      Object.entries(counts).every(([key, count]) => summary[key as keyof typeof counts] === count) &&
      rows.every(row => identifier(row.row_id, 128) && row.preview.intent === action.review.intent && creatorPreviewSemanticsValid(row.preview));
  },
};

/** Pending-action kinds the handwritten semantic layer checks. */
export const PENDING_ACTION_KINDS = Object.keys(PENDING_ACTION_SEMANTICS) as PendingActionKind[];

function pendingActionSemanticsValid(action: PendingActionView): boolean {
  return (PENDING_ACTION_SEMANTICS[action.kind] as (value: PendingActionView) => boolean)(action);
}

function creatorPreviewSemanticsValid(preview: CreatorSavePreview): boolean {
  const routeIds = preview.routes.map(route => route.route_id);
  const validReasons: Record<CreatorSavePreview["status"], CreatorSavePreview["reason"][]> = {
    eligible_new: [null], eligible_update: [null], existing_skipped: ["existing_creator"], duplicate_skipped: ["duplicate_selection"],
    identity_pending: ["identity_ambiguous", "conflicting_update"], contact_conflict: ["required_contact_route_conflict"], failed: ["preparation_failed"],
  };
  const intentValid = !(preview.intent === "add_new" && preview.status === "eligible_update") &&
    !(preview.intent === "save_or_update" && preview.status === "existing_skipped");
  const scoreValid = preview.score === null ||
    (Number.isSafeInteger(preview.score) && preview.score >= 0 && preview.score <= 100);
  return new Set(routeIds).size === routeIds.length &&
    Number.isSafeInteger(preview.expires_at) && preview.expires_at >= 0 && optionalText(preview.avatar_url) &&
    scoreValid && intentValid &&
    validReasons[preview.status].includes(preview.reason) &&
    preview.routes.every(route => boundedText(route.route_id) && boundedText(route.display_value));
}

function creatorOutcomesSemanticsValid(rows: CreatorSaveBatchOutcome[]): boolean {
  return rows.length >= 1 && rows.length <= 50 && new Set(rows.map(row => row.row_id)).size === rows.length && rows.every(row =>
    identifier(row.row_id, 128) && optionalText(row.display_name) && optionalText(row.handle) &&
    optionalText(row.profile_url) && optionalText(row.avatar_url) &&
    (row.status === "saved" ? typeof row.creator_id === "string" && identifier(row.creator_id, 256) : row.creator_id === null) &&
    creatorOperationSemanticsValid(row) &&
    (row.failure_code === null || identifier(row.failure_code, 128)) && new Set(row.confirmed_route_ids).size === row.confirmed_route_ids.length &&
    row.confirmed_route_ids.every(id => boundedText(id)));
}

function creatorOperationSemanticsValid(row: Pick<CreatorSaveBatchOutcome, "status" | "operation" | "reason">): boolean {
  if (row.operation === "added" || row.operation === "updated") return row.status === "saved" && row.reason === null;
  if (row.operation === "not_selected") return row.status === "not_selected" && row.reason === "user_not_selected";
  if (row.operation === "unknown") return row.status === "unknown" && row.reason === "commit_unknown";
  if (row.operation === "existing_skipped") return (row.status === "not_run" || row.status === "not_selected") && row.reason === "existing_creator";
  if (row.operation === "duplicate_skipped") return (row.status === "not_run" || row.status === "not_selected") && row.reason === "duplicate_selection";
  return row.operation === "failed" && (row.status === "failed" || row.status === "not_run") && row.reason !== null &&
    ["identity_ambiguous", "required_contact_route_conflict", "conflicting_update", "preparation_failed", "row_failed"].includes(row.reason);
}

// Mirrors the runtime's MAX_OWNER_FRAME_RESOURCE_REFS. It follows the OWNER
// FRAME's reference ceiling, which is max(Creator save batch limit, 64) — not
// the batch limit itself. An owner frame also carries ordinary calls, and those
// keep the generic 64-ref budget whether or not a Creator save shares the batch,
// so the owner and generic cases are the same number. Do not "simplify" this to
// the batch limit: a legal 51-64 ref generic outcome would be rejected here
// while the runtime and the generated wire schema both accept it.
const MAX_OWNER_FRAME_RESOURCE_REFS = 64;

function batchSemanticsValid(snapshot: ResultSnapshot): boolean {
  const { batch } = snapshot;
  const callIds = batch.outcomes.map((outcome) => outcome.call_id);
  return new Set(callIds).size === callIds.length && batch.outcomes.every((outcome) =>
    identifier(outcome.call_id, 128) &&
    identifier(outcome.operation_id, 128) &&
    dispositionSemanticsValid(outcome.disposition) &&
    boundedText(outcome.summary) &&
    outcome.resource_refs.length <= MAX_OWNER_FRAME_RESOURCE_REFS &&
    outcome.resource_refs.every((reference) =>
      identifier(reference.id, 256) && optionalText(reference.label)
    )
  );
}

function dispositionSemanticsValid(disposition: CallDisposition): boolean {
  if (disposition.kind === "succeeded") return true;
  if (disposition.kind === "failed" || disposition.kind === "unknown") {
    return identifier(disposition.code, 128);
  }
  return boundedText(disposition.reason);
}

function presentationSemanticsValid(snapshot: ResultSnapshot): boolean {
  const { presentation } = snapshot;
  const batchReview = snapshot.pending_action?.kind === "creator_save_batch";
  const batchOutcome = presentation.sections.some((section) => section.kind === "creator_save_rows");
  // The save owner's 50 rows use its 4 MiB projection allowance. Generic
  // presentation and generic execution limits remain independent.
  const maxBytes = snapshot.completed_sections_count > 0 ? 32 * 1024 * 1024 : batchReview || batchOutcome ? 4 * 1024 * 1024 : 64 * 1024;
  const maxSections = snapshot.completed_sections_count > 0 ? 8 * (50 + 32) : batchReview ? 50 + 32 : 32;
  if (
    !boundedText(presentation.title) ||
    !optionalText(presentation.summary) ||
    presentation.sections.length > maxSections ||
    new TextEncoder().encode(JSON.stringify(presentation)).length > maxBytes
  ) return false;
  return presentation.sections.every((section) => {
    if (section.kind === "creator_save_rows") return creatorOutcomesSemanticsValid(section.rows);
    if (!optionalText(section.heading)) return false;
    if (section.kind === "fields") {
      return section.fields.every((field) =>
        boundedText(field.label) && boundedText(field.value)
      );
    }
    if (section.kind === "items") {
      return section.items.every((item) =>
        boundedText(item.title) && optionalText(item.description)
      );
    }
    return boundedText(section.body);
  });
}

function stateSemanticsValid(state: ResultState): boolean {
  if (state.kind === "available") return true;
  if (state.kind === "submitted") {
    return state.action.kind === "confirmed" ||
      (answersAreUnique(state.action.answers) && choiceAnswersWithinBudget(state.action.answers) &&
        state.action.answers.every(choiceAnswerShapeValid));
  }
  return identifier(state.code, 128) && boundedText(state.summary);
}

function claimedContinuationConsistent(claimed: ClaimedResultContinuation): boolean {
  if (claimed.dispatch_started_at_ms > claimed.dispatch_not_after_ms) return false;
  const context = claimed.request.context;
  if (context.kind === "choice") return answersAreUnique(context.answers);
  if (context.kind === "creator_save_batch") return continuationCreatorOutcomesValid(context.summary, context.outcomes);
  if (context.kind === "execution") {
    const callIds = context.outcomes.map((outcome) => outcome.call_id);
    return new Set(callIds).size === callIds.length;
  }
  return true;
}

type ContinuationCreatorOutcome = Extract<ResultContinuationContext, { kind: "creator_save_batch" }>["outcomes"][number];
type CreatorSaveResultSummary = Extract<ResultContinuationContext, { kind: "creator_save_batch" }>["summary"];

// Mirrors validate_continuation_creator_outcomes: a follow-up needs the saved
// Creator or the failure code; display fields stay on the card.
function continuationCreatorOutcomesValid(summary: CreatorSaveResultSummary, rows: ContinuationCreatorOutcome[]): boolean {
  if (rows.length < 1 || rows.length > 50 || summary.total !== rows.length) return false;
  const rowIds = rows.map(row => row.row_id);
  if (new Set(rowIds).size !== rowIds.length || !rowIds.every(rowId => identifier(rowId, 128))) return false;
  const counts = { added: 0, updated: 0, existing_skipped: 0, duplicate_skipped: 0, not_selected: 0, failed: 0, unknown: 0 };
  const valid = rows.every(row => {
    counts[row.operation] += 1;
    return (row.status === "saved" ? typeof row.creator_id === "string" && identifier(row.creator_id, 256) : row.creator_id === null) &&
    (row.failure_code === null || identifier(row.failure_code, 128)) &&
    creatorOperationSemanticsValid(row);
  });
  return valid && Object.entries(counts).every(([key, value]) => value === summary[key as keyof typeof counts]);
}

function parseClaimedContinuation(value: unknown): ClaimedResultContinuation | null {
  // The generated contract validates the shape; handwritten checks below add
  // the semantic invariants of validate_continuation_request.
  if (!validateClaimedResultContinuation(value)) return null;
  const item = asRecord(value);
  if (!item || !exactKeys(item, [
    "binding",
    "continuation_revision",
    "lease_id",
    "request",
    "dispatch_started_at_ms",
    "dispatch_not_after_ms",
  ])) return null;
  const binding = parseBinding(item.binding);
  const request = asRecord(item.request);
  const context = parseContinuationContext(request?.context);
  if (
    !binding ||
    !positiveInteger(item.continuation_revision) ||
    typeof item.lease_id !== "string" ||
    !LEASE_ID.test(item.lease_id) ||
    !request ||
    !exactKeys(request, "requires_model_context" in request ? ["message", "context", "requires_model_context"] : ["message", "context"]) ||
    ("requires_model_context" in request && typeof request.requires_model_context !== "boolean") ||
    !boundedText(request.message) ||
    !context ||
    !timestamp(item.dispatch_started_at_ms) ||
    !timestamp(item.dispatch_not_after_ms)
  ) return null;
  const claimed = {
    binding,
    continuation_revision: item.continuation_revision,
    lease_id: item.lease_id,
    request: { message: request.message, context, ...("requires_model_context" in request ? { requires_model_context: request.requires_model_context as boolean } : {}) },
    dispatch_started_at_ms: item.dispatch_started_at_ms,
    dispatch_not_after_ms: item.dispatch_not_after_ms,
  };
  return claimedContinuationConsistent(claimed) ? claimed : null;
}

function parseBinding(value: unknown): ResultBinding | null {
  const item = asRecord(value);
  if (!item || !exactKeys(item, ["result_ref", "view", "origin", "fingerprint", "revision"])) {
    return null;
  }
  const view = asRecord(item.view);
  if (
    typeof item.result_ref !== "string" ||
    !RESULT_REF.test(item.result_ref) ||
    !view ||
    !exactKeys(view, ["identity", "kind"]) ||
    typeof view.identity !== "string" ||
    !VIEW_IDENTITY.test(view.identity) ||
    !["read_only", "confirmation", "choice"].includes(String(view.kind)) ||
    typeof item.origin !== "string" ||
    !identifier(item.origin, 128) ||
    typeof item.fingerprint !== "string" ||
    !FINGERPRINT.test(item.fingerprint) ||
    !positiveInteger(item.revision)
  ) return null;
  return {
    result_ref: item.result_ref,
    view: {
      identity: view.identity,
      kind: view.kind as ResultBinding["view"]["kind"],
    },
    origin: item.origin,
    fingerprint: item.fingerprint,
    revision: item.revision,
  };
}

function parseContinuation(value: unknown): ResultContinuationSnapshot | null {
  const item = asRecord(value);
  if (!item || !exactKeys(item, [
    "state",
    "continuation_revision",
    "dispatch_started_at_ms",
    "dispatch_not_after_ms",
  ])) return null;
  if (
    !["not_requested", "pending", "dispatching", "delivered", "paused", "delivery_unknown"]
      .includes(String(item.state)) ||
    !positiveInteger(item.continuation_revision) ||
    !(item.dispatch_started_at_ms === null || timestamp(item.dispatch_started_at_ms)) ||
    !(item.dispatch_not_after_ms === null || timestamp(item.dispatch_not_after_ms))
  ) return null;
  const continuation = item as ResultContinuationSnapshot;
  return continuationConsistent(continuation) ? continuation : null;
}

type ContinuationContextKind = ResultContinuationContext["kind"];

// One parser per generated continuation-context kind.
const CONTINUATION_CONTEXT_PARSERS: Record<ContinuationContextKind, (item: Record<string, unknown>) => ResultContinuationContext | null> = {
  search_plan_confirmed: (item) => {
    if (!exactKeys(item, ["kind", "confirmed_plan_ref", "description", "keywords"])) return null;
    const context = item as Extract<ResultContinuationContext, { kind: "search_plan_confirmed" }>;
    return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/u.test(context.confirmed_plan_ref) &&
      searchPlanDescriptionValid(context.description) && searchPlanKeywordsValid(context.keywords) ? context : null;
  },
  cancelled: (item) => exactKeys(item, ["kind"]) ? item as ResultContinuationContext : null,
  edit_requested: (item) => exactKeys(item, ["kind"]) ? item as ResultContinuationContext : null,
  choice: (item) => {
    if (!exactKeys(item, ["kind", "answers"])) return null;
    const answers = parseAnswers(item.answers);
    return answers ? { kind: "choice", answers } : null;
  },
  creator_save_batch: (item) => {
    if (!exactKeys(item, ["kind", "summary", "outcomes"]) || !Array.isArray(item.outcomes)) return null;
    const summary = asRecord(item.summary);
    if (!summary || !exactKeys(summary, ["total", "added", "updated", "existing_skipped", "duplicate_skipped", "not_selected", "failed", "unknown"])) return null;
    const outcomes = item.outcomes as ContinuationCreatorOutcome[];
    return continuationCreatorOutcomesValid(summary as CreatorSaveResultSummary, outcomes) ? { kind: "creator_save_batch", summary: summary as CreatorSaveResultSummary, outcomes } : null;
  },
  execution: (item) => {
    if (!exactKeys(item, ["kind", "outcomes"]) || !Array.isArray(item.outcomes) ||
        item.outcomes.length < 1 || item.outcomes.length > 8) return null;
    const outcomes = item.outcomes.map(parseContinuationOutcome);
    if (!outcomes.every((outcome): outcome is NonNullable<typeof outcome> => outcome !== null)) return null;
    const callIds = outcomes.map((outcome) => outcome.call_id);
    return new Set(callIds).size === callIds.length ? { kind: "execution", outcomes } : null;
  },
};

/** Continuation-context kinds the handwritten parser accepts. */
export const CONTINUATION_CONTEXT_KINDS = Object.keys(CONTINUATION_CONTEXT_PARSERS) as ContinuationContextKind[];

function parseContinuationContext(value: unknown): ResultContinuationContext | null {
  const item = asRecord(value);
  if (!item || typeof item.kind !== "string" || !Object.hasOwn(CONTINUATION_CONTEXT_PARSERS, item.kind)) return null;
  return CONTINUATION_CONTEXT_PARSERS[item.kind as ContinuationContextKind](item);
}

function parseContinuationOutcome(value: unknown):
  | Extract<ResultContinuationContext, { kind: "execution" }>["outcomes"][number]
  | null {
  const item = asRecord(value);
  if (!item || !exactKeys(item, ["call_id", "operation_id", "disposition", "summary", "resource_refs"])) {
    return null;
  }
  const disposition = parseDisposition(item.disposition);
  return IDENTIFIER.test(String(item.call_id)) &&
      IDENTIFIER.test(String(item.operation_id)) &&
      disposition &&
      boundedText(item.summary) && Array.isArray(item.resource_refs) &&
      item.resource_refs.length <= 64 &&
      item.resource_refs.every((value) => { const ref = asRecord(value); return ref && identifier(ref.id, 256) && optionalText(ref.label); })
    ? {
        call_id: item.call_id as string,
        operation_id: item.operation_id as string,
        disposition,
        summary: item.summary,
        resource_refs: item.resource_refs as Extract<ResultContinuationContext, { kind: "execution" }>["outcomes"][number]["resource_refs"],
      }
    : null;
}

function parseDisposition(value: unknown): CallDisposition | null {
  const item = asRecord(value);
  if (!item || typeof item.kind !== "string") return null;
  if (item.kind === "succeeded") return exactKeys(item, ["kind"]) ? { kind: "succeeded" } : null;
  if ((item.kind === "failed" || item.kind === "unknown") &&
      exactKeys(item, ["kind", "code"]) && IDENTIFIER.test(String(item.code))) {
    return { kind: item.kind, code: item.code as string };
  }
  if (item.kind === "not_run" && exactKeys(item, ["kind", "reason"]) && boundedText(item.reason)) {
    return { kind: "not_run", reason: item.reason };
  }
  return null;
}

function parseAnswers(value: unknown): ResultChoiceAnswer[] | null {
  if (!Array.isArray(value) || value.length < 1 || value.length > 32) return null;
  const answers = value.map((answer): ResultChoiceAnswer | null => {
    const item = asRecord(answer);
    if (!item || typeof item.question_id !== "string") return null;
    let parsed: ResultChoiceAnswer;
    if (exactKeys(item, ["question_id", "option_id"]) && typeof item.option_id === "string") {
      parsed = { question_id: item.question_id, option_id: item.option_id };
    } else if (exactKeys(item, ["question_id", "option_ids"]) && Array.isArray(item.option_ids) && item.option_ids.every(id => typeof id === "string")) {
      parsed = { question_id: item.question_id, option_ids: item.option_ids as string[] };
    } else if (exactKeys(item, ["question_id", "text"]) && typeof item.text === "string") {
      parsed = { question_id: item.question_id, text: item.text };
    } else return null;
    return choiceAnswerShapeValid(parsed) ? parsed : null;
  });
  if (!answers.every((answer): answer is NonNullable<typeof answer> => answer !== null)) return null;
  return answersAreUnique(answers) && choiceAnswersWithinBudget(answers) ? answers : null;
}

function positiveInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) > 0;
}

function timestamp(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) >= 0;
}

function boundedText(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0 &&
    new TextEncoder().encode(value).length <= 8_192;
}

function optionalText(value: unknown): boolean {
  return value === null || boundedText(value);
}

function identifier(value: unknown, maxBytes: number): value is string {
  return typeof value === "string" && value.length >= 1 && value.length <= maxBytes &&
    /^[A-Za-z0-9._:-]+$/u.test(value);
}

function answersAreUnique(answers: Array<{ question_id: string }>): boolean {
  const questions = answers.map((answer) => answer.question_id);
  return new Set(questions).size === questions.length;
}

function continuationConsistent(continuation: ResultContinuationSnapshot): boolean {
  const hasStarted = continuation.dispatch_started_at_ms !== null;
  const hasDeadline = continuation.dispatch_not_after_ms !== null;
  if (hasStarted !== hasDeadline) return false;
  if (
    hasStarted &&
    continuation.dispatch_started_at_ms! > continuation.dispatch_not_after_ms!
  ) return false;
  if (continuation.state === "dispatching") return hasStarted;
  if (continuation.state === "pending" || continuation.state === "not_requested") {
    return !hasStarted;
  }
  return true;
}

function consistentSnapshot(
  view: ResultViewKind,
  pending: PendingActionView | null,
  state: ResultState,
  kind: PresentationKind,
): boolean {
  if (kind === "read_only" && view !== "read_only") return false;
  if (view === "choice" && kind !== "choice") return false;
  if (kind === "choice" && view !== "choice") return false;
  if (kind === "search_plan_editor" && view !== "confirmation") return false;
  if (kind === "creator_save_batch_confirmation" && state.kind === "available" && view !== "confirmation") return false;
  if (kind === "outreach_confirmation" && state.kind === "available" && view !== "confirmation") return false;
  if (kind === "email_confirmation" && state.kind === "available" && view !== "confirmation") return false;
  if (kind === "creator_batch_confirmation" && state.kind === "available" && view !== "confirmation") return false;
  if ((kind === "creator_save_batch_outcome" || kind === "outreach_outcome" || kind === "email_outcome" || kind === "creator_batch_outcome") && pending !== null) return false;
  if (pending?.kind === "creator_save_batch" && kind !== "creator_save_batch_confirmation") return false;
  if (pending?.kind === "search_plan" && kind !== "search_plan_editor") return false;
  if (state.kind === "available") {
    if (view === "read_only") return pending === null;
    if (view === "confirmation") return kind === "creator_save_batch_confirmation" ? pending?.kind === "creator_save_batch" : kind === "search_plan_editor" ? pending?.kind === "search_plan" : pending?.kind === "confirm";
    return pending?.kind === "choice";
  }
  return pending === null;
}

function normalizeStateOutputWire(value: unknown): unknown {
  const item = asRecord(value);
  if (!item || typeof item.kind !== "string") return value;
  if (item.kind === "snapshot") {
    return { ...item, snapshot: normalizeSnapshotWire(item.snapshot) };
  }
  if (item.kind === "claimed_continuation") {
    const claimed = asRecord(item.claimed);
    const request = asRecord(claimed?.request);
    const context = asRecord(request?.context);
    if (claimed && request && context?.kind === "creator_save_batch" && Array.isArray(context.outcomes)) return {
      ...item, claimed: { ...claimed, request: { ...request, context: { ...context, outcomes: context.outcomes.map((value) => normalizeNullableFields(value, ["creator_id", "failure_code", "reason"])) } } },
    };
    if (claimed && request && context?.kind === "execution" && Array.isArray(context.outcomes)) return {
      ...item, claimed: { ...claimed, request: { ...request, context: { ...context, outcomes: context.outcomes.map((value) => {
        const row = asRecord(value);
        if (!row) return value;
        const refs = Object.hasOwn(row, "resource_refs") ? row.resource_refs : [];
        return { ...row, resource_refs: Array.isArray(refs) ? refs.map((value) => normalizeNullableFields(value, ["label"])) : refs };
      }) } } },
    };
  }
  if (item.kind === "completed_continuation") {
    const completed = asRecord(item.completed);
    if (!completed) return value;
    return {
      ...item,
      completed: {
        ...completed,
        continuation: normalizeContinuationWire(completed.continuation),
      },
    };
  }
  return value;
}

function normalizeSnapshotWire(value: unknown): unknown {
  const item = asRecord(value);
  if (!item) return value;
  return {
    ...item,
    batch: normalizeBatchWire(item.batch),
    presentation: normalizePresentationWire(item.presentation),
    pending_action: normalizePendingActionWire(item.pending_action ?? null),
    continuation: normalizeContinuationWire(item.continuation),
  };
}

function normalizeBatchWire(value: unknown): unknown {
  const item = asRecord(value);
  if (!item || !Array.isArray(item.outcomes)) return value;
  return {
    ...item,
    outcomes: item.outcomes.map((outcome) => {
      const row = asRecord(outcome);
      if (!row || !Array.isArray(row.resource_refs)) return outcome;
      return {
        ...row,
        resource_refs: row.resource_refs.map((reference) => {
          const resource = asRecord(reference);
          return resource ? { ...resource, label: resource.label ?? null } : reference;
        }),
      };
    }),
  };
}

function normalizePresentationWire(value: unknown): unknown {
  const item = asRecord(value);
  if (!item || !Array.isArray(item.sections)) return value;
  return {
    ...item,
    summary: item.summary ?? null,
    sections: item.sections.map((section) => {
      const row = asRecord(section);
      if (!row) return section;
      if (row.kind === "creator_save_rows") return { ...row, rows: Array.isArray(row.rows) ? row.rows.map(normalizeCreatorOutcomeWire) : row.rows };
      const normalized: Record<string, unknown> = { ...row, heading: row.heading ?? null };
      if (Array.isArray(row.items)) {
        normalized.items = row.items.map((entry) => {
          const item = asRecord(entry);
          return item ? { ...item, description: item.description ?? null } : entry;
        });
      }
      return normalized;
    }),
  };
}

function normalizeCreatorOutcomeWire(value: unknown): unknown {
  return normalizeNullableFields(value, ["display_name", "handle", "profile_url", "avatar_url", "creator_id", "failure_code", "reason"]);
}

function normalizePendingActionWire(value: unknown): unknown {
  const item = asRecord(value);
  if (item?.kind === "creator_save_batch") {
    const review = asRecord(item.review);
    return review && Array.isArray(review.rows) ? { ...item, review: { ...review, rows: review.rows.map(value => {
      const row = asRecord(value);
      return row ? { ...row, preview: normalizeNullableFields(row.preview, ["display_name", "handle", "profile_url", "source_url", "avatar_url", "notes", "score", "assessment", "reason"]) } : value;
    }) } } : value;
  }
  if (!item || item.kind !== "choice" || !Array.isArray(item.questions)) return value;
  return {
    ...item,
    questions: item.questions.map((question) => {
      const row = asRecord(question);
      if (!row || !Array.isArray(row.options)) return question;
      return {
        ...row,
        selection_mode: row.selection_mode === undefined ? "single" : row.selection_mode,
        options: row.options.map((option) => {
          const item = asRecord(option);
          return item ? { ...item, description: item.description ?? null } : option;
        }),
      };
    }),
  };
}

function normalizeNullableFields(value: unknown, keys: string[]): unknown {
  const item = asRecord(value);
  if (!item) return value;
  const normalized = { ...item };
  for (const key of keys) if (!Object.hasOwn(item, key)) normalized[key] = null;
  return normalized;
}

function normalizeContinuationWire(value: unknown): unknown {
  const item = asRecord(value);
  return item
    ? {
        ...item,
        dispatch_started_at_ms: item.dispatch_started_at_ms ?? null,
        dispatch_not_after_ms: item.dispatch_not_after_ms ?? null,
      }
    : value;
}

function classifySnapshotErrors(errors: readonly unknown[] | null | undefined): ResultSnapshotRejectReason {
  const first = asRecord(errors?.[0]);
  const params = asRecord(first?.params);
  const path = typeof first?.instancePath === "string" ? first.instancePath : "";
  const missing = typeof params?.missingProperty === "string" ? params.missingProperty : "";
  const segment = path.split("/").filter(Boolean)[0] ?? missing;
  if (
    segment === "binding" ||
    segment === "batch" ||
    segment === "presentation" ||
    segment === "pending_action" ||
    segment === "state" ||
    segment === "continuation"
  ) return segment;
  return "root";
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function exactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return actual.length === expected.length &&
    actual.every((key, index) => key === expected[index]);
}
