import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ExternalLink, Link2, Loader2, Mail, MoreHorizontal, Pencil, Plus, Search, Trash2 } from "lucide-react";

import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { PlatformIcon, RouteBrandIcon } from "@/components/platform";
import type {
  CreatorAccountView,
  CreatorContactRouteRecord,
  CreatorPlatform,
  LocatorKind,
  SourceKind,
} from "@/lib/creator";
import { openExternalUrl } from "@/lib/open-external";
import { contactChannelLabel, sourceKindLabel } from "@/pages/leads/i18n-labels";

function tiktokProfileUrl(value: string): string | null {
  try {
    const url = new URL(value);
    if ((url.protocol === "https:" || url.protocol === "http:") && !url.username && !url.password
      && (url.hostname === "tiktok.com" || url.hostname.endsWith(".tiktok.com"))) return value;
  } catch { /* A handle is not a URL. */ }
  return null;
}

function externalUrl(channel: string, value: string, locatorKind?: LocatorKind) {
  if (channel === "tiktok") {
    const url = tiktokProfileUrl(value);
    if (url) return url;
    const handle = value.replace(/^@/, "");
    if (locatorKind === "handle" && /^[A-Za-z0-9._]+$/.test(handle)) return `https://www.tiktok.com/@${handle}`;
    return null;
  }
  if (["website", "link_hub", "youtube"].includes(channel)) return value;
  return null;
}

export function SavedContactRoutes({
  routes,
  accounts = [],
  onEdit,
  onDelete,
  scrollable = true,
  readOnly = false,
}: {
  routes: CreatorContactRouteRecord[];
  accounts?: CreatorAccountView[];
  onEdit?: (route: CreatorContactRouteRecord) => void;
  onDelete?: (route: CreatorContactRouteRecord) => void;
  scrollable?: boolean;
  readOnly?: boolean;
}) {
  return (
    <RouteViewport
      scrollable={scrollable}
      rows={savedContactRouteRows(routes, accounts, readOnly, onEdit, onDelete)}
    />
  );
}

export type ContactRouteRow = {
  key: string;
  channel: string;
  locatorKind?: LocatorKind;
  value: string;
  sources: SourceKind[];
  sourceUrls: string[];
  collision: string;
  /** Exact-match context: whether this discovered route already exists on the
   * matched Creator, or can be attached to it right here. */
  attach?: { state: "present" | "ready" | "busy"; onAttach?: () => void };
  checked?: boolean;
  showCheckbox?: boolean;
  selectionLocked?: boolean;
  onCheckedChange?: (checked: boolean) => void;
  platform?: CreatorPlatform;
  onEdit?: () => void;
  onDelete?: () => void;
  allowExternalAction?: boolean;
};

export function savedContactRouteRows(
  routes: CreatorContactRouteRecord[],
  accounts: CreatorAccountView[],
  readOnly: boolean,
  onEdit?: (route: CreatorContactRouteRecord) => void,
  onDelete?: (route: CreatorContactRouteRecord) => void,
): ContactRouteRow[] {
  return [
    ...accounts.map((account) => ({
      key: `account:${account.platform}:${account.external_id}`,
      channel: account.platform,
      locatorKind: account.handle ? "handle" as const : undefined,
      value: account.handle ? `@${account.handle}` : account.external_id,
      sources: [account.origin],
      sourceUrls: account.profile_url ? [account.profile_url] : [],
      collision: "none",
      platform: account.platform,
      allowExternalAction: !readOnly,
    })),
    ...routes.map((record) => ({
      key: record.route.id,
      channel: record.locator.channel,
      locatorKind: record.locator.kind,
      value: record.locator.display_value,
      sources: record.observations.map((item) => item.source.kind),
      sourceUrls: record.observations.flatMap((item) => item.source.source_url ? [item.source.source_url] : []),
      collision: "none",
      onEdit: !readOnly && record.allowed_actions.includes("update") && onEdit
        ? () => onEdit(record)
        : undefined,
      onDelete: !readOnly && record.allowed_actions.includes("delete") && onDelete
        ? () => onDelete(record)
        : undefined,
      allowExternalAction: !readOnly,
    })),
  ];
}

export function routeExternalActionUrl(row: ContactRouteRow): string | null {
  if (row.allowExternalAction === false) return null;
  if (row.platform === "tiktok") {
    const profile = row.sourceUrls.map(tiktokProfileUrl).find((url) => url !== null);
    if (profile) return profile;
  }
  return externalUrl(row.channel, row.value, row.locatorKind) ?? row.sourceUrls[0] ?? null;
}

