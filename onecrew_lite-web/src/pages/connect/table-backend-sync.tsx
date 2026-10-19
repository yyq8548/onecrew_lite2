import { useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { readTableSyncStatus, configureTableSync, changeTableSyncConsent, hasDefaultSyncConsent, tableSyncQueryKey } from '@/lib/table-sync';
import { tableSyncMessage } from '@/lib/table-sync-message';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { useTranslation } from 'react-i18next';

export function TableBackendSync({ campaignId = '', credentialId = null }: { campaignId?: string; credentialId?: string | null }) {
  const { i18n } = useTranslation(); const zh = i18n.language.startsWith('zh');
  const client = useQueryClient(); const queryKey = tableSyncQueryKey(credentialId, campaignId);
  const [checkedPolicy, setCheckedPolicy] = useState<string | null>(null);
  const consentOp = useRef<{action: string; id: string; fingerprint: string | null} | null>(null);
  const status = useQuery({ queryKey, queryFn: () => readTableSyncStatus(campaignId), refetchInterval: 30000 });
  const update = useMutation({ mutationFn: (enabled: boolean) => configureTableSync(campaignId, enabled),
    onSuccess: () => client.invalidateQueries({queryKey}) });
  const consent = useMutation({ mutationFn: async (action: string) => {
    const fingerprint = action === 'grant' ? status.data?.capabilities.policy_fingerprint ?? null : null;
    if (!consentOp.current || action === 'revoke') consentOp.current = {action, id: crypto.randomUUID(), fingerprint};
    const saved = consentOp.current;
    if (saved.action !== action) throw new Error(zh ? '请先确认上次授权操作结果' : 'Resolve the previous consent operation first');
    return changeTableSyncConsent(action, saved.id, saved.fingerprint);
  }, onSuccess: () => { consentOp.current = null; setCheckedPolicy(null); void client.invalidateQueries({queryKey: tableSyncQueryKey(credentialId).slice(0, 2)}); } });
  const policy = status.data?.capabilities.policy;
  const canGrant = !status.isError && !!policy && !!status.data?.capabilities.policy_fingerprint && status.data.capabilities.consent_management_enabled;
  const consentCurrent = !!status.data && hasDefaultSyncConsent(status.data);
  const checked = checkedPolicy !== null && checkedPolicy === status.data?.capabilities.policy_fingerprint;
  const busy = update.isPending || consent.isPending;
  return <Card><CardHeader><CardTitle>{zh ? 'OneCrew 后台表格同步' : 'OneCrew table cloud sync'}</CardTitle></CardHeader>
    <CardContent className="space-y-3 text-sm">
      <p>{zh ? '本地动态表格与后台上传分别保存。确认协议后，搜索结束或暂停时自动上传推荐名单、表格结构与完整评语。' : 'Local tables and cloud uploads are saved separately. After consent, recommendations, table structure and complete notes upload automatically when a search ends or pauses.'}</p>
      {status.isError ? <p role="alert">{zh ? '后台连接失败，当前协议和同步状态无法确认。本地表格仍保留，请重新检查。' : 'Cloud connection failed. Current consent and sync status are unconfirmed. Local tables are retained; check again.'}</p>
        : status.data ? <p role="status">{tableSyncMessage(status.data, !!campaignId, zh)}</p>
        : <p role="status">{zh ? '正在检查后台协议与同步状态…' : 'Checking cloud consent and sync status…'}</p>}
      {!status.isError && campaignId && status.data?.version_id && <p className="break-all text-xs text-muted-foreground">{zh ? '最近确认的后台版本：' : 'Last confirmed cloud version: '}{status.data.version_id}</p>}
      {status.data?.error && !status.isError && <details><summary>{zh ? '查看同步错误代码' : 'Sync error code'}</summary><code>{status.data.error}</code></details>}
      {policy && <div className="space-y-2">
        <p>{zh ? '上传至 OneCrew 后台，用途：使用分析。保留：' : 'Upload to OneCrew for usage analytics. Retention: '}
          {policy.retention_days === null ? (zh ? '无固定期限' : 'No fixed expiry') : `${policy.retention_days}${zh ? '天' : ' days'}`}</p>
        <a href={policy.document_url} target="_blank" rel="noreferrer" className="underline">{zh ? '查看隐私协议' : 'Privacy policy'}</a>{' · '}
        <a href={policy.deletion_url} target="_blank" rel="noreferrer" className="underline">{zh ? '删除申请' : 'Deletion request'}</a>
      </div>}
      {status.data && !consentCurrent && !status.isError && <>
        {!canGrant && <p role="alert">{zh ? '当前同步协议暂不可用，无法确认新的上传授权。请重新检查。' : 'The current sync policy is unavailable. New upload consent cannot be confirmed; check again.'}</p>}
        <label className="flex items-start gap-2"><input type="checkbox" checked={checked} onChange={e => setCheckedPolicy(e.target.checked ? status.data?.capabilities.policy_fingerprint ?? null : null)} disabled={!canGrant || busy}/>
          {zh ? '我已阅读协议，同意上传表格结构、推荐名单及评语。撤回授权将停止后续同步。' : 'I have read the policy and consent to uploading table structure, recommendations and notes. Revoking consent stops future sync.'}</label>
        <Button variant="outline" disabled={!checked || !canGrant || busy} onClick={() => consent.mutate('grant')}>{zh ? '确认上传授权' : 'Confirm upload consent'}</Button>
      </>}
      <div className="flex flex-wrap gap-2">
        <Button variant="outline" disabled={status.isFetching || busy} onClick={() => { void status.refetch(); }}>{zh ? '重新检查同步状态' : 'Check sync status'}</Button>
        {campaignId && <Button disabled={(!status.data?.enabled && !status.isError && !consentCurrent) || busy} onClick={() => update.mutate(status.isError ? false : !status.data?.enabled)}>
          {(status.data?.enabled || status.isError) ? (zh ? '关闭本表同步' : 'Disable this table') : (zh ? '开启本表同步' : 'Enable this table')}</Button>}
        <Button variant="outline" disabled={busy} onClick={() => consent.mutate('revoke')}>{zh ? '撤回后台上传授权' : 'Revoke cloud upload consent'}</Button>
      </div>
      {(update.isError || consent.isError) && <p role="alert">{String((update.error ?? consent.error)?.message ?? (zh ? '操作未确认，请重试同一操作。' : 'Operation unconfirmed. Retry the same operation.'))}</p>}
    </CardContent></Card>;
}
