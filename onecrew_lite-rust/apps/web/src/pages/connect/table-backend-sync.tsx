import { useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { readTableSyncStatus, configureTableSync, changeTableSyncConsent } from '@/lib/table-sync';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { useTranslation } from 'react-i18next';

export function TableBackendSync({ campaignId = '' }: { campaignId?: string }) {
  const { i18n } = useTranslation(); const zh = i18n.language.startsWith('zh');
  const client = useQueryClient(); const queryKey = ['table-backend-sync', campaignId];
  const [checked, setChecked] = useState(false);
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
  }, onSuccess: () => { consentOp.current = null; setChecked(false); void client.invalidateQueries({queryKey}); } });
  const policy = status.data?.capabilities.policy;
  const canGrant = !!policy && status.data?.capabilities.consent_management_enabled;
  const busy = update.isPending || consent.isPending;
  const labels: Record<string,string> = zh ? {disabled:'未开启',waiting:'等待同步',synced:'已同步',up_to_date:'无新变化',error:'同步未完成',needs_attention:'连续失败，请检查后重新开启',consent_revoked:'已撤回授权'}
    : {disabled:'Off',waiting:'Waiting',synced:'Synced',up_to_date:'No changes',error:'Sync incomplete',needs_attention:'Repeated failures. Check and enable again',consent_revoked:'Consent revoked'};
  return <Card><CardHeader><CardTitle>{zh ? '推荐表后台同步' : 'Recommendation table cloud sync'}</CardTitle></CardHeader>
    <CardContent className="space-y-3 text-sm">
      <p>{zh ? '在连接登录页同意默认协议后，搜索结束或暂停时，推荐表的五列结果与完整评语自动同步到后台。' : 'After accepting the default policy on the connection page, recommendation tables and complete rating notes sync automatically when a search ends or pauses.'}</p>
      {campaignId && status.data && <p role="status">{labels[status.data.status] ?? status.data.status}{status.data.error ? ` · ${status.data.error}` : ''}</p>}
      {!campaignId && status.data && <p role="status">{status.data.capabilities.upload_enabled && status.data.capabilities.consent_current
        ? (zh ? '同步授权有效，结果表默认后台同步。' : 'Sync consent is current. Result tables sync automatically.')
        : (zh ? '尚无当前协议的有效接受记录。请在连接登录页确认协议。' : 'No current policy acceptance is recorded. Accept the policy on the connection page.')}</p>}
      {status.isError && <p role="alert">{zh ? '后台能力暂时无法确认，未开始新上传。' : 'Cloud capability unavailable. No new upload started.'}</p>}
      {policy && <div className="space-y-2">
        <p>{zh ? '上传至 OneCrew 后台，用途：使用分析。保留：' : 'Upload to OneCrew for usage analytics. Retention: '}
          {policy.retention_days === null ? (zh ? '无固定期限' : 'No fixed expiry') : `${policy.retention_days}${zh ? '天' : ' days'}`}</p>
        <a href={policy.document_url} target="_blank" rel="noreferrer" className="underline">{zh ? '查看隐私协议' : 'Privacy policy'}</a>{' · '}
        <a href={policy.deletion_url} target="_blank" rel="noreferrer" className="underline">{zh ? '删除申请' : 'Deletion request'}</a>
      </div>}
      {campaignId && !status.data?.capabilities.upload_enabled && <>
        <label className="flex items-start gap-2"><input type="checkbox" checked={checked} onChange={e => setChecked(e.target.checked)} disabled={!canGrant || busy}/>
          {zh ? '我已阅读协议，同意上传表格结构、推荐名单及评语。撤回授权将停止后续同步。' : 'I have read the policy and consent to uploading table structure, recommendations and notes. Revoking consent stops future sync.'}</label>
        <Button variant="outline" disabled={!checked || !canGrant || busy} onClick={() => consent.mutate('grant')}>{zh ? '确认上传授权' : 'Confirm upload consent'}</Button>
      </>}
      <div className="flex flex-wrap gap-2">
        {campaignId && <Button disabled={(!status.data?.enabled && !status.isError && !status.data?.capabilities.upload_enabled) || busy} onClick={() => update.mutate(status.isError ? false : !status.data?.enabled)}>
          {(status.data?.enabled || status.isError) ? (zh ? '关闭本表同步' : 'Disable this table') : (zh ? '开启本表同步' : 'Enable this table')}</Button>}
        <Button variant="outline" disabled={busy} onClick={() => consent.mutate('revoke')}>{zh ? '撤回后台上传授权' : 'Revoke cloud upload consent'}</Button>
      </div>
      {(update.isError || consent.isError) && <p role="alert">{String((update.error ?? consent.error)?.message ?? (zh ? '操作未确认，请重试同一操作。' : 'Operation unconfirmed. Retry the same operation.'))}</p>}
    </CardContent></Card>;
}
