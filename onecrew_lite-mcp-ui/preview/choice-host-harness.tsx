import { useReducer, useState } from "react";
import { createRoot } from "react-dom/client";

import "../src/styles/theme.css";
import { HostFrameProvider, type HostFrame } from "../src/app/host-frame";
import { ChoiceAction, ChoiceActions } from "../src/features/result-card/components/choice-action";
import { initialChoiceState, reduceChoice, type ChoiceAction as DraftAction } from "../src/features/result-card/choice-state";
import { ResultTextDetailsProvider } from "../src/features/result-card/components/text-details";
import { AppLocaleProvider } from "../src/lib/i18n";
import type { ResultChoiceQuestion } from "../src/lib/result-contract";

const questions: ResultChoiceQuestion[] = [{
  id: "scope",
  header: "Target scope",
  prompt: "Choose the scope for this local test.",
  selection_mode: "single",
  options: [{ id: "us", label: "United States", description: null }, { id: "jp", label: "Japan", description: null }],
}];

function ChoiceHostHarness() {
  const [state, dispatch] = useReducer((current: ReturnType<typeof initialChoiceState>, action: DraftAction) => reduceChoice(current, action, questions), undefined, initialChoiceState);
  const [submitted, setSubmitted] = useState(0);
  const frame: HostFrame = { displayMode: "inline", availableDisplayModes: ["inline", "fullscreen"], platform: "desktop", touch: false, safeArea: null, availableHeight: 640 };
  return <AppLocaleProvider locale="en-US">
    <main style={{ padding: 16 }}>
      <p data-submit-count="true">host final actions: {submitted}</p>
      <HostFrameProvider frame={frame}>
        <ResultTextDetailsProvider revision={1}>
          <div className="result-shell" style={{ display: "flex", flexDirection: "column" }}>
            <header data-slot="card-header" style={{ height: 80, flex: "0 0 auto" }}>Choice test</header>
            <div data-result-scroll="true" className="result-shell-content">
              <ChoiceAction questions={questions} state={state} dispatch={dispatch} disabled={submitted > 0} />
            </div>
            <footer data-result-actions="true" className="result-shell-footer flex flex-wrap justify-end gap-2">
              <ChoiceActions questions={questions} state={state} dispatch={dispatch} disabled={submitted > 0} onCancel={() => {}} onSubmit={() => setSubmitted(current => current + 1)} />
            </footer>
          </div>
        </ResultTextDetailsProvider>
      </HostFrameProvider>
    </main>
  </AppLocaleProvider>;
}

createRoot(document.getElementById("root")!).render(<ChoiceHostHarness />);
