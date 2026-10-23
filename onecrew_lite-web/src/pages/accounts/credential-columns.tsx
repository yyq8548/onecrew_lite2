import type { ReactNode } from "react";
import { type ColumnDef } from "@tanstack/react-table";
import type { TFunction } from "i18next";
import { AlertCircle, BadgeCheck, KeyRound, Loader2, Pencil, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { ServiceCredential } from "@/types";

import { YoutubeKeyDialog } from "./youtube-key-dialog";

type AccountsT = TFunction<"accounts">;

/** A credential plus the busy state its row controls read. Busy travels in the
 *  row rather than in the columns factory: rebuilding the columns array gives
 *  every cell a new component type and remounts it, edit dialog included. */
export type CredentialRow = {
  credential: ServiceCredential;
  /** This row's own test or delete call is in flight. */
  busy: boolean;
  /** Some row's call is in flight; delete and edit wait for it. */
  anyBusy: boolean;
};

export function credentialRows(
  credentials: readonly ServiceCredential[],
  busyId: string | null,
): CredentialRow[] {
  return credentials.map((credential) => ({
    credential,
    busy: busyId === credential.id,
    anyBusy: busyId !== null,
  }));
}

type CredentialColumnOptions = {
  t: AccountsT;
  /** Resolved UI language for the "last used" timestamp. */
  locale: string | undefined;
  onTest: (credential: ServiceCredential) => void;
  onDelete: (credential: ServiceCredential) => void;
};

function statusColumn(t: AccountsT): ColumnDef<CredentialRow> {
  return {
    id: "status",
    header: t("credentials.columns.status"),
    size: 120,
    minSize: 120,
    enableSorting: false,
    cell: ({ row }) => {
      const { credential } = row.original;
      const cooling = Boolean(credential.cooldown_until && credential.cooldown_until * 1000 > Date.now());
      const failed = credential.disabled || cooling || Boolean(credential.last_error);
      return (
        <Badge variant={credential.disabled ? "destructive" : "secondary"} className="w-fit">
          {failed ? <AlertCircle className="mr-1 size-3" /> : <BadgeCheck className="mr-1 size-3" />}
          {credential.disabled
            ? t("credentials.statuses.unavailable")
            : cooling
              ? t("credentials.statuses.coolingDown")
              : credential.last_error
                ? t("credentials.statuses.needsAttention")
                : t("credentials.statuses.available")}
        </Badge>
      );
    },
  };
}

function recentStatusColumn(t: AccountsT, locale: string | undefined): ColumnDef<CredentialRow> {
  return {
    id: "recentStatus",
    header: t("credentials.columns.recentStatus"),
    size: 220,
    minSize: 180,
    // The widest free-form column absorbs the leftover width, as the
    // `1.4fr` track it replaces did.
    meta: { grow: true },
    enableSorting: false,
    cell: ({ row }) => {
      const { credential } = row.original;
      return (
        <div className="min-w-0 flex-1 whitespace-normal text-xs text-muted-foreground">
          {credential.last_error ? (
            <span className="line-clamp-2 text-destructive">{credential.last_error}</span>
          ) : credential.last_used_at ? (
            t("credentials.lastCalled", {
              date: new Date(credential.last_used_at * 1000).toLocaleString(locale),
            })
          ) : (
            t("credentials.neverCalled")
          )}
        </div>
      );
    },
  };
}

/** Test + delete, the two controls both credential tables share. The labels
 *  come from the hook so the cell re-translates when the language changes. */
function CredentialActions({
  row,
  testLabelKey,
  onTest,
  onDelete,
  children,
}: {
  row: CredentialRow;
  testLabelKey: "credentials.actions.testApiKey";
  onTest: (credential: ServiceCredential) => void;
  onDelete: (credential: ServiceCredential) => void;
  children?: ReactNode;
}) {
  const { t } = useTranslation("accounts");
  const testLabel = t(testLabelKey);
  const deleteLabel = t("credentials.actions.delete");
  return (
    <div className="flex w-full justify-end gap-1">
      <Button
        variant="ghost"
        size="icon"
        title={testLabel}
        aria-label={testLabel}
        disabled={row.busy}
        onClick={() => onTest(row.credential)}
      >
        {row.busy ? <Loader2 className="size-4 animate-spin" /> : <BadgeCheck className="size-4" />}
      </Button>
      <Button
        variant="ghost"
        size="icon"
        title={deleteLabel}
        aria-label={deleteLabel}
        disabled={row.anyBusy}
        onClick={() => onDelete(row.credential)}
      >
        <Trash2 className="size-4" />
      </Button>
      {children}
    </div>
  );
}

function YoutubeEditAction({
  row,
  onChanged,
}: {
  row: CredentialRow;
  onChanged: () => void | Promise<void>;
}) {
  const { t } = useTranslation("accounts");
  const editLabel = t("credentials.actions.edit");
  return (
    <YoutubeKeyDialog
      credential={row.credential}
      onChanged={onChanged}
      trigger={(
        <Button
          variant="ghost"
          size="icon"
          title={editLabel}
          aria-label={editLabel}
          disabled={row.anyBusy}
        >
          <Pencil className="size-4" />
        </Button>
      )}
    />
  );
}

export function youtubeCredentialColumns({
  t,
  locale,
  onTest,
  onDelete,
  onChanged,
}: CredentialColumnOptions & { onChanged: () => void | Promise<void> }): ColumnDef<CredentialRow>[] {
  return [
    {
      id: "credential",
      header: t("credentials.columns.credential"),
      size: 200,
      minSize: 160,
      enableSorting: false,
      cell: ({ row }) => (
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 text-sm font-medium">
            <KeyRound className="size-4 shrink-0 text-muted-foreground" />
            <span className="truncate">{row.original.credential.label}</span>
          </div>
          <div className="mt-0.5 text-xs text-muted-foreground">
            {t("credentials.localOnlyMasked")}
          </div>
        </div>
      ),
    },
    {
      id: "projectLabel",
      header: t("credentials.columns.projectLabel"),
      size: 160,
      minSize: 140,
      enableSorting: false,
      cell: ({ row }) => (
        <span className="min-w-0 flex-1 truncate text-sm text-muted-foreground">
          {typeof row.original.credential.metadata.project === "string"
            ? row.original.credential.metadata.project
            : "—"}
        </span>
      ),
    },
    statusColumn(t),
    recentStatusColumn(t, locale),
    {
      id: "actions",
      header: () => <span className="w-full text-right">{t("credentials.columns.actions")}</span>,
      // Three icon controls plus the cell padding: a pinned column never
      // shrinks, so this is the width that stays reachable at any viewport.
      size: 132,
      minSize: 132,
      enableHiding: false,
      enableSorting: false,
      cell: ({ row }) => (
        <CredentialActions
          row={row.original}
          testLabelKey="credentials.actions.testApiKey"
          onTest={onTest}
          onDelete={onDelete}
        >
          <YoutubeEditAction row={row.original} onChanged={onChanged} />
        </CredentialActions>
      ),
    },
  ];
}
