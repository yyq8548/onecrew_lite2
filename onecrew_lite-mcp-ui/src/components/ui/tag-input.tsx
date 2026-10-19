import { useId, useRef, useState } from "react";
import { X } from "lucide-react";
import { Badge } from "@mcp-ui/components/ui/badge";
import { Button } from "@mcp-ui/components/ui/button";
import { Label } from "@mcp-ui/components/ui/label";
import { appendTags, tagCommitKey } from "@mcp-ui/components/ui/tag-input-state";

/** Controlled, local draft only. Limits and accessible copy belong to the caller.
 * Enter/comma commit; comma/newline paste adds phrases atomically. Spaces remain
 * within a phrase. No Host calls, form submission, or business defaults. */
export function TagInput({ value, onChange, maxItems, maxLength, lengthOf, onPendingInputChange, disabled = false, showHelp = true, copy }: {
  value: string[]; onChange: (value: string[]) => void; maxItems: number; maxLength: number; disabled?: boolean;
  showHelp?: boolean;
  lengthOf?: (text: string) => number;
  onPendingInputChange?: (pending: boolean) => void;
  copy: { label: string; help: string; placeholder: string; add: string; remove: (value: string) => string; previous: string; next: string; page: (current: number, total: number) => string; countError: string; lengthError: string; duplicateError: string };
}) {
  const id = useId();
  const composing = useRef(false);
  const [draft, setDraft] = useState("");
  const [requestedPage, setPage] = useState(0);
  const [error, setError] = useState<"count" | "length" | "duplicate" | null>(null);
  const total = Math.max(1, Math.ceil(value.length / 6));
  const page = Math.min(requestedPage, total - 1);
  const commit = (input: string) => {
    const result = appendTags(value, input, maxItems, maxLength, lengthOf);
    setError(result.error);
    if (result.error) return;
    onChange(result.values);
    setDraft("");
    onPendingInputChange?.(false);
    setPage(Math.max(0, Math.ceil(result.values.length / 6) - 1));
  };
  return <div className="grid min-w-0 gap-2" data-slot="tag-input">
    <Label htmlFor={id}>{copy.label}</Label>
    <ul className="flex min-w-0 flex-wrap gap-3">
      {value.slice(page * 6, (page + 1) * 6).map((tag, index) => <li key={`${tag}-${index}`} className="min-w-0 max-w-full">
        <Badge variant="secondary" className="max-w-full gap-1 text-sm"><span className="max-w-28 truncate" title={tag}>{tag}</span>
          <button type="button" className="shrink-0 rounded-sm focus-visible:bg-accent disabled:opacity-50" disabled={disabled} aria-label={copy.remove(tag)} onClick={() => { onChange(value.filter((_, item) => item !== page * 6 + index)); setError(null); }}><X className="size-4" aria-hidden="true" /></button>
        </Badge>
      </li>)}
    </ul>
    {total > 1 && <div className="flex items-center justify-between gap-2">
      <Button type="button" variant="ghost" size="sm" disabled={disabled || page === 0} onClick={() => setPage(page - 1)}>{copy.previous}</Button>
      <span role="status" className="text-sm">{copy.page(page + 1, total)}</span>
      <Button type="button" variant="ghost" size="sm" disabled={disabled || page + 1 >= total} onClick={() => setPage(page + 1)}>{copy.next}</Button>
    </div>}
    <div className="flex min-w-0 gap-2">
      <input id={id} className="h-9 min-w-0 flex-1 rounded-md border border-input bg-transparent px-2 text-sm outline-none focus-visible:border-ring disabled:opacity-50" value={draft} disabled={disabled} placeholder={copy.placeholder} aria-invalid={error != null} aria-describedby={`${id}-help${error ? ` ${id}-error` : ""}`}
        onChange={(event) => { setDraft(event.currentTarget.value); onPendingInputChange?.(event.currentTarget.value.trim().length > 0); setError(null); }}
        onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; }}
        onKeyDown={(event) => { if (tagCommitKey(event.key, composing.current || event.nativeEvent.isComposing, event.keyCode)) { event.preventDefault(); commit(draft); } }}
        onPaste={(event) => { const text = event.clipboardData.getData("text"); if (!composing.current && /[,\r\n]/.test(text)) { event.preventDefault(); const input = event.currentTarget; const pasted = draft.slice(0, input.selectionStart ?? draft.length) + text + draft.slice(input.selectionEnd ?? draft.length); const result = appendTags(value, pasted, maxItems, maxLength, lengthOf); if (result.error) { setDraft(pasted); onPendingInputChange?.(true); } commit(pasted); } }} />
      <Button type="button" variant="outline" disabled={disabled || !draft.trim()} onClick={() => commit(draft)}>{copy.add}</Button>
    </div>
    <p id={`${id}-help`} className={showHelp ? "text-sm text-muted-foreground" : "sr-only"}>{copy.help}</p>
    {error && <p id={`${id}-error`} role="alert" className="text-sm text-destructive">{error === "duplicate" ? copy.duplicateError : error === "count" ? copy.countError : copy.lengthError}</p>}
  </div>;
}
