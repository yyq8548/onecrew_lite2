import { useEffect, useMemo, useRef, useState, type Ref } from "react";
import {
  Loader2,
  Mail,
  Search,
  ArrowUpRight,
  Wrench,
} from "lucide-react";
import { useTranslation } from "react-i18next";

import { YoutubeIcon } from "@/components/platform";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Sheet,
  SheetClose,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { useCanvasPortal } from "@/lib/canvas-portal";
import type { ToolkitDirectory } from "@/lib/toolkit-directory";
import { cn } from "@/lib/utils";

import { PlatformTile } from "./shared";
import {
  ACCOUNT_TOOLKIT_SPECS,
  directoryByToolkit,
  filterToolkitOperations,
  matchesToolkitCatalogSearch,
  matchesToolkitStatus,
  toolkitOverviewCounts,
  toolkitStatusCounts,
  toolkitConnectionStatusView,
  type AccountToolkitKey,
  type AccountToolkitSpec,
  type ToolkitConnectionStatus,
  type ToolkitStatusFilter,
} from "./toolkit-catalog-model";

export const TOOLKIT_DETAIL_SHEET_CLASS =
  "flex w-[min(560px,calc(100%-1rem))] max-w-[calc(100%-1rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[560px]";
export const TOOLKIT_CARD_FOOTER_CLASS =
  "mt-auto justify-between gap-3 border-t border-border/60 bg-muted/10 px-4 py-2 [.border-t]:pt-2";

function ToolkitIcon({ toolkit, className }: { toolkit: AccountToolkitKey; className?: string }) {
  if (toolkit === "x" || toolkit === "instagram" || toolkit === "telegram") {
    return <PlatformTile platform={toolkit} className={className} />;
  }
  if (toolkit === "youtube") {
    return (
      <span className={cn("grid place-items-center rounded-lg border bg-card text-red-600 dark:text-red-500", className)}>
        <YoutubeIcon className="size-5" />
      </span>
    );
  }
  return (
    <span className={cn("grid place-items-center rounded-lg border bg-card text-blue-600 dark:text-blue-400", className)}>
      <Mail className="size-5" />
    </span>
  );
}

export function ToolkitCatalogCard({
  spec,
  directory,
  directoryLoading,
  directoryError,
  status,
  directoryButtonRef,
  onOpenDirectory,
  onManage,
}: {
  spec: AccountToolkitSpec;
  directory: ToolkitDirectory | undefined;
  directoryLoading: boolean;
  directoryError: boolean;
  status: ToolkitConnectionStatus;
  directoryButtonRef?: Ref<HTMLButtonElement>;
  onOpenDirectory: () => void;
  onManage: () => void;
}) {
  const { t } = useTranslation("accounts");
  const statusView = toolkitConnectionStatusView(status, t);
  const counts = toolkitStatusCounts(status);
  const connectionKind = spec.toolkit === "youtube" || spec.toolkit === "email" ? "credential" : "account";
  return (
    <Card className="group h-full gap-0 overflow-hidden rounded-xl border-border/80 py-0 shadow-none transition-colors hover:border-foreground/25">
      <CardHeader className="block min-w-0 flex-1 px-4 pb-3 pt-4">
        <div className="flex items-center gap-3">
          <ToolkitIcon toolkit={spec.toolkit} className="size-10 shrink-0" />
          <div className="min-w-0 flex-1"><CardTitle className="truncate text-sm">{spec.displayName}</CardTitle><p className="mt-1 text-xs text-muted-foreground">{t(`catalog.overview.kind.${connectionKind}`)}</p></div>
          <Badge variant={statusView.badge} className="shrink-0 rounded-md">{statusView.label}</Badge>
        </div>
        <CardDescription className="mt-3 line-clamp-2 text-xs leading-5">
          {t(spec.descriptionKey)}
        </CardDescription>
        <div className="mt-3 flex flex-wrap items-baseline gap-2 text-xs">
          {counts.checked ? <><span className="text-lg font-semibold tabular-nums leading-none text-foreground">{counts.connected}</span><span className="text-muted-foreground">{t(`catalog.overview.unit.${connectionKind}`, { count: counts.connected })}</span></> : <span className="text-muted-foreground">{t("catalog.overview.countUnavailable")}</span>}
          {counts.attention > 0 && <span className="ml-auto text-muted-foreground">{t("catalog.overview.cardAttention", { count: counts.attention })}</span>}
        </div>
      </CardHeader>
      <CardFooter className={TOOLKIT_CARD_FOOTER_CLASS}>
        {spec.toolkit !== "email" && <Button
          ref={directoryButtonRef}
          type="button"
          variant="ghost"
          size="sm"
          className="h-8 min-w-0 px-2 text-muted-foreground"
          aria-label={t("catalog.viewDirectoryAria", { platform: spec.displayName })}
          onClick={onOpenDirectory}
        >
          {directoryLoading && !directory ? (
            <Loader2 className="size-3.5 shrink-0 animate-spin" />
          ) : (
            <Wrench className="size-3.5 shrink-0" />
          )}
          <span className="truncate">
            {directory
              ? t("catalog.operationCount", { count: directory.operations.length })
              : directoryLoading
                ? t("catalog.loadingOperations")
                : directoryError
                  ? t("catalog.directoryReadFailed")
                  : t("catalog.directoryUnavailable")}
          </span>
        </Button>}
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-8 shrink-0 gap-1.5 px-2 text-foreground hover:bg-secondary"
          onClick={onManage}
        >
          {t(spec.manageLabelKey)}
          <ArrowUpRight className="size-3.5 text-muted-foreground" />
        </Button>
      </CardFooter>
    </Card>
  );
}

