import { hasDefaultSyncConsent, type TableSyncStatus } from './table-sync';

export function tableSyncMessage(value: TableSyncStatus, table: boolean, zh: boolean): string {
  if (value.capabilities.consent?.status === 'revoked' || value.status === 'consent_revoked') {
    return zh ? '后台上传授权已撤回，后续同步已停止。本地表格仍保留。' : 'Cloud upload consent revoked. Future sync stopped; local tables are retained.';
  }
  if (!hasDefaultSyncConsent(value)) {
    return zh ? '等待确认当前同步协议：新结果尚未上传。本地表格仍保留，请阅读下方协议并确认上传授权。' : 'Awaiting current sync consent: new results have not uploaded. Local tables are retained. Read the policy below and confirm upload consent.';
  }
  if (!table) return zh ? '同步授权有效，结果表默认后台同步。各表上传结果请在搜索计划中查看。' : 'Sync consent is current. Result tables sync automatically; inspect upload results in each search plan.';
  if (value.error === 'table_sync_consent_required') return zh ? '同步协议已确认，等待后台重新检查并上传。' : 'Sync consent confirmed. Awaiting the next cloud check and upload.';
  const labels: Record<string, [string, string]> = {
    disabled: ['本表后台同步已关闭，本地表格仍保留。', 'Cloud sync is off for this table; local data is retained.'],
    waiting: ['等待后台同步，尚未确认本轮上传完成。', 'Waiting for cloud sync; this upload is not confirmed.'],
    synced: ['已同步：后台已确认接收本轮表格结果。', 'Synced: the cloud confirmed receipt of this table result.'],
    up_to_date: ['无新变化：与最近确认的后台版本一致。', 'No changes since the last confirmed cloud version.'],
    error: ['后台同步失败，本地表格仍保留。请查看错误代码并重新检查。', 'Cloud sync failed. Local tables are retained; inspect the error code and check again.'],
    needs_attention: ['后台同步连续失败，已暂停。请检查错误后重新开启本表同步。', 'Cloud sync paused after repeated failures. Inspect the error, then enable this table again.'],
  };
  if (['synced', 'up_to_date'].includes(value.status) && !value.version_id) return zh ? '后台接收记录不完整，无法确认上传完成。请重新检查。' : 'Cloud receipt is incomplete; upload completion is unconfirmed. Check again.';
  return labels[value.status]?.[zh ? 0 : 1] ?? (zh ? `同步状态无法识别：${value.status}。请重新检查。` : `Unrecognized sync status: ${value.status}. Check again.`);
}
