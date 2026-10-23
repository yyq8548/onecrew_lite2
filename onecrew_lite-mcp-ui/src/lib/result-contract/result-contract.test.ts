import { describe, expect, it } from "vitest";

import {
  choiceAnswersValid,
  diagnoseResultSnapshot,
  normalizeEditInstruction,
  parseInitialResult,
  parseResultSnapshot,
  parseResultStateFailure,
  parseResultStateOutput,
} from "@mcp-ui/lib/result-contract";
import { BINDING, QUESTIONS, SNAPSHOT, choiceSnapshot, terminalSnapshot } from "@mcp-ui/test-support/fixtures";
import { creatorBatchSnapshot } from "@mcp-ui/test-support/creator-batch-fixture";
import { DM_FAILURE, dmFailedSnapshot } from "@mcp-ui/test-support/dm-failure-fixture";

describe("MCP App V2 ResultSnapshot contract", () => {
  it("accepts only the exact initial metadata snapshot and matching view UUID", () => {
    const payload = { content: [], _meta: { "onecrew/result": SNAPSHOT, viewUUID: BINDING.view.identity } };
    expect(parseInitialResult(payload)).toEqual({ status: "valid", snapshot: SNAPSHOT });
    expect(parseInitialResult({ content: [], _meta: { "onecrew/result": SNAPSHOT, viewUUID: `rv_${"0".repeat(32)}` } })).toEqual({ status: "invalid" });
    expect(parseInitialResult({ content: [], _meta: { viewUUID: BINDING.view.identity } })).toEqual({ status: "missing" });
    expect(parseInitialResult({ content: [], structuredContent: { ...SNAPSHOT } })).toEqual({ status: "missing" });
  });

  it("rejects extra fields, V1 plans, open view kinds, and semantically inconsistent actions", () => {
    expect(parseResultSnapshot({ ...SNAPSHOT, unexpected: true })).toBeNull();
    expect(parseResultSnapshot({ ...SNAPSHOT, presentation: { ...SNAPSHOT.presentation, kind: "creator_discovery" } })).toBeNull();
    expect(parseResultSnapshot({ ...SNAPSHOT, presentation: { ...SNAPSHOT.presentation, kind: "toolkit_guide" } })).toBeNull();
    expect(parseResultSnapshot({ ...SNAPSHOT, binding: { ...BINDING, view: { ...BINDING.view, kind: "read_only" } } })).toBeNull();
    expect(parseResultSnapshot({ ...SNAPSHOT, state: { kind: "resolved", code: "done", summary: "Done" } })).toBeNull();
    expect(parseResultSnapshot({ ...SNAPSHOT, pending_action: null, state: { kind: "submitted", action: { kind: "confirmed" } } })?.state).toEqual({ kind: "submitted", action: { kind: "confirmed" } });
    expect(parseResultSnapshot({ ...SNAPSHOT, pending_action: null, state: { kind: "submitted", kind2: "confirmed" } })).toBeNull();
    expect(parseResultSnapshot({ kind: "plan", plan: {}, app: {} })).toBeNull();
  });

  it("locates the first rejected snapshot field group without exposing values", () => {
    expect(diagnoseResultSnapshot(SNAPSHOT)).toBeNull();
    expect(diagnoseResultSnapshot({ ...SNAPSHOT, unexpected: true })).toBe("root");
    expect(diagnoseResultSnapshot({ ...SNAPSHOT, binding: { ...BINDING, result_ref: "invalid" } })).toBe("binding");
    expect(diagnoseResultSnapshot({ ...SNAPSHOT, batch: { outcomes: [] } })).toBe("batch");
    expect(diagnoseResultSnapshot({ ...SNAPSHOT, presentation: { ...SNAPSHOT.presentation, title: "" } })).toBe("presentation");
    expect(diagnoseResultSnapshot({ ...SNAPSHOT, pending_action: { kind: "confirm", prompt: "", label: "确认" } })).toBe("pending_action");
    expect(diagnoseResultSnapshot({ ...SNAPSHOT, state: { kind: "pending" } })).toBe("state");
    expect(diagnoseResultSnapshot({ ...SNAPSHOT, continuation: { state: "not_requested" } })).toBe("continuation");
    expect(diagnoseResultSnapshot({
      ...SNAPSHOT,
      binding: { ...BINDING, view: { ...BINDING.view, kind: "read_only" } },
    })).toBe("consistency");
  });

  it("normalizes omitted nullable wire fields while keeping every object closed", () => {
    const wire = structuredClone(choiceSnapshot()) as unknown as Record<string, any>;
    delete wire.presentation.summary;
    wire.presentation.sections = [
      { kind: "items", items: [{ title: "候选人" }] },
      { kind: "notice", body: "请选择后继续。", tone: "neutral" },
    ];
    delete wire.pending_action.questions[0].options[0].description;
    wire.batch.outcomes[0].resource_refs = [{ kind: "creator", id: "creator-1" }];
    delete wire.continuation.dispatch_started_at_ms;
    delete wire.continuation.dispatch_not_after_ms;

    const parsed = parseResultSnapshot(wire);
    expect(parsed?.presentation.summary).toBeNull();
    expect(parsed?.presentation.sections).toEqual([
      { kind: "items", heading: null, items: [{ title: "候选人", description: null }] },
      { kind: "notice", heading: null, body: "请选择后继续。", tone: "neutral" },
    ]);
    expect((parsed?.pending_action as any).questions[0].options[0].description).toBeNull();
    expect(parsed?.batch.outcomes[0].resource_refs[0].label).toBeNull();
    expect(parsed?.continuation).toEqual({
      state: "not_requested",
      continuation_revision: 1,
      dispatch_started_at_ms: null,
      dispatch_not_after_ms: null,
    });

    const terminalWire = structuredClone(terminalSnapshot()) as unknown as Record<string, any>;
    delete terminalWire.pending_action;
    const parsedTerminal = parseResultSnapshot(terminalWire);
    expect(parsedTerminal?.pending_action).toBeNull();
    expect(parsedTerminal?.state).toEqual({ kind: "resolved", code: "completed", summary: "创作者已创建。" });

    wire.continuation.unexpected = true;
    expect(parseResultSnapshot(wire)).toBeNull();
  });

  it("normalizes an omitted nullable Creator source URL without widening the row shape", () => {
    const wire = creatorBatchSnapshot(1) as unknown as Record<string, any>;
    const review = wire.pending_action.review;
    delete review.rows[0].preview.source_url;
    const parsed = parseResultSnapshot(wire);
    expect(parsed?.pending_action?.kind).toBe("creator_save_batch");
    if (parsed?.pending_action?.kind !== "creator_save_batch") throw new Error("expected creator save review");
    expect(parsed.pending_action.review.rows[0]!.preview.source_url).toBeNull();
    expect(parseResultSnapshot({ ...wire, pending_action: { ...wire.pending_action, unexpected: true } })).toBeNull();
  });

  it("preserves owner identifier and UTF-8 text semantics after generated shape validation", () => {
    const unicodeIdentifier = structuredClone(choiceSnapshot());
    if (unicodeIdentifier.pending_action?.kind !== "choice") throw new Error("expected choice");
    unicodeIdentifier.pending_action.questions[0].id = "路线";
    expect(parseResultSnapshot(unicodeIdentifier)).toBeNull();

    expect(parseResultSnapshot({
      ...SNAPSHOT,
      presentation: { ...SNAPSHOT.presentation, title: "   " },
    })).toBeNull();
    expect(parseResultSnapshot({
      ...SNAPSHOT,
      presentation: { ...SNAPSHOT.presentation, title: "界".repeat(2_731) },
    })).toBeNull();
  });

  it("parses only closed state outputs and closed failure codes", () => {
    expect(parseResultStateOutput({ kind: "snapshot", snapshot: SNAPSHOT })).toEqual({ kind: "snapshot", snapshot: SNAPSHOT });
    expect(parseResultStateOutput({ kind: "snapshot", snapshot: SNAPSHOT, extra: true })).toBeNull();
    expect(parseResultStateFailure({ code: "expired" })).toEqual({ code: "expired" });
    expect(parseResultStateFailure({ code: "revision_conflict", current_revision: 4 })).toEqual({ code: "revision_conflict", current_revision: 4 });
    expect(parseResultStateFailure({ code: "revision_conflict" })).toBeNull();
    expect(parseResultStateFailure({ code: "provider_error" })).toBeNull();
  });

  it("preserves native DM evidence beside the complete terminal snapshot", () => {
    const response = { kind: "snapshot", snapshot: dmFailedSnapshot(), dm_failure: DM_FAILURE };
    expect(parseResultStateOutput(response)).toEqual(response);
    expect(parseResultStateOutput({ ...response, dm_failure: null })).toEqual({ ...response, dm_failure: null });
    const nullCodes = { ...response, dm_failure: { ...DM_FAILURE, diagnostic: { ...DM_FAILURE.diagnostic, code: null, http_status: null, rpc_code: null } } };
    expect(parseResultStateOutput(nullCodes)).toEqual(nullCodes);
    for (const field of ["code", "http_status", "rpc_code"]) {
      const diagnostic = Object.fromEntries(Object.entries(DM_FAILURE.diagnostic).filter(([key]) => key !== field));
      expect(parseResultStateOutput({ ...response, dm_failure: { ...DM_FAILURE, diagnostic } })).toBeNull();
    }
    for (const dm_failure of [
      { ...DM_FAILURE, retry_safe: true },
      { ...DM_FAILURE, diagnostic: { ...DM_FAILURE.diagnostic, raw_body: "excluded" } },
      { ...DM_FAILURE, diagnostic: { ...DM_FAILURE.diagnostic, origin: "guessed" } },
      { ...DM_FAILURE, diagnostic: { ...DM_FAILURE.diagnostic, protocol: null } },
      { ...DM_FAILURE, diagnostic: { ...DM_FAILURE.diagnostic, rpc_code: "420" } },
    ]) expect(parseResultStateOutput({ ...response, dm_failure })).toBeNull();
    expect(parseResultStateOutput({ ...response, snapshot: { ...response.snapshot, pending_action: SNAPSHOT.pending_action } })).toBeNull();
    expect(parseResultStateOutput({ ...response, snapshot: { ...response.snapshot, completed_sections_count: 2 } })).toBeNull();
    expect(parseResultStateOutput({ ...response, snapshot: { ...response.snapshot, binding: { ...BINDING, revision: 0 } } })).toBeNull();
  });

  it("validates ordered, exact multi-question answers", () => {
    const answers = [{ question_id: "scope", option_id: "email" }, { question_id: "notify", option_id: "yes" }];
    expect(choiceAnswersValid(QUESTIONS, answers)).toBe(true);
    expect(choiceAnswersValid(QUESTIONS, [...answers].reverse())).toBe(false);
    expect(choiceAnswersValid(QUESTIONS, answers.slice(0, 1))).toBe(false);
    expect(choiceAnswersValid(QUESTIONS, [{ ...answers[0], option_id: "forged" }, answers[1]])).toBe(false);
  });

  it("defaults only missing selection_mode and validates exclusive multi answer shapes", () => {
    const wire = structuredClone(choiceSnapshot()) as unknown as Record<string, any>;
    delete wire.pending_action.questions[0].selection_mode;
    expect((parseResultSnapshot(wire)?.pending_action as any).questions[0].selection_mode).toBe("single");
    for (const mode of [null, "unknown", false]) { wire.pending_action.questions[0].selection_mode = mode; expect(parseResultSnapshot(wire)).toBeNull(); }
    const question = { ...QUESTIONS[0]!, selection_mode: "multiple" as const };
    expect(choiceAnswersValid([question], [{ question_id: "scope", option_ids: ["email", "all"] }])).toBe(true);
    for (const answer of [{ option_ids: [] }, { option_ids: ["email", "email"] }, { option_ids: ["unknown"] }, { option_id: "email" }, { option_ids: ["email"], text: "other" }]) expect(choiceAnswersValid([question], [{ question_id: "scope", ...answer }])).toBe(false);
    expect(choiceAnswersValid([QUESTIONS[0]!], [{ question_id: "scope", option_ids: ["email"] }])).toBe(false);
  });

  it("normalizes bounded edit instructions and rejects controls, blank text, and oversize UTF-8", () => {
    expect(normalizeEditInstruction("  把地区改成西班牙  ")).toBe("把地区改成西班牙");
    expect(normalizeEditInstruction("  ")).toBeNull();
    expect(normalizeEditInstruction("bad\u0007value")).toBeNull();
    expect(normalizeEditInstruction("界".repeat(683))).toBeNull();
  });

  it("rejects creator preview scores outside the retained 0–100 semantic range", () => {
    for (const score of [0, null, 87]) {
      const snapshot = creatorBatchSnapshot(1);
      if (snapshot.pending_action?.kind !== "creator_save_batch") throw new Error("expected creator save review");
      snapshot.pending_action.review.rows[0]!.preview.score = score;
      expect(parseResultSnapshot(snapshot), `score ${score}`).not.toBeNull();
    }
    for (const score of [101, 255, -1, 87.5]) {
      const snapshot = creatorBatchSnapshot(1);
      if (snapshot.pending_action?.kind !== "creator_save_batch") throw new Error("expected creator save review");
      snapshot.pending_action.review.rows[0]!.preview.score = score;
      expect(parseResultSnapshot(snapshot), `score ${score}`).toBeNull();
    }
  });
});
