/** Read-only service startup diagnostics. The webpage never initiates data
 * migration; plugin-only service startup follows the same initialization path. */
import { invoke } from "@/lib/local-runtime";
import { appI18n } from "@/i18n";
import zhCNShell from "@/i18n/locales/zh-CN/shell";
import { dataStartupStatus, type DataStartupStatus } from "@onecrew/runtime-contracts/status";

export type { DataStartupStatus };
type LocalDataFailureKind = NonNullable<DataStartupStatus["failure"]>["kind"];

export async function inspectLocalData(): Promise<DataStartupStatus> {
  const result = await invoke<unknown>("local_data_status");
  const parsed = dataStartupStatus.safeParse(result);
  if (!parsed.success) throw new Error("invalid_data_startup_status");
  return parsed.data;
}

const FAILURE_KEYS = {
  insufficient_space: "insufficientSpace", busy: "busy", backup_failed: "backupFailed",
  newer_version: "newerVersion", not_writable: "notWritable", initialization_failed: "initializationFailed", unsafe_data: "unsafeData",
} as const satisfies Record<LocalDataFailureKind, keyof typeof zhCNShell.localData.failure>;

export function failureMessage(kind: LocalDataFailureKind, storageUpgraded: boolean): string {
  const key = storageUpgraded ? "partial" : FAILURE_KEYS[kind];
  return appI18n.isInitialized ? appI18n.t(`localData.failure.${key}`, { ns: "shell" }) : zhCNShell.localData.failure[key];
}
