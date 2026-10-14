import { useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { AppBridge, PostMessageTransport } from "@modelcontextprotocol/ext-apps/app-bridge";
import type { McpUiHostContext } from "@modelcontextprotocol/ext-apps";
import type { CallToolRequest, CallToolResult } from "@modelcontextprotocol/sdk/types.js";

import type { ResultChoiceAnswer, ResultSnapshot } from "@mcp-ui/lib/result-contract";
import { choiceSnapshot } from "@mcp-ui/test-support/fixtures";
import { creatorBatchSnapshot } from "@mcp-ui/test-support/creator-batch-fixture";
import { simulateChoiceResult } from "@mcp-ui/test-support/choice-result";

type Scenario = "creator" | "creator-omit-source" | "choice";
type Mode = "inline" | "fullscreen";
type Policy = "grant" | "reject";

const scenarioParam = new URLSearchParams(window.location.search).get("scenario");
const scenario: Scenario = scenarioParam === "choice" ? "choice" : scenarioParam === "creator-omit-source" ? "creator-omit-source" : "creator";
const creatorSnapshot = creatorBatchSnapshot(12);
const initialSnapshot = scenario === "choice" ? choiceSnapshot() : scenario === "creator-omit-source" ? omitSourceUrls(creatorSnapshot) : creatorSnapshot;

function hostContext(mode: Mode): McpUiHostContext {
  return {
    displayMode: mode,
    availableDisplayModes: ["inline", "fullscreen"],
    containerDimensions: { width: 720, height: 640 },
    locale: "en-US",
    platform: "desktop",
    deviceCapabilities: { touch: false, hover: true },
    safeAreaInsets: { top: 0, right: 0, bottom: 0, left: 0 },
    toolInfo: { tool: { name: "onecrew_result_present", inputSchema: { type: "object" } } },
  };
}

function toolResult(snapshot: ResultSnapshot): CallToolResult {
  return { content: [], _meta: { "onecrew/result": snapshot, viewUUID: snapshot.binding.view.identity } };
}

function stateResult(snapshot: ResultSnapshot): CallToolResult {
  return { content: [], structuredContent: { kind: "snapshot", snapshot } };
}

function omitSourceUrls(snapshot: ResultSnapshot): ResultSnapshot {
  const action = snapshot.pending_action;
  if (action?.kind !== "creator_save_batch") return snapshot;
  return {
    ...snapshot,
    pending_action: {
      ...action,
      review: {
        ...action.review,
        rows: action.review.rows.map(row => {
          const { source_url: _sourceUrl, ...preview } = row.preview;
          return { ...row, preview: preview as unknown as typeof row.preview };
        }),
      },
    },
  };
}

function OfficialHostHarness() {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const bridgeRef = useRef<AppBridge | null>(null);
  const snapshotRef = useRef(initialSnapshot);
  const policyRef = useRef<Policy>("grant");
  const [connection, setConnection] = useState("waiting");
  const [mode, setMode] = useState<Mode>("inline");
  const [policy, setPolicy] = useState<Policy>("grant");
  const [calls, setCalls] = useState<string[]>([]);
  const [finalSubmits, setFinalSubmits] = useState(0);
  const [widthPatch, setWidthPatch] = useState("not-sent");
  const [reportedSize, setReportedSize] = useState("none");

  const connectHost = () => {
    const iframe = iframeRef.current;
    if (!iframe?.contentWindow || bridgeRef.current) return;
    const bridge = new AppBridge(
      null,
      { name: "onecrew-official-preview-host", version: "1.0.0" },
      { serverTools: {}, logging: {}, updateModelContext: { text: {} }, message: { text: {} } },
      { hostContext: hostContext("inline") },
    );
    bridgeRef.current = bridge;
    bridge.oninitialized = () => {
      setConnection("initialized");
      void bridge.sendToolResult(toolResult(snapshotRef.current));
    };
    bridge.onsizechange = ({ width, height }) => setReportedSize(`${width ?? "?"}x${height ?? "?"}`);
    bridge.onrequestdisplaymode = async ({ mode: requested }) => {
      const accepted: Mode = requested === "fullscreen" && policyRef.current === "grant" ? "fullscreen" : "inline";
      setMode(accepted);
      bridge.setHostContext({ displayMode: accepted });
      return { mode: accepted };
    };
    bridge.oncalltool = async (params: CallToolRequest["params"]) => {
      const action = typeof params.arguments?.action === "string" ? params.arguments.action : "unknown";
      setCalls(current => [...current, action]);
      if (action === "select" && scenario === "choice") {
        const answers = params.arguments?.answers;
        if (!Array.isArray(answers)) return { content: [], isError: true };
        try {
          snapshotRef.current = simulateChoiceResult(snapshotRef.current, answers as ResultChoiceAnswer[]);
          setFinalSubmits(current => current + 1);
        } catch {
          return { content: [], isError: true };
        }
      }
      return stateResult(snapshotRef.current);
    };
    void bridge.connect(new PostMessageTransport(iframe.contentWindow, iframe.contentWindow))
      .then(() => setConnection("connected"))
      .catch(() => setConnection("error"));
  };

  const sendWidthOnlyContext = () => {
    bridgeRef.current?.setHostContext({ containerDimensions: { width: 480 } });
    setWidthPatch("sent (height remains Host-stated 640px)");
  };

  return <main style={{ padding: 16, fontFamily: "system-ui, sans-serif" }}>
    <h1>Official SDK Host · {scenario}</h1>
    <p data-host-status="true">bridge: {connection} · mode: {mode} · policy: {policy} · reported size: {reportedSize}</p>
    <p data-final-submit-count="true">final select calls: {finalSubmits}</p>
    <p data-width-only-status="true">width-only context: {widthPatch}</p>
    <div style={{ display: "flex", flexWrap: "wrap", gap: 8, marginBottom: 12 }}>
      <button type="button" data-policy="grant" onClick={() => { policyRef.current = "grant"; setPolicy("grant"); }}>grant fullscreen</button>
      <button type="button" data-policy="reject" onClick={() => { policyRef.current = "reject"; setPolicy("reject"); }}>reject fullscreen</button>
      <button type="button" data-width-only="true" onClick={sendWidthOnlyContext}>send width-only context</button>
    </div>
    <p data-tool-calls="true">tool calls: {calls.length ? calls.join(", ") : "none"}</p>
    <iframe ref={iframeRef} onLoad={connectHost} title="Real InspectorApp" src="/src/widgets/operation-inspector/index.html" style={{ width: "100%", height: 640, border: "1px solid #aaa" }} />
  </main>;
}

createRoot(document.getElementById("root")!).render(<OfficialHostHarness />);
