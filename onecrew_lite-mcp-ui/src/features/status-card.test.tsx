import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { SNAPSHOT, choiceSnapshot } from "@mcp-ui/test-support/fixtures";
import { StatusCardView, statusKey } from "@mcp-ui/features/status-card";

describe("status-only MCP entry", () => {
  it("routes confirmations and choices to the workbench without rendering inputs or actions", () => {
    for (const snapshot of [SNAPSHOT, choiceSnapshot()]) {
      expect(statusKey(snapshot)).toBe("stageWorkbench");
      const html = renderToStaticMarkup(<StatusCardView snapshot={snapshot} stage={statusKey(snapshot)} />);
      expect(html).toContain("工作台");
      expect(html).not.toMatch(/<(button|input|select|textarea|form)\b/);
      expect(html).not.toContain("Nora Beauty");
    }
  });
  it("reports failed and unknown outcomes instead of claiming completion", () => {
    const base = { ...SNAPSHOT, pending_action: null };
    expect(statusKey({ ...base, state: { kind: "failed", code: "provider_failure", summary: "failed", retryable: false } })).toBe("stageFailed");
    expect(statusKey({ ...base, state: { kind: "unknown", code: "unknown", summary: "unknown" } })).toBe("stageUnknown");
    expect(statusKey({ ...base, batch: { outcomes: [{ ...base.batch.outcomes[0], disposition: { kind: "failed", code: "offline" } }] } })).toBe("stageFailed");
    expect(statusKey(base)).toBe("stageUnknown");
  });
  it("active entry mounts only the read-only controller", () => {
    const entry = readFileSync(new URL("../app/inspector-app.tsx", import.meta.url), "utf8");
    const controller = readFileSync(new URL("./status-card.tsx", import.meta.url), "utf8");
    expect(entry).toContain("<StatusCard ");
    expect(entry).not.toContain("<ResultCard ");
    expect(controller).toContain("readCurrentResult(appClient, binding)");
    expect(controller).not.toMatch(/confirmResult|selectResult|cancelResult|claimContinuation|dispatchContinuation|callServerTool/);
  });
  it("uses a searching stage only for an owner-submitted discovery round", () => {
    const submitted = { ...SNAPSHOT, state: { kind: "submitted" as const, action: { kind: "confirmed" as const } } };
    expect(statusKey(submitted)).toBe("stageProcessing");
    expect(statusKey({ ...submitted, batch: { outcomes: [{ ...SNAPSHOT.batch.outcomes[0], operation_id: "onecrew.discovery.advance" }] } })).toBe("stageSearching");
    expect(statusKey({ ...SNAPSHOT, batch: { outcomes: [{ ...SNAPSHOT.batch.outcomes[0], operation_id: "onecrew.discovery.advance" }] } })).toBe("stageWorkbench");
  });
});
