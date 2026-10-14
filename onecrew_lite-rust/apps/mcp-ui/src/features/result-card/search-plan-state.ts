import type { ResultBinding, SearchPlanEditorDraft, SearchPlanPatch } from "@mcp-ui/lib/result-contract";
import { searchPlanDescriptionValid, searchPlanKeywordsValid } from "@mcp-ui/lib/result-contract";

export type SearchPlanFields = Pick<SearchPlanEditorDraft, "description" | "keywords">;
export type SearchPlanEdit = {
  binding: ResultBinding;
  digest: string;
  original: SearchPlanFields;
  fields: SearchPlanFields;
};

export const utf8Length = (text: string) => new TextEncoder().encode(text).length;

export function startSearchPlanEdit(draft: SearchPlanEditorDraft, binding: ResultBinding, digest: string): SearchPlanEdit {
  const fields = { description: draft.description, keywords: [...draft.keywords] };
  return { binding, digest, original: fields, fields };
}

export function searchPlanEditPatch(edit: SearchPlanEdit): SearchPlanPatch {
  return {
    ...(edit.fields.description !== edit.original.description ? { description: edit.fields.description } : {}),
    ...(JSON.stringify(edit.fields.keywords) !== JSON.stringify(edit.original.keywords) ? { keywords: edit.fields.keywords } : {}),
  };
}

export function searchPlanEditStale(edit: SearchPlanEdit, binding: ResultBinding, digest: string): boolean {
  return edit.binding.result_ref !== binding.result_ref || edit.binding.fingerprint !== binding.fingerprint || edit.binding.view.identity !== binding.view.identity || edit.binding.revision !== binding.revision || edit.digest !== digest;
}

// Explicit user rebase preserves only fields they edited. Untouched fields use
// the newer server preview, so a concurrent update is not silently overwritten.
export function rebaseSearchPlanEdit(edit: SearchPlanEdit, draft: SearchPlanEditorDraft, binding: ResultBinding, digest: string): SearchPlanEdit {
  const patch = searchPlanEditPatch(edit);
  const current = startSearchPlanEdit(draft, binding, digest);
  return { ...current, fields: {
    description: patch.description ?? current.fields.description,
    keywords: patch.keywords ?? current.fields.keywords,
  } };
}

// Only description and keywords are editable: SearchPlanPatch accepts nothing
// else, and operation-bound scopes are read-only in the App.
export function searchPlanFieldsError(fields: SearchPlanFields): "description" | "keywords" | null {
  if (!searchPlanDescriptionValid(fields.description)) return "description";
  if (!searchPlanKeywordsValid(fields.keywords)) return "keywords";
  return null;
}
