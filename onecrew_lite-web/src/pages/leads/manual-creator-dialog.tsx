import { useEffect, useRef, useState } from "react";
import { Loader2, Plus, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import { Sheet, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle, SheetTrigger } from "@/components/ui/sheet";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { CREATORS_QUERY_KEY, createManualCreator } from "@/lib/creator";
import { manualCreatorInput, type ManualEntryValidationKey } from "./manual-entry-model";
import { openAfterSuccessfulSubmit, shouldApplyUserOpenChange } from "./manual-dialog-state";
import type { ContactChannel } from "@/lib/creator";
import { contactChannelLabel } from "./i18n-labels";
import { CreatorTagsInput } from "./creator-tags-input";
import type { CreatorTagsDraft } from "./creator-tags-model";
import { creatorWriteErrorDetail } from "./creator-tags-mutation";

const CHANNELS: ContactChannel[] = ["email", "x", "instagram", "telegram", "youtube", "tiktok", "website", "link_hub"];
type RouteDraft = { channel: ContactChannel; value: string; sourceUrl: string };

export function ManualCreatorDialog() {
  const { t } = useTranslation("leads");
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [displayName, setDisplayName] = useState("");
  const [notes, setNotes] = useState("");
  const [tags, setTags] = useState<CreatorTagsDraft>({ tags: [], text: "" });
  const [contactRoutes, setContactRoutes] = useState<RouteDraft[]>([{ channel: "email", value: "", sourceUrl: "" }]);
  const [error, setError] = useState<
    | { kind: "fixed"; key: ManualEntryValidationKey }
    | { kind: "detail"; value: string }
    | null
  >(null);
  const [pending, setPending] = useState(false);
  const nameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) window.setTimeout(() => nameRef.current?.focus(), 0);
  }, [open]);

  const reset = () => {
    setDisplayName("");
    setNotes("");
    setTags({ tags: [], text: "" });
    setContactRoutes([{ channel: "email", value: "", sourceUrl: "" }]);
    setError(null);
  };

  const changeOpen = (next: boolean) => {
    if (!shouldApplyUserOpenChange(pending, next)) return;
    setOpen(next);
    if (!next) reset();
  };

  const submit = async () => {
    const draft = manualCreatorInput({ displayName, notes, contactRoutes, tags });
    if (!draft.input) {
      setError({ kind: "fixed", key: draft.error });
      if (draft.error === "contactRoutes.validation.creatorNameRequired") nameRef.current?.focus();
      return;
    }
    setPending(true);
    setError(null);
    try {
      await createManualCreator(draft.input);
      await queryClient.invalidateQueries({ queryKey: CREATORS_QUERY_KEY });
      toast.success(t("manualCreator.success"));
      reset();
      setOpen(openAfterSuccessfulSubmit());
    } catch (cause) {
      setError({ kind: "detail", value: creatorWriteErrorDetail(cause) ?? t("tags.createFailed") });
    } finally {
      setPending(false);
    }
  };

  const errorMessage = error?.kind === "fixed" ? t(error.key) : error?.value ?? null;
  const tagError = error?.kind === "fixed"
    && error.key !== "contactRoutes.validation.creatorNameRequired"
    && error.key !== "contactRoutes.validation.valueRequired" ? error.key : null;

  return (
    <Sheet open={open} onOpenChange={changeOpen}>
      <SheetTrigger asChild>
        <Button size="sm"><Plus className="size-4" />{t("manualCreator.trigger")}</Button>
      </SheetTrigger>
      <SheetContent side="right" className="w-full max-w-md gap-0 sm:max-w-md" onEscapeKeyDown={(event) => pending && event.preventDefault()}>
        <SheetHeader className="shrink-0 border-b p-4 sm:p-6">
          <SheetTitle>{t("manualCreator.title")}</SheetTitle>
          <SheetDescription>{t("manualCreator.description")}</SheetDescription>
        </SheetHeader>
        <form className="flex min-h-0 flex-1 flex-col" onSubmit={(event) => { event.preventDefault(); void submit(); }}>
          <div className="min-h-0 flex-1 space-y-6 overflow-y-auto px-4 py-6 sm:px-6">
            <div className="grid gap-2">
              <Label htmlFor="manual-creator-name">{t("manualCreator.name")}</Label>
              <Input
                ref={nameRef}
                id="manual-creator-name"
                value={displayName}
                onChange={(event) => setDisplayName(event.target.value)}
                aria-invalid={error?.kind === "fixed" && error.key === "contactRoutes.validation.creatorNameRequired"}
                aria-describedby={error && !tagError ? "manual-creator-error" : undefined}
                autoComplete="off"
                disabled={pending}
              />
            </div>
            <CreatorTagsInput value={tags} disabled={pending} error={tagError}
              onChange={(value) => { setTags(value); setError(null); }} />
            <div className="grid gap-2">
              <Label htmlFor="manual-creator-notes">{t("manualCreator.notes")}</Label>
              <Textarea
                id="manual-creator-notes"
                value={notes}
                onChange={(event) => setNotes(event.target.value)}
                rows={4}
                disabled={pending}
              />
            </div>
            <div className="grid gap-2">
              <div className="flex items-center justify-between">
                <Label>{t("manualCreator.initialRoutes")}</Label>
                <Button type="button" size="sm" variant="outline" disabled={pending} onClick={() => setContactRoutes((rows) => [...rows, { channel: "email", value: "", sourceUrl: "" }])}><Plus className="size-4" />{t("common.actions.add")}</Button>
              </div>
              {contactRoutes.map((route, index) => (
                <div key={index} className="grid gap-3 rounded-lg border p-3">
                  <div className="flex items-center gap-2">
                    <Select value={route.channel} disabled={pending} onValueChange={(value) => setContactRoutes((rows) => rows.map((row, rowIndex) => rowIndex === index ? { ...row, channel: value as ContactChannel } : row))}>
                      <SelectTrigger aria-label={t("manualCreator.channelType", { index: index + 1 })} className="min-w-0 flex-1">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {CHANNELS.map((channel) => <SelectItem key={channel} value={channel}>{contactChannelLabel(t, channel)}</SelectItem>)}
                      </SelectContent>
                    </Select>
                    <Button type="button" size="icon" variant="ghost" aria-label={t("manualCreator.deleteRoute", { index: index + 1 })} disabled={pending} onClick={() => setContactRoutes((rows) => rows.filter((_, rowIndex) => rowIndex !== index))}><Trash2 className="size-4" /></Button>
                  </div>
                  <Input aria-label={t("manualCreator.channelValue", { index: index + 1 })} value={route.value} disabled={pending} placeholder={t("manualCreator.channelValuePlaceholder")} onChange={(event) => setContactRoutes((rows) => rows.map((row, rowIndex) => rowIndex === index ? { ...row, value: event.target.value } : row))} />
                  <Input aria-label={t("manualCreator.sourceUrl", { index: index + 1 })} value={route.sourceUrl} disabled={pending} placeholder={t("manualCreator.sourceUrlPlaceholder")} onChange={(event) => setContactRoutes((rows) => rows.map((row, rowIndex) => rowIndex === index ? { ...row, sourceUrl: event.target.value } : row))} />
                </div>
              ))}
            </div>
            {errorMessage && !tagError && <p id="manual-creator-error" role="alert" className="text-sm text-destructive">{errorMessage}</p>}
          </div>
          <SheetFooter className="shrink-0 flex-row justify-end border-t p-4 sm:px-6">
            <Button className="min-h-11 flex-1 sm:flex-none" type="button" variant="outline" disabled={pending} onClick={() => changeOpen(false)}>{t("common.actions.cancel")}</Button>
            <Button className="min-h-11 flex-1 sm:flex-none" type="submit" disabled={pending || !displayName.trim()}>
              {pending && <Loader2 className="size-4 animate-spin" />}
              {pending ? t("manualCreator.submitting") : t("manualCreator.submit")}
            </Button>
          </SheetFooter>
        </form>
      </SheetContent>
    </Sheet>
  );
}
