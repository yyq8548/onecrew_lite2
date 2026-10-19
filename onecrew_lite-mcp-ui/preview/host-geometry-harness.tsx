import { useMemo, useState } from "react";
import { createRoot } from "react-dom/client";

import "../src/styles/theme.css";
import { HostFrameProvider, type HostFrame } from "../src/app/host-frame";
import { CreatorReviewList } from "../src/features/result-card/components/creator-review-list";
import { ResultTextDetailsProvider } from "../src/features/result-card/components/text-details";
import { AppLocaleProvider, type AppLocale } from "../src/lib/i18n";
import { CREATOR_SAVE_PREVIEW } from "../src/test-support/creator-save-fixture";

type DisplayMode = NonNullable<HostFrame["displayMode"]>;

function HostGeometryHarness() {
  const [mode, setMode] = useState<DisplayMode>("fullscreen");
  const [height, setHeight] = useState(900);
  const [hostHeight, setHostHeight] = useState(900);
  const [requestPolicy, setRequestPolicy] = useState<"grant" | "reject" | "delay">("grant");
  const [locale, setLocale] = useState<AppLocale>("en-US");
  const rows = useMemo(() => Array.from({ length: 39 }, (_, index) => ({
    row_id: `row-${index}`,
    preview: {
      ...structuredClone(CREATOR_SAVE_PREVIEW),
      display_name: `Creator ${index}`,
      status: index === 38 ? "existing_skipped" as const : "eligible_new" as const,
      reason: index === 38 ? "existing_creator" as const : null,
      assessment: index === 1 ? "A deliberately taller review warning with enough text to wrap across the narrow row." : CREATOR_SAVE_PREVIEW.assessment,
    },
  })), []);
  const [selectedIds, setSelectedIds] = useState(() => rows.filter(row => row.preview.status === "eligible_new").map(row => row.row_id));
  const frame: HostFrame = {
    displayMode: mode,
    availableDisplayModes: ["inline", "fullscreen"],
    platform: "desktop",
    touch: false,
    safeArea: { top: 16, right: 0, bottom: 20, left: 0 },
    // Keep this deliberately stale while fullscreen: the list must use its
    // actual shell box rather than inheriting an inline budget.
    availableHeight: mode === "fullscreen" ? hostHeight : height,
  };
  const requestDisplayMode = (requested: DisplayMode) => {
    if (requested === "fullscreen" && requestPolicy === "reject") {
      setMode("inline");
      setHostHeight(height);
      return;
    }
    setMode(requested);
    if (requested === "fullscreen" && requestPolicy === "delay") {
      setHostHeight(280);
      window.setTimeout(() => setHostHeight(height), 100);
    } else {
      setHostHeight(height);
    }
  };
  return <AppLocaleProvider locale={locale}>
    <main style={{ padding: 16 }}>
      <style>{`[data-creator-row-probe] { position: absolute !important; visibility: hidden !important; pointer-events: none !important; } [data-creator-row] > div, [data-creator-row-probe] > li > div, [data-creator-list-scroll] > div > div:first-child { display: grid; grid-template-columns: 20px minmax(0, 1fr) 88px 76px 76px 16px; gap: 12px; }`}</style>
      <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginBottom: 12 }}>
        <button type="button" onClick={() => setMode("fullscreen")}>fullscreen</button>
        <button type="button" onClick={() => setMode("inline")}>inline</button>
        <button type="button" onClick={() => setRequestPolicy("reject")}>reject-request</button>
        <button type="button" onClick={() => setRequestPolicy("delay")}>delay-dimensions</button>
        <button type="button" onClick={() => setRequestPolicy("grant")}>grant-request</button>
        <button type="button" onClick={() => setHeight(current => current)}>same-size</button>
        <button type="button" onClick={() => setHeight(460)}>short</button>
        <button type="button" onClick={() => setHeight(900)}>tall</button>
        <button type="button" onClick={() => setLocale(current => current === "en-US" ? "zh-CN" : "en-US")}>locale</button>
      </div>
      <p data-selected-count="true">selected: {selectedIds.length}</p>
      <HostFrameProvider frame={frame} onRequestDisplayMode={requestDisplayMode}>
        <ResultTextDetailsProvider revision={1}>
          <div className="result-shell" data-display-mode={mode} style={{ display: "flex", flexDirection: "column", height: `${height}px` }}>
          <header data-slot="card-header" style={{ height: 96, flex: "0 0 auto" }}>Review</header>
          <div data-result-scroll="true" className="result-shell-content" style={{ flex: mode === "fullscreen" ? "1 1 auto" : undefined }}>
            <CreatorReviewList
              rows={rows}
              selectedIds={selectedIds}
              optionalIds={{}}
              onSelect={(rowId, checked) => setSelectedIds(current => checked ? [...new Set([...current, rowId])] : current.filter(id => id !== rowId))}
              onRoute={() => {}}
              disabled={false}
            />
          </div>
          <footer data-result-actions="true" style={{ height: 84, flex: "0 0 auto" }}>Actions</footer>
          </div>
        </ResultTextDetailsProvider>
      </HostFrameProvider>
    </main>
  </AppLocaleProvider>;
}

createRoot(document.getElementById("root")!).render(<HostGeometryHarness />);
