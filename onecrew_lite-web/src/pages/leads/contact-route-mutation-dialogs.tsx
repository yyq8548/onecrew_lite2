import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  CREATORS_QUERY_KEY,
  confirmCreatorContactRouteDelete,
  prepareCreatorContactRouteDelete,
  updateManualCreatorContactRoute,
  type ContactChannel,
  type CreatorContactRouteDeletePrepareResult,
  type CreatorContactRouteRecord,
  type ManualCreatorContactRouteUpdateResult,
} from "@/lib/creator";
import { ContactRouteFields } from "./contact-route-fields";
import {
  contactRouteDeleteBlockingArtifactLabel,
  contactRouteDeleteBlockedMessage,
  contactRouteDeleteConfirmInput,
  contactRouteDeleteDescription,
  contactRouteDeleteImpactItems,
  contactRouteDeletePrepareInput,
  contactRouteDeleteRetryKey,
  contactRouteEditDraft,
  manualContactRouteUpdateInput,
  type ContactRouteDeleteRetryKey,
  type ManualEntryValidationKey,
} from "./manual-entry-model";
import { runDialogMutationOnce, shouldApplyUserOpenChange } from "./manual-dialog-state";

type EditRouteError =
  | { kind: "fixed"; key: ManualEntryValidationKey | "contactRoutes.edit.replacementRequired" }
  | { kind: "detail"; value: string };

type DeleteRouteError =
  | { kind: "fixed"; key: ContactRouteDeleteRetryKey }
  | { kind: "detail"; value: string };