function ToolkitDirectorySheet({
  selected,
  directory,
  directoryLoading,
  directoryError,
  status,
  open,
  onOpenChange,
  onRetry,
  onRestoreFocus,
}: {
  selected: AccountToolkitSpec | null;
  directory: ToolkitDirectory | undefined;
  directoryLoading: boolean;
  directoryError: boolean;
  status: ToolkitConnectionStatus | undefined;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onRetry: () => void;
  onRestoreFocus: () => void;
}) {
  const { t } = useTranslation("accounts");
  const canvas = useCanvasPortal();
  const searchRef = useRef<HTMLInputElement>(null);
  const [search, setSearch] = useState("");
  const operations = directory ? filterToolkitOperations(directory, search) : [];
  const statusView = status ? toolkitConnectionStatusView(status, t) : null;

  useEffect(() => setSearch(""), [selected?.toolkit]);

  return (
    <Sheet
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) setSearch("");
        onOpenChange(nextOpen);
      }}
    >
      <SheetContent
        container={canvas}
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          searchRef.current?.focus();
        }}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          onRestoreFocus();
        }}
        className={TOOLKIT_DETAIL_SHEET_CLASS}
      >
        {selected && (
          <>
            <SheetHeader className="shrink-0 border-b px-5 py-4 pr-12">
              <div className="flex items-start gap-3">
                <ToolkitIcon toolkit={selected.toolkit} className="size-10 shrink-0" />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <SheetTitle>{t("catalog.sheetTitle", { platform: selected.displayName })}</SheetTitle>
                    <Badge variant="outline" className="font-mono">{selected.toolkit}</Badge>
                    {statusView && <Badge variant={statusView.badge}>{statusView.label}</Badge>}
                  </div>
                  <SheetDescription className="mt-1.5 leading-5">
                    {directory?.summary ?? t(selected.descriptionKey)}
                  </SheetDescription>
                </div>
              </div>
            </SheetHeader>

            <div className="shrink-0 space-y-3 border-b px-5 py-4">
              <div>
                <div className="text-xs font-medium text-muted-foreground">
                  {t("catalog.about", { platform: selected.displayName })}
                </div>
                <p className="mt-1 text-sm leading-6">
                  {directory?.purpose ?? t(selected.descriptionKey)}
                </p>
              </div>
              {statusView && (
                <div className="rounded-lg bg-muted/40 px-3 py-2.5 text-xs leading-5 text-muted-foreground">
                  {statusView.detail}
                </div>
              )}
              {directory && directory.capability_categories.length > 0 && (
                <div className="flex flex-wrap gap-1.5">
                  {directory.capability_categories.map((category) => (
                    <Badge key={category} variant="secondary">{category}</Badge>
                  ))}
                </div>
              )}
            </div>

            <div className="flex min-h-0 flex-1 flex-col">
              <div className="relative shrink-0 px-5 py-3">
                <Search className="pointer-events-none absolute left-8 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  ref={searchRef}
                  value={search}
                  onChange={(event) => setSearch(event.target.value)}
                  placeholder={t("catalog.searchPlaceholder")}
                  aria-label={t("catalog.searchAria", { platform: selected.displayName })}
                  className="pl-9"
                />
              </div>

              {directoryLoading && !directory ? (
                <div className="grid min-h-40 flex-1 place-items-center text-sm text-muted-foreground">
                  <span className="flex items-center gap-2">
                    <Loader2 className="size-4 animate-spin" />{t("catalog.loadingDirectory")}
                  </span>
                </div>
              ) : directoryError && !directory ? (
                <div className="grid min-h-40 flex-1 place-items-center px-5 text-center">
                  <div className="space-y-3">
                    <p className="text-sm text-destructive">{t("catalog.directoryFailed")}</p>
                    <Button type="button" size="sm" variant="outline" onClick={onRetry}>
                      {t("catalog.retryDirectory")}
                    </Button>
                  </div>
                </div>
              ) : directory ? (
                <ScrollArea className="min-h-0 flex-1 px-5">
                  <div className="space-y-2 pb-5">
                    <div className="flex items-center justify-between text-xs text-muted-foreground">
                      <span>
                        {t("catalog.operationProgress", {
                          visible: operations.length,
                          total: directory.operations.length,
                        })}
                      </span>
                      <span>{t("catalog.readOnlyNotice")}</span>
                    </div>
                    {operations.map((operation) => (
                      <article key={operation.id} className="rounded-lg border px-3.5 py-3">
                        <h3 className="text-sm font-medium">{operation.title}</h3>
                        <code className="mt-1 block break-all text-[11px] text-muted-foreground">{operation.id}</code>
                        <p className="mt-2 text-xs leading-5 text-muted-foreground">{operation.summary}</p>
                      </article>
                    ))}
                    {operations.length === 0 && (
                      <p className="rounded-lg border border-dashed px-4 py-8 text-center text-sm text-muted-foreground">
                        {t("catalog.noMatchingOperation", { search: search.trim() })}
                      </p>
                    )}
                  </div>
                </ScrollArea>
              ) : (
                <p className="px-5 py-8 text-center text-sm text-muted-foreground">
                  {t("catalog.noDirectory")}
                </p>
              )}
            </div>

            <SheetFooter className="shrink-0 border-t px-5 py-3 sm:flex-row sm:justify-end">
              <SheetClose asChild>
                <Button type="button" variant="outline">{t("common.actions.close")}</Button>
              </SheetClose>
            </SheetFooter>
          </>
        )}
      </SheetContent>
    </Sheet>
  );
}

