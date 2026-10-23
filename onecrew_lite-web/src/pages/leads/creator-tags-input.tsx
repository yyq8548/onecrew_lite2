import { useId, useState } from "react";
import { Plus, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  creatorTagsFromDraft,
  trimCreatorTag,
  type CreatorTagsDraft,
  type CreatorTagsValidationKey,
} from "./creator-tags-model";

export function CreatorTagsBadges({ tags }: { tags: readonly string[] }) {
  if (tags.length === 0) return <span className="text-muted-foreground">—</span>;
  return <div className="flex min-w-0 flex-wrap gap-1">
    {tags.map((tag) => <Tooltip key={tag}>
      <TooltipTrigger asChild>
        <Badge variant="secondary" className="max-w-full shrink text-foreground">
          <span className="truncate">{tag}</span>
        </Badge>
      </TooltipTrigger>
      <TooltipContent className="max-w-64 break-all">{tag}</TooltipContent>
    </Tooltip>)}
  </div>;
}

// The installed shadcn headless package has no free-form tag input. Compose
// shared Input/Badge/Button behavior here, within the Creator feature owner.
export function CreatorTagsInput({ value, onChange, disabled = false, error: submitError }: {
  value: CreatorTagsDraft;
  onChange: (value: CreatorTagsDraft) => void;
  disabled?: boolean;
  error?: CreatorTagsValidationKey | null;
}) {
  const { t } = useTranslation("leads");
  const id = useId();
  const [error, setError] = useState<CreatorTagsValidationKey | null>(null);
  const visibleError = error ?? submitError;
  const change = (next: CreatorTagsDraft) => { setError(null); onChange(next); };
  const add = () => {
    const result = creatorTagsFromDraft(value);
    if (result.error) { setError(result.error); return; }
    change({ tags: result.tags, text: "" });
  };
  const count = Array.from(trimCreatorTag(value.text)).length;

  return <div className="min-w-0 space-y-2">
    <Label htmlFor={id}>{t("tags.label")}</Label>
    {value.tags.length > 0 && <div className="flex flex-wrap gap-1">
      {value.tags.map((tag, index) => <Badge key={tag} variant="secondary" className="max-w-full gap-1 pr-0.5">
        <span className="truncate" title={tag}>{tag}</span>
        <Button type="button" variant="ghost" size="icon" className="size-6 shrink-0"
          disabled={disabled} aria-label={t("tags.remove", { tag })}
          onClick={() => change({ ...value, tags: value.tags.filter((_, position) => position !== index) })}>
          <X className="size-3" />
        </Button>
      </Badge>)}
    </div>}
    <div className="flex min-w-0 items-center gap-2">
      <Input id={id} value={value.text} autoComplete="off" disabled={disabled}
        placeholder={t("tags.placeholder")}
        aria-invalid={Boolean(visibleError)}
        aria-describedby={`${id}-hint${visibleError ? ` ${id}-error` : ""}`}
        onChange={(event) => change({ ...value, text: event.target.value })}
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.nativeEvent.isComposing) {
            event.preventDefault();
            add();
          }
        }} />
      <Button type="button" size="icon" variant="outline" disabled={disabled || value.text === ""}
        aria-label={t("tags.add")} title={t("tags.add")} onClick={add}>
        <Plus className="size-4" />
      </Button>
    </div>
    <div id={`${id}-hint`} className="flex items-start justify-between gap-2 text-xs text-muted-foreground">
      <span>{t("tags.hint")}</span>
      <span className="shrink-0 tabular-nums" aria-label={t("tags.characterCount", { count })}>{count}/12</span>
    </div>
    {visibleError && <p id={`${id}-error`} role="alert" className="text-sm text-destructive">{t(visibleError)}</p>}
  </div>;
}
