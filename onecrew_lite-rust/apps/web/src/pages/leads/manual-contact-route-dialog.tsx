import { useEffect, useRef, useState } from "react";
import { Loader2, Plus } from "lucide-react";
import { toast } from "sonner";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { addManualCreatorContactRoute, CREATORS_QUERY_KEY, type ContactChannel } from "@/lib/creator";
import { ContactRouteFields } from "./contact-route-fields";
import { manualContactRouteInput, type ManualEntryValidationKey } from "./manual-entry-model";
import { openAfterSuccessfulSubmit, runDialogMutationOnce, shouldApplyUserOpenChange } from "./manual-dialog-state";

export function ManualContactRouteDialog({ creatorId, disabled }: { creatorId: string; disabled: boolean }) {
  const { t } = useTranslation("leads");
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [channel, setChannel] = useState<ContactChannel>("email");
  const [value, setValue] = useState("");
  const [sourceUrl, setSourceUrl] = useState("");
  const [error, setError] = useState<
    | { kind: "fixed"; key: ManualEntryValidationKey }
    | { kind: "detail"; value: string }
    | null
  >(null);
  const [pending, setPending] = useState(false);
  const pendingRef = useRef(false);
  const valueRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) window.setTimeout(() => valueRef.current?.focus(), 0);
  }, [open]);

  const reset = () => {
    setChannel("email");
    setValue("");
    setSourceUrl("");
    setError(null);
  };

  const changeOpen = (next: boolean) => {
    if (!shouldApplyUserOpenChange(pendingRef.current, next)) return;
    setOpen(next);
    if (!next) reset();
  };

  const submit = async () => {
    if (pendingRef.current) return;
    const draft = manualContactRouteInput({ creatorId, channel, value, sourceUrl });
    if (!draft.input) {
      setError({ kind: "fixed", key: draft.error });
      valueRef.current?.focus();
      return;
    }
    setError(null);
    try {
      const run = await runDialogMutationOnce({
        lock: pendingRef,
        onPendingChange: setPending,
        mutation: () => addManualCreatorContactRoute(draft.input),
        afterSuccess: () => queryClient.invalidateQueries({ queryKey: CREATORS_QUERY_KEY }),
      });
      if (!run.started) return;
      toast.success(run.value.status === "already_exists"
        ? t("contactRoutes.add.alreadyExists")
        : t("contactRoutes.add.success"));
      reset();
      setOpen(openAfterSuccessfulSubmit());
    } catch (cause) {
      setError({ kind: "detail", value: String(cause) });
    }
  };

  const errorMessage = error?.kind === "fixed" ? t(error.key) : error?.value ?? null;

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogTrigger asChild>
        <Button type="button" variant="outline" size="sm" disabled={disabled}>
          <Plus className="size-4" />{t("contactRoutes.add.trigger")}
        </Button>
      </DialogTrigger>
      <DialogContent
        className="sm:max-w-lg"
        showCloseButton={!pending}
        onEscapeKeyDown={(event) => pendingRef.current && event.preventDefault()}
        onPointerDownOutside={(event) => pendingRef.current && event.preventDefault()}
        onInteractOutside={(event) => pendingRef.current && event.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle>{t("contactRoutes.add.title")}</DialogTitle>
          <DialogDescription>{t("contactRoutes.add.description")}</DialogDescription>
        </DialogHeader>
        <form className="space-y-4 py-2" onSubmit={(event) => { event.preventDefault(); void submit(); }}>
          <ContactRouteFields
            idPrefix="manual-route"
            channel={channel}
            value={value}
            sourceUrl={sourceUrl}
            pending={pending}
            error={errorMessage}
            valueRef={valueRef}
            onChannelChange={setChannel}
            onValueChange={setValue}
            onSourceUrlChange={setSourceUrl}
          />
          <DialogFooter>
            <Button type="button" variant="outline" disabled={pending} onClick={() => changeOpen(false)}>{t("common.actions.cancel")}</Button>
            <Button type="submit" disabled={pending || !value.trim()}>
              {pending && <Loader2 className="size-4 animate-spin" />}
              {pending ? t("contactRoutes.add.submitting") : t("contactRoutes.add.submit")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