export function ToolkitCatalog({
  directories,
  directoryLoading,
  directoryError,
  statuses,
  search,
  onSearchChange,
  onManage,
  onRetryDirectory,
}: {
  directories: readonly ToolkitDirectory[];
  directoryLoading: boolean;
  directoryError: boolean;
  statuses: Record<AccountToolkitKey, ToolkitConnectionStatus>;
  search: string;
  onSearchChange?: (value: string) => void;
  onManage: (target: AccountToolkitSpec["manageTarget"]) => void;
  onRetryDirectory: () => void;
}) {
  const { t } = useTranslation("accounts");
  const [selectedToolkit, setSelectedToolkit] = useState<AccountToolkitKey | null>(null);
  const [filter, setFilter] = useState<ToolkitStatusFilter>("all");
  const overview = toolkitOverviewCounts(statuses);
  const lastSelectedToolkit = useRef<AccountToolkitKey | null>(null);
  const directoryButtons = useRef(new Map<AccountToolkitKey, HTMLButtonElement>());
  const byToolkit = useMemo(() => directoryByToolkit(directories), [directories]);
  const visible = ACCOUNT_TOOLKIT_SPECS.filter((spec) =>
    matchesToolkitStatus(statuses[spec.toolkit], filter) && matchesToolkitCatalogSearch(spec, byToolkit[spec.toolkit], search, [
      t(spec.descriptionKey),
      t(spec.manageLabelKey),
    ])
  );
  const selected = ACCOUNT_TOOLKIT_SPECS.find((spec) => spec.toolkit === selectedToolkit) ?? null;

  return (
    <div className="@container space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-x-8 gap-y-4 border-b pb-5">
        <div className="flex items-start gap-3"><span className="mt-1 h-11 w-1 rounded-full bg-[var(--brand-accent)]" /><div><h2 className="text-xl font-semibold tracking-tight">{t("catalog.overview.title")}</h2><p className="mt-1 text-xs leading-5 text-muted-foreground">{t("catalog.overview.description")}</p></div></div>
        <dl className="flex items-center gap-6 sm:gap-8">{(["total", "connected", "attention"] as const).map((key) => <div key={key} className="min-w-12"><dt className="text-xs text-muted-foreground">{t(`catalog.overview.${key}`)}</dt><dd className={cn("mt-1 text-2xl font-semibold tabular-nums leading-none", key === "attention" && overview[key] > 0 && "text-[var(--brand-ink)]")}>{overview[key]}</dd></div>)}</dl>
      </div>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-1 rounded-lg bg-muted/50 p-1" aria-label={t("catalog.overview.filterAria")}>{(["all", "connected", "attention"] as const).map((value) => <Button key={value} variant="ghost" size="sm" aria-pressed={filter === value} className={cn("h-8 gap-2 px-3 text-xs text-muted-foreground", filter === value && "bg-secondary text-[var(--brand-ink)] shadow-sm hover:bg-secondary")} onClick={() => setFilter(value)}>{t(`catalog.overview.filter.${value}`)}<span className="text-[11px] tabular-nums opacity-70">{overview[value === "all" ? "total" : value]}</span></Button>)}</div>
        {onSearchChange && <div className="relative min-w-48 flex-1 sm:max-w-72"><Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" /><Input value={search} onChange={(event) => onSearchChange(event.target.value)} aria-label={t("searchPlaceholder")} placeholder={t("searchPlaceholder")} className="h-9 bg-card pl-9" /></div>}
      </div>
      {visible.length === 0 ? (
        <p className="rounded-lg border border-dashed p-6 text-center text-sm text-muted-foreground">
          {search.trim()
            ? t("catalog.noMatchingPlatform", { search: search.trim() })
            : t(filter === "attention" ? "catalog.noAttentionPlatform" : "catalog.noConnectedPlatform")}
        </p>
      ) : (
        <ul
          className="grid gap-3 @min-[650px]:grid-cols-2 @min-[960px]:grid-cols-3"
          aria-label={t("catalog.listAria")}
        >
          {visible.map((spec) => (
            <li key={spec.toolkit} className="min-w-0">
              <ToolkitCatalogCard
                spec={spec}
                directory={byToolkit[spec.toolkit]}
                directoryLoading={directoryLoading}
                directoryError={directoryError}
                status={statuses[spec.toolkit]}
                directoryButtonRef={(node) => {
                  if (node) directoryButtons.current.set(spec.toolkit, node);
                  else directoryButtons.current.delete(spec.toolkit);
                }}
                onOpenDirectory={() => {
                  lastSelectedToolkit.current = spec.toolkit;
                  setSelectedToolkit(spec.toolkit);
                }}
                onManage={() => onManage(spec.manageTarget)}
              />
            </li>
          ))}
        </ul>
      )}
      <ToolkitDirectorySheet
        selected={selected}
        directory={selected ? byToolkit[selected.toolkit] : undefined}
        directoryLoading={directoryLoading}
        directoryError={directoryError}
        status={selected ? statuses[selected.toolkit] : undefined}
        open={selected !== null}
        onOpenChange={(open) => !open && setSelectedToolkit(null)}
        onRetry={onRetryDirectory}
        onRestoreFocus={() => {
          const toolkit = lastSelectedToolkit.current;
          if (toolkit) directoryButtons.current.get(toolkit)?.focus();
        }}
      />
    </div>
  );
}
