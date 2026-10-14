import type { ComponentType } from "react";

import type { PresentationKind, PresentationProjection } from "@mcp-ui/lib/result-contract";
import { ProjectionSections } from "@mcp-ui/features/result-card/components/projection-sections";
import { SearchPlanProjectionView } from "@mcp-ui/features/result-card/components/search-plan-preview";

export type ResultViewProps = { projection: PresentationProjection };

const VIEW_COMPONENTS: Record<PresentationKind, ComponentType<ResultViewProps>> = {
  creator_save_batch_confirmation: ProjectionSections,
  creator_save_batch_outcome: ProjectionSections,
  // A manual Creator batch review and its receipt are ordinary field/item
  // projections: the runtime owner already named every Creator and route, so
  // the card renders those sections rather than a bespoke view.
  creator_batch_confirmation: ProjectionSections,
  creator_batch_outcome: ProjectionSections,
  outreach_confirmation: OutreachDraftView,
  outreach_outcome: OutreachDraftView,
  email_confirmation: ProjectionSections,
  email_outcome: ProjectionSections,
  search_plan_editor: SearchPlanProjectionView,
  outreach_draft: OutreachDraftView,
  record_preview: RecordPreviewView,
  simple_operation: SimpleOperationView,
  read_only: ReadOnlyView,
  choice: ChoiceProjectionView,
  batch_result: BatchResultView,
};

/** Presentation kinds that have a business view. */
export const RESULT_VIEW_KINDS = Object.keys(VIEW_COMPONENTS) as PresentationKind[];

export function ResultBusinessView({ projection }: ResultViewProps) {
  const View = VIEW_COMPONENTS[projection.kind];
  return <View projection={projection} />;
}

export function OutreachDraftView(props: ResultViewProps) { return <ProjectionSections {...props} />; }
export function RecordPreviewView(props: ResultViewProps) { return <ProjectionSections {...props} />; }
export function SimpleOperationView(props: ResultViewProps) { return <ProjectionSections {...props} />; }
export function ReadOnlyView(props: ResultViewProps) { return <ProjectionSections {...props} />; }
export function ChoiceProjectionView(props: ResultViewProps) { return <ProjectionSections {...props} />; }
export function BatchResultView(props: ResultViewProps) { return <ProjectionSections {...props} />; }