export function EditContactRouteDialog({
  route,
  onClose,
}: {
  route: CreatorContactRouteRecord | null;
  onClose: () => void;
}) {
  const { t } = useTranslation("leads");
  const queryClient = useQueryClient();
  const [channel, setChannel] = useState<ContactChannel>("email");
  const [value, setValue] = useState("");
  const [sourceUrl, setSourceUrl] = useState("");
  const [error, setError] = useState<EditRouteError | null>(null);
  const [pending, setPending] = useState(false);
  const pendingRef = useRef(false);
  const valueRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!route) return;
    const draft = contactRouteEditDraft(route);
    setChannel(draft.channel);
    setValue(draft.value);
    setSourceUrl(draft.sourceUrl);
    setError(null);
    window.setTimeout(() => valueRef.current?.focus(), 0);
  }, [route]);

  const changeOpen = (next: boolean) => {
    if (!shouldApplyUserOpenChange(pendingRef.current, next)) return;
    if (!next) onClose();
  };

  const submit = async () => {
    if (!route || pendingRef.current) return;
    const draft = manualContactRouteUpdateInput({
      creatorId: route.route.creator_id,
      contactRouteId: route.route.id,
      channel,
      value,
      sourceUrl,
    });
    if (!draft.input) {
      setError({ kind: "fixed", key: draft.error });
      valueRef.current?.focus();
      return;
    }
    setError(null);
    try {
      let result: ManualCreatorContactRouteUpdateResult | null = null;
      const run = await runDialogMutationOnce({
        lock: pendingRef,
        onPendingChange: setPending,
        mutation: async () => {
          result = await updateManualCreatorContactRoute(draft.input);
          return result;
        },
        afterSuccess: async () => {
          if (result?.status !== "replacement_required") {
            await queryClient.invalidateQueries({ queryKey: CREATORS_QUERY_KEY });
          }
        },
      });
      if (!run.started) return;
      if (run.value.status === "replacement_required") {
        setError({ kind: "fixed", key: "contactRoutes.edit.replacementRequired" });
        valueRef.current?.focus();
        return;
      }
      toast.success(run.value.status === "unchanged"
        ? t("contactRoutes.edit.unchanged")
        : t("contactRoutes.edit.success"));
      onClose();
    } catch (cause) {
      setError({ kind: "detail", value: String(cause) });
    }
  };

  const errorMessage = error?.kind === "fixed" ? t(error.key) : error?.value ?? null;

  return (
    <Dialog open={Boolean(route)} onOpenChange={changeOpen}>
      <DialogContent
        className="sm:max-w-lg"
        showCloseButton={!pending}
        onEscapeKeyDown={(event) => pendingRef.current && event.preventDefault()}
        onPointerDownOutside={(event) => pendingRef.current && event.preventDefault()}
        onInteractOutside={(event) => pendingRef.current && event.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle>{t("contactRoutes.edit.title")}</DialogTitle>
          <DialogDescription>{t("contactRoutes.edit.description")}</DialogDescription>
        </DialogHeader>
        <form className="space-y-4 py-2" onSubmit={(event) => { event.preventDefault(); void submit(); }}>
          <ContactRouteFields
            idPrefix="edit-route"
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
              {pending ? t("contactRoutes.edit.submitting") : t("contactRoutes.edit.submit")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function DeleteContactRouteDialog({
  route,
  onClose,
}: {
  route: CreatorContactRouteRecord | null;
  onClose: () => void;
}) {
  const { t } = useTranslation("leads");
  const [error, setError] = useState<DeleteRouteError | null>(null);
  const [prepared, setPrepared] = useState<CreatorContactRouteDeletePrepareResult | null>(null);
  const [pending, setPending] = useState(false);
  const pendingRef = useRef(false);

  useEffect(() => {
    setError(null);
    setPrepared(null);
  }, [route]);

  const changeOpen = (next: boolean) => {
    if (!shouldApplyUserOpenChange(pendingRef.current, next)) return;
    if (!next) onClose();
  };

  const prepare = async () => {
    if (!route || pendingRef.current) return;
    setError(null);
    try {
      const run = await runDialogMutationOnce({
        lock: pendingRef,
        onPendingChange: setPending,
        mutation: () => prepareCreatorContactRouteDelete(contactRouteDeletePrepareInput(route)),
        afterSuccess: async () => undefined,
      });
      if (run.started) setPrepared(run.value);
    } catch (cause) {
      setError({ kind: "detail", value: String(cause) });
    }
  };

  const confirm = async () => {
    if (prepared?.status !== "confirmation_required" || pendingRef.current) return;
    setError(null);
    try {
      const run = await runDialogMutationOnce({
        lock: pendingRef,
        onPendingChange: setPending,
        mutation: () => confirmCreatorContactRouteDelete(contactRouteDeleteConfirmInput(prepared.operation_id)),
        afterSuccess: async () => undefined,
      });
      if (!run.started) return;
      if (run.value.status === "stale" || run.value.status === "expired") {
        setPrepared(null);
        setError({ kind: "fixed", key: contactRouteDeleteRetryKey(run.value) });
        return;
      }
      toast.success(t("contactRoutes.delete.success"));
      onClose();
    } catch (cause) {
      setError({ kind: "detail", value: String(cause) });
    }
  };

  const blocked = prepared?.status === "blocked" ? prepared : null;
  const confirmation = prepared?.status === "confirmation_required" ? prepared : null;
  const errorMessage = error?.kind === "fixed" ? t(error.key) : error?.value ?? null;

  return (
    <Dialog open={Boolean(route)} onOpenChange={changeOpen}>
      <DialogContent
        showCloseButton={!pending}
        onEscapeKeyDown={(event) => pendingRef.current && event.preventDefault()}
        onPointerDownOutside={(event) => pendingRef.current && event.preventDefault()}
        onInteractOutside={(event) => pendingRef.current && event.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle>{t("contactRoutes.delete.title")}</DialogTitle>
          <DialogDescription>{route ? contactRouteDeleteDescription(route, t) : t("contactRoutes.delete.fallbackDescription")}</DialogDescription>
        </DialogHeader>
        {blocked && (
          <div className="space-y-3 text-sm">
            <p className="font-medium text-destructive">
              {contactRouteDeleteBlockedMessage(blocked.reason, t)}
            </p>
            {blocked.children.length > 0 && (
              <div className="space-y-2">
                <p className="text-xs font-medium text-muted-foreground">{t("contactRoutes.delete.derivedRoutesHeading")}</p>
                <ul className="space-y-1 rounded-md border p-3">
                  {blocked.children.map((child) => (
                    <li key={`${child.creator_id}:${child.contact_route_id}`} className="flex justify-between gap-3">
                      <span className="truncate">{child.display_value}</span>
                      <span className="shrink-0 text-muted-foreground">{child.channel}</span>
                    </li>
                  ))}
                </ul>
              </div>
            )}
            {blocked.blocking_artifacts.length > 0 && (
              <ul className="space-y-1 rounded-md border p-3">
                {blocked.blocking_artifacts.map((artifact) => (
                  <li key={artifact.kind} className="flex justify-between gap-3">
                    <span>{contactRouteDeleteBlockingArtifactLabel(artifact.kind, t)}</span>
                    <span className="tabular-nums text-muted-foreground">{artifact.count}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
        {confirmation && (
          <div className="space-y-3 text-sm">
            <p>{t("contactRoutes.delete.confirmImpact")}</p>
            <div className="grid grid-cols-2 gap-x-5 gap-y-1 rounded-md border p-3">
              {contactRouteDeleteImpactItems(confirmation.impact, t).map((item) => (
                <div key={item.key} className="flex justify-between gap-3">
                  <span className="text-muted-foreground">{item.label}</span>
                  <span className="tabular-nums">{item.count}</span>
                </div>
              ))}
              <div className="col-span-2 mt-2 flex justify-between border-t pt-2 font-medium">
                <span>{t("contactRoutes.delete.localRecordsTotal")}</span>
                <span className="tabular-nums">{confirmation.impact.local_records_total}</span>
              </div>
            </div>
            <p className="rounded-md bg-muted px-3 py-2 text-muted-foreground">
              {t("contactRoutes.delete.remoteDataNotice")}
            </p>
          </div>
        )}
        {errorMessage && <p role="alert" className="text-sm text-destructive">{errorMessage}</p>}
        <DialogFooter>
          <Button type="button" variant="outline" disabled={pending} onClick={() => changeOpen(false)}>{t("common.actions.close")}</Button>
          <Button
            type="button"
            variant={confirmation ? "destructive" : "default"}
            disabled={pending}
            onClick={() => void (confirmation ? confirm() : prepare())}
          >
            {pending && <Loader2 className="size-4 animate-spin" />}
            {pending
              ? t("contactRoutes.delete.checking")
              : confirmation
                ? t("contactRoutes.delete.confirm")
                : blocked
                  ? t("contactRoutes.delete.recheck")
                  : t("contactRoutes.delete.checkAndContinue")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
