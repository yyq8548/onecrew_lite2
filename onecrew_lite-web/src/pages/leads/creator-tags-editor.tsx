import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Loader2, Pencil } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { CreatorTagsBadges, CreatorTagsInput } from "./creator-tags-input";
import { creatorTagsFromDraft, type CreatorTagsDraft, type CreatorTagsValidationKey } from "./creator-tags-model";
import { creatorWriteErrorDetail, saveCreatorTags } from "./creator-tags-mutation";

export function CreatorTagsEditor({ creatorId, tags, editable }: {
  creatorId: string;
  tags: string[];
  editable: boolean;
}) {
  const { t } = useTranslation("leads");
  const queryClient = useQueryClient();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<CreatorTagsDraft>({ tags, text: "" });
  const [validation, setValidation] = useState<CreatorTagsValidationKey | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const begin = () => {
    setDraft({ tags, text: "" });
    setValidation(null);
    setError(null);
    setEditing(true);
  };
  const save = async () => {
    if (pending || !editable) return;
    const resolved = creatorTagsFromDraft(draft);
    if (resolved.error) { setValidation(resolved.error); return; }
    setPending(true);
    setError(null);
    try {
      await saveCreatorTags(queryClient, creatorId, resolved.tags);
      setEditing(false);
      toast.success(t("tags.saved"));
    } catch (cause) {
      setError(creatorWriteErrorDetail(cause) ?? "");
    } finally {
      setPending(false);
    }
  };

  return <div className="min-w-0 flex-[1_1_220px] space-y-1.5">
    {editing && editable ? <form className="space-y-3" onSubmit={(event) => { event.preventDefault(); void save(); }}>
      <CreatorTagsInput value={draft} disabled={pending} error={validation}
        onChange={(value) => { setDraft(value); setValidation(null); setError(null); }} />
      {error !== null && <p role="alert" className="text-sm text-destructive">{t("tags.saveFailed")}{error ? `: ${error}` : ""}</p>}
      <div className="flex flex-wrap justify-end gap-2">
        <Button type="button" size="sm" variant="outline" disabled={pending} onClick={() => setEditing(false)}>{t("common.actions.cancel")}</Button>
        <Button type="submit" size="sm" disabled={pending}>
          {pending && <Loader2 className="size-4 animate-spin" />}{t(pending ? "tags.saving" : "tags.save")}
        </Button>
      </div>
    </form> : <>
      <div className="flex min-h-4 items-center justify-between gap-2">
        <div className="text-xs font-medium text-muted-foreground">{t("tags.label")}</div>
        {editable && <Button type="button" variant="ghost" size="sm" className="h-6 px-1.5" onClick={begin}>
          <Pencil className="size-3" />{t("tags.edit")}
        </Button>}
      </div>
      <CreatorTagsBadges tags={tags} />
    </>}
  </div>;
}
