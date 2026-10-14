import type {
  ExactResultStateInput,
  ResultBinding as GeneratedResultBinding,
  ResultSnapshot as GeneratedResultSnapshot,
  ResultStateFailure as GeneratedResultStateFailure,
  ResultStateOutput as GeneratedResultStateOutput,
} from "@mcp-ui/lib/generated/contracts";

export type ResultBinding = GeneratedResultBinding;
export type ResultSnapshot = GeneratedResultSnapshot & { presented: true };
export type ResultSnapshotOutput = Omit<Extract<GeneratedResultStateOutput, { kind: "snapshot" }>, "snapshot"> & {
  snapshot: ResultSnapshot;
};
export type ResultDmFailure = NonNullable<ResultSnapshotOutput["dm_failure"]>;
export type ResultStateOutput =
  | ResultSnapshotOutput
  | Exclude<GeneratedResultStateOutput, { kind: "snapshot" }>;
export type ResultStateFailure = GeneratedResultStateFailure;
export type ResultStateInput = ExactResultStateInput;
export type ResultStateFailureCode = ResultStateFailure["code"];
export type PresentationProjection = ResultSnapshot["presentation"];
export type PresentationKind = PresentationProjection["kind"];
export type PresentationSection = PresentationProjection["sections"][number];
export type PendingActionView = NonNullable<ResultSnapshot["pending_action"]>;
export type CreatorSaveBatchReview = Extract<PendingActionView, { kind: "creator_save_batch" }>["review"];
export type CreatorSavePreview = CreatorSaveBatchReview["rows"][number]["preview"];
export type CreatorSaveBatchSelection = Extract<ResultStateInput, { action: "confirm_creator_save_batch" }>["selections"][number];
export type CreatorSaveBatchOutcome = Extract<PresentationSection, { kind: "creator_save_rows" }>["rows"][number];
export type CreatorSaveRoute = CreatorSavePreview["routes"][number];
export type SearchPlanEditorDraft = Extract<PendingActionView, { kind: "search_plan" }>["draft"];
export type SearchPlanPatch = Extract<ResultStateInput, { action: "revise_search_plan" }>["patch"];
export type ResultChoiceQuestion = Extract<
  PendingActionView,
  { kind: "choice" }
>["questions"][number];
export type ResultChoiceAnswer = Extract<
  ResultStateInput,
  { action: "select" }
>["answers"][number];
export type ResultViewKind = ResultBinding["view"]["kind"];
export type ExecutedBatch = ResultSnapshot["batch"];
export type CallDisposition = ExecutedBatch["outcomes"][number]["disposition"];
export type ResultState = ResultSnapshot["state"];
export type ResultContinuationSnapshot = ResultSnapshot["continuation"];
export type ClaimedResultContinuation = Extract<
  ResultStateOutput,
  { kind: "claimed_continuation" }
>["claimed"];
export type ResultContinuationContext = ClaimedResultContinuation["request"]["context"];

export type InitialResultParse =
  | { status: "valid"; snapshot: ResultSnapshot }
  | { status: "missing" }
  | { status: "invalid" };

export type ResultSnapshotRejectReason =
  | "root"
  | "binding"
  | "batch"
  | "presentation"
  | "pending_action"
  | "state"
  | "continuation"
  | "consistency";

export type ResultSchemaDiagnostic = { schemaPath: string; keyword: string; missingProperty?: string };
