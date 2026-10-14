import type { CreatorExportResult } from "@/lib/creator";
import type { LeadsT } from "./i18n-labels";

export function creatorExportSuccessCopy(result: CreatorExportResult, t: LeadsT) {
  return {
    title: result.reused_existing
      ? t("transfer.export.reused")
      : t("transfer.export.success"),
    description: t("transfer.export.summary", {
      fileName: result.file_name,
      count: result.creator_count,
    }),
  };
}

export function creatorExportButtonAccessibility(exporting: boolean, t: LeadsT) {
  return {
    ariaLabel: exporting ? t("transfer.export.ariaPending") : t("transfer.export.ariaIdle"),
    ariaBusy: exporting,
  };
}
