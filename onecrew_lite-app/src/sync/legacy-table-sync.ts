import { parseApplicationJson } from "../contracts/application-json.ts";
import { validateTableSyncPending } from "./table-sync-integrity.ts";
import { tableSyncKey, tableSyncStoreSchema, type TableSyncState } from "./table-sync-store.ts";

// 完整源文件由迁移事务备份；这里只按已验证 Access 用户分区，保留原上传及未知回执。
export async function partitionLegacyTableSync(bytes: Uint8Array, accessUserId: string): Promise<{ selected: TableSyncState; otherUserIds: readonly string[] }> {
  if (!accessUserId || bytes.byteLength > 512 * 1024 * 1024) throw new Error("migration.table_sync_source_invalid");
  const state = tableSyncStoreSchema.parse(parseApplicationJson(bytes));
  if (state.consent_operation !== null && state.consent_user === null) throw new Error("migration.table_sync_consent_owner_missing");
  const selected = tableSyncStoreSchema.parse({ ...state, tables: {}, consent_user: state.consent_user === accessUserId ? accessUserId : null, consent_operation: state.consent_user === accessUserId ? state.consent_operation : null });
  const others = new Set<string>();
  for (const [key, table] of Object.entries(state.tables)) {
    if (!table.user_id || key !== tableSyncKey(table.user_id, table.campaign_id)) throw new Error("migration.table_sync_owner_invalid");
    if (table.pending) await validateTableSyncPending(table.pending);
    if (table.user_id === accessUserId) selected.tables[key] = table;
    else others.add(table.user_id);
  }
  if (state.consent_user && state.consent_user !== accessUserId) others.add(state.consent_user);
  return { selected, otherUserIds: Object.freeze([...others].sort()) };
}
