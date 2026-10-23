import type { ToolkitDirectory, ToolkitDirectoryToolkit } from "@/lib/toolkit-directory";
import type { Account, Platform, ServiceCredential } from "@/types";
import type { TFunction } from "i18next";

export type AccountToolkitKey = Extract<
  ToolkitDirectoryToolkit,
  "x" | "instagram" | "telegram" | "youtube" | "email"
>;

export type AccountToolkitSpec = {
  toolkit: AccountToolkitKey;
  displayName: string;
  descriptionKey: `catalog.specs.${AccountToolkitKey}.description`;
  manageTarget: Platform | "youtube" | "gmail";
  manageLabelKey: `catalog.specs.${AccountToolkitKey}.manage`;
};

export const ACCOUNT_TOOLKIT_SPECS: readonly AccountToolkitSpec[] = [
  {
    toolkit: "x",
    displayName: "X",
    descriptionKey: "catalog.specs.x.description",
    manageTarget: "x",
    manageLabelKey: "catalog.specs.x.manage",
  },
  {
    toolkit: "instagram",
    displayName: "Instagram",
    descriptionKey: "catalog.specs.instagram.description",
    manageTarget: "instagram",
    manageLabelKey: "catalog.specs.instagram.manage",
  },
  {
    toolkit: "telegram",
    displayName: "Telegram",
    descriptionKey: "catalog.specs.telegram.description",
    manageTarget: "telegram",
    manageLabelKey: "catalog.specs.telegram.manage",
  },
  {
    toolkit: "youtube",
    displayName: "YouTube",
    descriptionKey: "catalog.specs.youtube.description",
    manageTarget: "youtube",
    manageLabelKey: "catalog.specs.youtube.manage",
  },
  {
    toolkit: "email",
    displayName: "Gmail",
    descriptionKey: "catalog.specs.email.description",
    manageTarget: "gmail",
    manageLabelKey: "catalog.specs.email.manage",
  },
] as const;

export type ToolkitStatusFilter = "all" | "connected" | "attention";

export function toolkitStatusCounts(status: ToolkitConnectionStatus): { connected: number; attention: number; checked: boolean } {
  switch (status.kind) {
    case "connected": return { connected: status.count, attention: 0, checked: true };
    case "attention": return { connected: status.connectedCount, attention: status.attentionCount + status.disabledCount, checked: true };
    case "disabled": return { connected: 0, attention: status.count, checked: true };
    case "error": return { connected: 0, attention: 1, checked: false };
    case "loading": return { connected: 0, attention: 0, checked: false };
    case "not_connected": return { connected: 0, attention: 0, checked: true };
  }
}

export function matchesToolkitStatus(status: ToolkitConnectionStatus, filter: ToolkitStatusFilter): boolean {
  if (filter === "all") return true;
  const counts = toolkitStatusCounts(status);
  return filter === "connected" ? counts.connected > 0 : counts.attention > 0;
}

export function toolkitOverviewCounts(statuses: Record<AccountToolkitKey, ToolkitConnectionStatus>) {
  const entries = ACCOUNT_TOOLKIT_SPECS.map((spec) => toolkitStatusCounts(statuses[spec.toolkit]));
  return { total: entries.length, connected: entries.filter((value) => value.connected > 0).length, attention: entries.filter((value) => value.attention > 0).length };
}

export type ToolkitConnectionStatus =
  | { kind: "loading" }
  | { kind: "error" }
  | { kind: "not_connected" }
  | { kind: "connected"; count: number }
  | {
      kind: "attention";
      connectedCount: number;
      attentionCount: number;
      disabledCount: number;
    }
  | { kind: "disabled"; count: number };

export type ToolkitConnectionStatusView = {
  label: string;
  detail: string;
  badge: "success" | "warning" | "danger" | "muted";
};

type ReadState<T> = {
  records: T[] | undefined;
  loading: boolean;
  error: boolean;
};

export function deriveOperatorToolkitStatus(
  state: ReadState<Account> & { platform: Platform },
): ToolkitConnectionStatus {
  if (state.error) return { kind: "error" };
  if (state.records === undefined && state.loading) return { kind: "loading" };
  const count = (state.records ?? []).filter((account) => account.platform === state.platform).length;
  return count > 0 ? { kind: "connected", count } : { kind: "not_connected" };
}

