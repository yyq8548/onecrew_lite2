import type { SearchPlanEditorDraft } from "../discovery/search-plan-contracts.ts";
import { presentationProjection, type PresentationProjection } from "./contracts.ts";

export function searchPlanProjection(draft: SearchPlanEditorDraft): PresentationProjection {
  return presentationProjection.parse({ kind: "search_plan_editor", title: "Review search plan", summary: "Edit acceptance criteria and keywords, then confirm before searching.", sections: [{ kind: "fields", heading: "Search plan", fields: [{ label: "Acceptance criteria", value: draft.description }] }] });
}
