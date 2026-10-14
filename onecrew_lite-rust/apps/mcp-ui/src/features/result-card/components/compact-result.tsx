import type { ResultSnapshot } from "@mcp-ui/lib/result-contract";
import { ProjectionSections } from "./projection-sections";

/** Keep returned facts intact; remove only exact duplicates from their visual projection. */
export function CompactResult({ snapshot }: { snapshot: ResultSnapshot }) {
 const refs=[...new Map(snapshot.batch.outcomes.flatMap(row=>row.resource_refs).map(ref=>[`${ref.kind}:${ref.id}`,ref])).values()];
 const outcomeCreatorIds = new Set(snapshot.presentation.sections.flatMap(section => section.kind === "creator_save_rows" ? section.rows.flatMap(row => row.creator_id ? [row.creator_id] : []) : []));
 const visibleRefs = refs.filter(ref => ref.kind !== "creator" || !outcomeCreatorIds.has(ref.id));
 const names=new Set(refs.flatMap(ref=>ref.label?[ref.label]:[]));
 const ids=new Set(refs.map(ref=>ref.id));
 const sections=snapshot.presentation.sections.flatMap<ResultSnapshot["presentation"]["sections"][number]>(section=>{
  if(section.kind!=="fields")return [section];
  const fields=section.fields.filter(field=>!(names.has(field.value) && /^(creator|name|display name|创作者|达人|名称)$/i.test(field.label)) && !(ids.has(field.value) && /(?:\bid\b|编号)/i.test(field.label)));
  return fields.length?[{...section,fields}]:[];
 });
 if (!visibleRefs.some(ref=>ref.label) && sections.length === 0) return null;
 return <div className="grid min-w-0 gap-4" data-compact-result="true">
  {visibleRefs.some(ref=>ref.label) && <ul className="grid gap-2">{visibleRefs.filter(ref=>ref.label).map(ref=><li key={`${ref.kind}:${ref.id}`} className="text-sm [overflow-wrap:anywhere]">{ref.label}</li>)}</ul>}
  {sections.length>0 && <ProjectionSections projection={{...snapshot.presentation,sections}}/>}
 </div>;
}