export function deriveServiceToolkitStatus(
  state: ReadState<ServiceCredential> & {
    provider: ServiceCredential["provider"];
    now?: number;
  },
): ToolkitConnectionStatus {
  if (state.error) return { kind: "error" };
  if (state.records === undefined && state.loading) return { kind: "loading" };

  const now = state.now ?? Date.now();
  const credentials = (state.records ?? []).filter(
    (credential) => credential.provider === state.provider,
  );
  if (credentials.length === 0) return { kind: "not_connected" };

  let connectedCount = 0;
  let attentionCount = 0;
  let disabledCount = 0;
  for (const credential of credentials) {
    if (credential.disabled) {
      disabledCount += 1;
    } else if (
      Boolean(credential.last_error)
      || Boolean(credential.cooldown_until && credential.cooldown_until * 1000 > now)
    ) {
      attentionCount += 1;
    } else {
      connectedCount += 1;
    }
  }

  if (connectedCount > 0 && attentionCount === 0 && disabledCount === 0) {
    return { kind: "connected", count: connectedCount };
  }
  if (connectedCount === 0 && attentionCount === 0) {
    return { kind: "disabled", count: disabledCount };
  }
  return { kind: "attention", connectedCount, attentionCount, disabledCount };
}

export function toolkitConnectionStatusView(
  status: ToolkitConnectionStatus,
  t: TFunction<"accounts">,
): ToolkitConnectionStatusView {
  switch (status.kind) {
    case "loading":
      return {
        label: t("catalog.status.loading.label"),
        detail: t("catalog.status.loading.detail"),
        badge: "muted",
      };
    case "error":
      return {
        label: t("catalog.status.error.label"),
        detail: t("catalog.status.error.detail"),
        badge: "danger",
      };
    case "not_connected":
      return {
        label: t("catalog.status.notConnected.label"),
        detail: t("catalog.status.notConnected.detail"),
        badge: "muted",
      };
    case "connected":
      return {
        label: t("catalog.status.connected.label"),
        detail: t("catalog.status.connected.detail", { count: status.count }),
        badge: "success",
      };
    case "disabled":
      return {
        label: t("catalog.status.disabled.label"),
        detail: t("catalog.status.disabled.detail", { count: status.count }),
        badge: "danger",
      };
    case "attention": {
      return {
        label: status.connectedCount > 0
          ? t("catalog.status.attention.partialLabel")
          : t("catalog.status.attention.label"),
        detail: t("catalog.status.attention.detail", status),
        badge: "warning",
      };
    }
  }
}

function normalized(value: string) {
  return value.trim().toLocaleLowerCase();
}

export function filterToolkitOperations(
  directory: ToolkitDirectory,
  search: string,
): ToolkitDirectory["operations"] {
  const query = normalized(search);
  if (!query) return directory.operations;
  return directory.operations.filter((operation) =>
    normalized(`${operation.id} ${operation.title} ${operation.summary}`).includes(query)
  );
}

/** A card matches only what the card itself stands for: the toolkit id, its
 *  display name, the localized copy the card renders, and the guide's own
 *  prose. Folding every operation id/title/summary in here made a short query
 *  ("one", from "Read one live X profile…") match every platform. Operation
 *  text is searched where operations are shown — `filterToolkitOperations`. */
export function matchesToolkitCatalogSearch(
  spec: AccountToolkitSpec,
  directory: ToolkitDirectory | undefined,
  search: string,
  localizedCopy: readonly string[] = [],
) {
  const query = normalized(search);
  if (!query) return true;
  const guideText = directory
    ? [
        directory.title,
        directory.summary,
        directory.purpose,
        directory.capability_categories.join(" "),
      ].join(" ")
    : "";
  return normalized(
    `${spec.toolkit} ${spec.displayName} ${localizedCopy.join(" ")} ${guideText}`,
  ).includes(query);
}

export function directoryByToolkit(
  directories: readonly ToolkitDirectory[],
): Partial<Record<AccountToolkitKey, ToolkitDirectory>> {
  const result: Partial<Record<AccountToolkitKey, ToolkitDirectory>> = {};
  for (const directory of directories) {
    if (ACCOUNT_TOOLKIT_SPECS.some((spec) => spec.toolkit === directory.toolkit)) {
      result[directory.toolkit as AccountToolkitKey] = directory;
    }
  }
  return result;
}