function RouteViewport({ rows, scrollable = true }: { rows: ContactRouteRow[]; scrollable?: boolean }) {
  const { t } = useTranslation("leads");
  const [search, setSearch] = useState("");
  const filtered = useMemo(() => {
    const query = search.trim().toLocaleLowerCase();
    if (!query) return rows;
    return rows.filter((row) => [row.channel, row.value, ...row.sources]
      .some((value) => String(value).toLocaleLowerCase().includes(query)));
  }, [rows, search]);

  const routeRows = (
    <div className="divide-y">
      {filtered.length ? filtered.map((row) => <RouteRow key={row.key} row={row} />) : (
        <div className="px-4 py-10 text-center text-sm text-muted-foreground">{t("contactRoutes.noMatches")}</div>
      )}
    </div>
  );

  return (
    <div className="overflow-hidden rounded-lg border">
      <div className="relative border-b p-2">
        <Search className="pointer-events-none absolute left-4 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          placeholder={t("contactRoutes.searchPlaceholder")}
          className="h-8 border-0 bg-transparent pl-8 shadow-none focus-visible:ring-0"
        />
      </div>
      {scrollable ? <ScrollArea className="h-[min(42vh,360px)]">{routeRows}</ScrollArea> : routeRows}
    </div>
  );
}

function RouteRow({ row }: { row: ContactRouteRow }) {
  const { t } = useTranslation("leads");
  const sources = [...new Set(row.sources)];
  const url = routeExternalActionUrl(row);
  return (
    <div className="flex min-w-0 items-center gap-3 px-3 py-3">
      {row.showCheckbox && (
        <Checkbox
          checked={row.checked}
          disabled={row.selectionLocked}
          aria-label={row.selectionLocked ? t("contactRoutes.requiredInitialAccount") : t("contactRoutes.select")}
          onCheckedChange={(value) => row.onCheckedChange?.(value === true)}
        />
      )}
      <div className="grid size-9 shrink-0 place-items-center rounded-full bg-muted">
        {row.platform
          ? <PlatformIcon platform={row.platform} className="size-4" />
          : row.channel === "email"
            ? <Mail className="size-4" />
            : <RouteBrandIcon url={externalUrl(row.channel, row.value, row.locatorKind) ?? row.sourceUrls[0]} className="size-4" />}
      </div>
      <div className="min-w-0 flex-1">
        {/* 详情面板是列表截断的唯一出口,它自己不能再截断:这里有竖向空间,
            长邮箱 / 长 URL 换行就好。wrap-anywhere 才断得开不含空格的串。 */}
        <div className="wrap-anywhere text-sm font-medium">{contactChannelLabel(t, row.channel)} · {row.value}</div>
        <div className="wrap-anywhere text-xs text-muted-foreground">
          {sources.map((source) => sourceKindLabel(t, source)).join(" · ") || t("sources.unknown")}
          {row.collision !== "none" ? ` · ${row.collision === "conflict" ? t("contactRoutes.collisionConflict") : t("contactRoutes.collisionPossible")}` : ""}
        </div>
      </div>
      {url && (
        <Button variant="ghost" size="icon" className="size-8 shrink-0" onClick={() => void openExternalUrl(url)}>
          <ExternalLink className="size-4" />
          <span className="sr-only">{t("contactRoutes.browserVerification")}</span>
        </Button>
      )}
      {row.attach?.state === "present" && (
        <span className="shrink-0 text-xs text-muted-foreground">{t("contactRoutes.attached")}</span>
      )}
      {row.attach?.state === "ready" && (
        <Button size="sm" variant="outline" className="shrink-0" onClick={row.attach.onAttach}>
          <Plus className="size-4" />{t("contactRoutes.attach")}
        </Button>
      )}
      {row.attach?.state === "busy" && (
        <Button size="sm" variant="outline" className="shrink-0" disabled>
          <Loader2 className="size-4 animate-spin" />{t("contactRoutes.attach")}
        </Button>
      )}
      {(row.onEdit || row.onDelete) && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon" className="size-8 shrink-0">
              <MoreHorizontal className="size-4" />
              <span className="sr-only">{t("contactRoutes.actions")}</span>
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {row.onEdit && (
              <DropdownMenuItem onSelect={row.onEdit}>
                <Pencil className="size-4" />{t("common.actions.edit")}
              </DropdownMenuItem>
            )}
            {row.onDelete && (
              <DropdownMenuItem variant="destructive" onSelect={row.onDelete}>
                <Trash2 className="size-4" />{t("common.actions.delete")}
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}
