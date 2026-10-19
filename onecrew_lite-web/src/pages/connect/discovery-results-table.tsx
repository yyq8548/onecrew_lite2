import { createContext, useContext, useMemo } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { ExternalLink } from "lucide-react";
import { useTranslation } from "react-i18next";

import { DataTable } from "@/components/data-table";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { DiscoveryHit } from "@/lib/opencli-discovery";
import { platformDisplayName } from "@/lib/platform-display";
import { SavedReviewMaterial } from "./discovery-review";
import { hasNativeScoring } from "./discovery-capabilities";

type ReviewActions = {
  verifiedSourceUrls: ReadonlySet<string>;
  canVerify: boolean;
  busy: boolean;
  onVerify: (hitId: string) => void;
  reviewContext: { campaignId: string; revision: number };
  onReviewSaved?: (campaignId: string) => Promise<void>;
};
const ReviewActionsContext = createContext<ReviewActions | null>(null);

// Stable cell type preserves an open review drawer during checkpoint updates.
function ReviewActionsCell({ row }: { row: { original: DiscoveryHit } }) {
  const actions = useContext(ReviewActionsContext);
  const { t } = useTranslation("shell");
  if (!actions) return null;
  const hit = row.original;
  return <div className="flex flex-wrap gap-2">
    {hit.platform === "youtube" && actions.canVerify &&
      <Button size="sm" variant="outline" disabled={actions.busy || actions.verifiedSourceUrls.has(hit.source_url)}
        onClick={() => actions.onVerify(hit.id)}>{t("opencliPilot.verifyHit")}</Button>}
    {hasNativeScoring(hit.platform) &&
      <SavedReviewMaterial title={hit.title} request={{ ...actions.reviewContext, hitId: hit.id, profileRef: hit.profile_ref }} onReviewSaved={actions.onReviewSaved} />}
  </div>;
}

/** Persisted Campaign hits stay leads; a row never implies a saved Creator. */
export function DiscoveryResultsTable({
  hits, verifiedSourceUrls, canVerify, busy, onVerify, reviewContext, onReviewSaved,
}: {
  hits: DiscoveryHit[];
  verifiedSourceUrls: ReadonlySet<string>;
  canVerify: boolean;
  busy: boolean;
  onVerify: (hitId: string) => void;
  reviewContext: { campaignId: string; revision: number };
  onReviewSaved?: (campaignId: string) => Promise<void>;
}) {
  const { t } = useTranslation("shell");
  const platforms = useMemo(() => [...new Set(hits.map(hit => hit.platform))].sort(), [hits]);
  const columns = useMemo<ColumnDef<DiscoveryHit, unknown>[]>(() => [
    { accessorKey: "platform", header: t("opencliPilot.tablePlatform"), cell: ({ row }) => platformDisplayName(row.original.platform) },
    {
      accessorKey: "author_label", header: t("opencliPilot.tableAuthor"),
      cell: ({ row }) => row.original.author_label || t("opencliPilot.tableUnknownAuthor"),
    },
    {
      id: "result", accessorFn: hit => `${hit.title} ${hit.author_label}`, header: t("opencliPilot.tableResult"),
      cell: ({ row }) => <a className="inline-flex max-w-[28rem] items-center gap-1 text-primary underline" href={row.original.source_url} target="_blank" rel="noreferrer" title={row.original.title}>
        <span className="truncate">{row.original.title}</span><ExternalLink className="size-3 shrink-0" aria-hidden />
      </a>,
    },
    { accessorKey: "search_keyword", header: t("opencliPilot.tableKeyword") },
    {
      id: "status", header: t("opencliPilot.tableStatus"),
      cell: ({ row }) => verifiedSourceUrls.has(row.original.source_url)
        ? <Badge variant="info">{t("opencliPilot.tableCandidate")}</Badge>
        : <Badge variant="muted">{t("opencliPilot.tableUnverified")}</Badge>,
    },
    {
      id: "actions", header: t("opencliPilot.tableAction"),
      cell: ReviewActionsCell,
    },
  ], [t, verifiedSourceUrls]);

  return <ReviewActionsContext.Provider value={{ verifiedSourceUrls, canVerify, busy, onVerify, reviewContext, onReviewSaved }}><DataTable
    columns={columns}
    data={hits}
    getRowId={hit => hit.id}
    pagination={false}
    emptyText={t("opencliPilot.tableEmpty")}
    toolbar={{
      searchKey: "result",
      searchPlaceholder: t("opencliPilot.tableSearch"),
      filters: [{ columnId: "platform", title: t("opencliPilot.tablePlatform"), options: platforms.map(platform => ({ label: platformDisplayName(platform), value: platform })) }],
    }}
  /></ReviewActionsContext.Provider>;
}
