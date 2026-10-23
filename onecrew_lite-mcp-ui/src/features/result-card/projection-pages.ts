import type { PresentationProjection, PresentationSection } from "@mcp-ui/lib/result-contract";

export type ProjectionRow = { heading: string | null; label: string; value: string; tone?: Extract<PresentationSection, { kind: "notice" }>["tone"] };

export function projectionRows(projection: PresentationProjection): ProjectionRow[] {
  return projection.sections.flatMap((section): ProjectionRow[] => {
    if (section.kind === "creator_save_rows") return section.rows.flatMap(row => Object.entries(row).map(([key, value]) => ({ heading: row.display_name ?? row.row_id, label: key, value: value === null ? "null" : Array.isArray(value) ? value.length ? value.join("\n") : "[]" : String(value) })));
    if (section.kind === "fields") return section.fields.length ? section.fields.map((field) => ({ heading: section.heading, label: field.label, value: field.value })) : [{ heading: section.heading, label: "", value: "" }];
    if (section.kind === "items") return section.items.length ? section.items.map((item) => ({ heading: section.heading, label: item.title, value: item.description ?? "" })) : [{ heading: section.heading, label: "", value: "" }];
    return [{ heading: section.heading, label: "", value: section.body, tone: section.tone }];
  });
}
