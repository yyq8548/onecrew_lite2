import { useState } from "react";
import { ArrowUpDown, Download, Loader2, Upload } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
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
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  CREATORS_QUERY_KEY,
  cancelCreatorImport,
  confirmCreatorImport,
  exportCreators,
  prepareCreatorImportFile,
  type CreatorImportFilePrepareResult,
} from "@/lib/creator";

import { creatorExportSuccessCopy } from "./creator-export-model";
import {
  creatorImportConflictAccountLabel,
  creatorImportConflictLabel,
  creatorImportImpactRows,
} from "./creator-import-model";

type ImportPreview = Extract<
  CreatorImportFilePrepareResult,
  { status: "blocked" | "confirmation_required" }
>;

export function CreatorTransferMenu({
  disabled = false,
  container,
}: {
  disabled?: boolean;
  container?: HTMLElement | null;
}) {
  const { t } = useTranslation("leads");
  const queryClient = useQueryClient();
  const [pendingAction, setPendingAction] = useState<
    "export" | "prepare" | "confirm" | "cancel" | null
  >(null);
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const pending = pendingAction !== null;

  const exportBundle = async () => {
    setPendingAction("export");
    try {
      const copy = creatorExportSuccessCopy(await exportCreators(), t);
      toast.success(copy.title, { description: copy.description });
    } catch (cause) {
      toast.error(t("transfer.export.failed"), { description: String(cause) });
    } finally {
      setPendingAction(null);
    }
  };

  const chooseImport = async () => {
    setPendingAction("prepare");
    setError(null);
    try {
      const result = await prepareCreatorImportFile();
      if (result.status === "cancelled") return;
      if (result.status === "already_imported") {
        toast.info(t("transfer.import.alreadyImported"), {
          description: t("transfer.import.fileSummary", {
            fileName: result.file_name,
            count: result.receipt.creator_count,
          }),
        });
        return;
      }
      setPreview(result);
    } catch (cause) {
      toast.error(t("transfer.import.readFailed"), { description: String(cause) });
    } finally {
      setPendingAction(null);
    }
  };

  const confirmImport = async () => {
    if (preview?.status !== "confirmation_required") return;
    setPendingAction("confirm");
    setError(null);
    try {
      const result = await confirmCreatorImport(preview.operation_id);
      if (result.status === "imported") {
        await queryClient.invalidateQueries({ queryKey: CREATORS_QUERY_KEY });
        toast.success(t("transfer.import.success"), {
          description: t("transfer.import.fileSummary", {
            fileName: preview.file_name,
            count: result.receipt.creator_count,
          }),
        });
      } else if (result.status === "unchanged") {
        toast.info(t("transfer.import.unchanged"), {
          description: t("transfer.import.unchangedDescription", { fileName: preview.file_name }),
        });
      } else if (result.status === "cancelled") {
        toast.info(t("transfer.import.cancelled"));
      } else {
        toast.error(
          result.status === "expired" ? t("transfer.import.expired") : t("transfer.import.changed"),
          { description: t("transfer.import.retryDescription") },
        );
      }
      setPreview(null);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setPendingAction(null);
    }
  };

  const closePreview = async () => {
    if (!preview) return;
    if (preview.status === "blocked") {
      setPreview(null);
      setError(null);
      return;
    }
    setPendingAction("cancel");
    setError(null);
    try {
      await cancelCreatorImport(preview.operation_id);
      setPreview(null);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setPendingAction(null);
    }
  };

  const changeOpen = (open: boolean) => {
    if (pending || open) return;
    void closePreview();
  };

  const impactRows = preview?.status === "confirmation_required"
    ? creatorImportImpactRows(preview.impact, t)
    : [];

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="outline"
            size="sm"
            className="h-8"
            disabled={disabled || pending}
            aria-label={pending ? t("transfer.menu.ariaPending") : t("transfer.menu.ariaIdle")}
            aria-busy={pending}
          >
            {pending ? <Loader2 className="size-4 animate-spin" /> : <ArrowUpDown className="size-4" />}
            {pending ? t("transfer.menu.pending") : t("transfer.menu.idle")}
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-56">
          <DropdownMenuItem onSelect={() => void chooseImport()}>
            <Upload />{t("transfer.menu.importExcel")}
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => void exportBundle()}>
            <Download />{t("transfer.menu.exportExcel")}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <Dialog open={preview !== null} onOpenChange={changeOpen}>
        <DialogContent
          container={container}
          className="sm:max-w-xl"
          onEscapeKeyDown={(event) => pending && event.preventDefault()}
          onPointerDownOutside={(event) => pending && event.preventDefault()}
        >
          <DialogHeader>
            <DialogTitle>
              {preview?.status === "blocked" ? t("transfer.import.blockedTitle") : t("transfer.import.confirmTitle")}
            </DialogTitle>
            <DialogDescription>
              {preview?.status === "blocked"
                ? t("transfer.import.blockedDescription", { fileName: preview.file_name })
                : t("transfer.import.confirmDescription", {
                  fileName: preview?.file_name ?? t("transfer.import.selectedFile"),
                })}
            </DialogDescription>
          </DialogHeader>

          {preview?.status === "blocked" ? (
            <div className="max-h-64 space-y-2 overflow-y-auto rounded-md border p-3 text-sm">
              <p className="font-medium">{t("transfer.import.conflictCount", { count: preview.conflict_count })}</p>
              {preview.conflicts.map((conflict, index) => (
                <div key={`${conflict.incoming_creator_key}-${conflict.kind}-${index}`} className="rounded-md bg-muted/60 p-2">
                  <p>{creatorImportConflictLabel(conflict, t)}</p>
                  {creatorImportConflictAccountLabel(conflict) && (
                    <p className="mt-1 break-all text-xs text-muted-foreground">
                      {t("transfer.import.account", { account: creatorImportConflictAccountLabel(conflict) ?? "" })}
                    </p>
                  )}
                  <p className="mt-1 break-all text-xs text-muted-foreground">
                    {conflict.local_creator_ids.length > 0
                      ? t("transfer.import.conflictCreatorsWithLocal", {
                        incoming: conflict.incoming_creator_key,
                        ids: conflict.local_creator_ids.join(", "),
                      })
                      : t("transfer.import.conflictCreatorOnly", {
                        incoming: conflict.incoming_creator_key,
                      })}
                  </p>
                </div>
              ))}
            </div>
          ) : (
            <div className="grid grid-cols-2 gap-2 rounded-md border p-3 text-sm">
              {impactRows.length === 0 && (
                <p className="col-span-2 text-muted-foreground">{t("transfer.import.noChanges")}</p>
              )}
              {impactRows.map((row) => (
                <div key={row.key} className="flex items-center justify-between gap-3 rounded-md bg-muted/60 px-3 py-2">
                  <span className="text-muted-foreground">{row.label}</span>
                  <span className="font-medium tabular-nums">{row.count}</span>
                </div>
              ))}
            </div>
          )}

          {error && <p role="alert" className="text-sm text-destructive">{error}</p>}
          {preview?.status === "confirmation_required" && preview.preview_rows?.length > 0 && (
            <div className="max-h-72 space-y-3 overflow-auto rounded-md border p-3 text-sm">
              {preview.preview_rows.map(row => <div key={row.record_key} className="border-b pb-2 last:border-0">
                <p className="font-medium">{row.display_name}</p>
                {row.contact_routes.map(route => <p key={`${route.channel}:${route.normalized_value}`} className="break-all text-muted-foreground">{route.channel === "wechat" ? t("channels.wechat") : route.channel}: {route.display_value}</p>)}
                {row.notes && <details><summary>{t("transfer.import.sourceDetails")}</summary><p className="whitespace-pre-wrap break-words text-xs text-muted-foreground">{row.notes}</p></details>}
              </div>)}
            </div>
          )}

          <DialogFooter>
            <Button variant="outline" disabled={pending} onClick={() => changeOpen(false)}>
              {pendingAction === "cancel" && <Loader2 className="size-4 animate-spin" />}
              {pendingAction === "cancel"
                ? t("transfer.import.cancelling")
                : preview?.status === "blocked" ? t("common.actions.close") : t("common.actions.cancel")}
            </Button>
            {preview?.status === "confirmation_required" && (
              <Button disabled={pending} onClick={() => void confirmImport()}>
                {pendingAction === "confirm" && <Loader2 className="size-4 animate-spin" />}
                {pendingAction === "confirm" ? t("transfer.import.importing") : t("transfer.import.confirm")}
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
