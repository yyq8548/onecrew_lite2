import { beforeAll, describe, expect, it } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createInstance } from 'i18next';
import { I18nextProvider } from 'react-i18next';
import { TableBackendSync } from './table-backend-sync';
import { tableSyncQueryKey, type TableSyncStatus } from '@/lib/table-sync';
import { tableSyncMessage } from '@/lib/table-sync-message';

const i18n = createInstance();
beforeAll(async () => { await i18n.init({ lng: 'zh-CN', resources: {} }); });
const status = (granted: boolean): TableSyncStatus => ({
  enabled: true, status: 'waiting', error: null, version_id: null,
  capabilities: { upload_enabled: granted, consent_current: granted, consent_management_enabled: true,
    consent: granted ? { status: 'granted', scopes: ['dynamic_table_schema', 'dynamic_table_records'] } : null,
    policy_fingerprint: 'a'.repeat(64), policy: { document_url: 'https://example.test/privacy', deletion_url: 'https://example.test/delete', purpose: 'usage_analytics', retention_days: null } },
});
function render(value: TableSyncStatus, campaignId = '') {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
  client.setQueryData(tableSyncQueryKey(null, campaignId), value);
  const html = renderToStaticMarkup(<I18nextProvider i18n={i18n}><QueryClientProvider client={client}><TableBackendSync campaignId={campaignId} /></QueryClientProvider></I18nextProvider>);
  client.clear();
  return html;
}

describe('recommendation cloud consent and status', () => {
  it('shows current consent without repeating confirmation or per-table prompts in account settings', () => {
    const html = render(status(true));
    expect(html).toContain('结果表默认后台同步');
    expect(html).not.toContain('确认上传授权');
    expect(html).not.toContain('开启本表同步');
    expect(html).toContain('撤回后台上传授权');
    expect(html).toContain('重新检查同步状态');
  });
  it('exposes an unchecked explicit policy confirmation on both settings and table views', () => {
    for (const id of ['', 'campaign']) {
      const html = render(status(false), id);
      expect(html).toContain('等待确认当前同步协议');
      expect(html).toContain('新结果尚未上传');
      expect(html).toContain('无固定期限');
      expect(html).not.toContain('checked=');
      expect(html).toMatch(/<button[^>]*disabled=""[^>]*>确认上传授权/);
      expect(html).toContain('https://example.test/privacy');
    }
  });
  it('requires a complete scoped grant and a cloud receipt before showing upload success', () => {
    const value = status(true);
    value.capabilities.consent!.scopes = ['dynamic_table_schema'];
    expect(tableSyncMessage(value, true, true)).toContain('等待确认当前同步协议');
    value.capabilities.consent!.scopes.push('dynamic_table_records');
    value.status = 'synced';
    expect(tableSyncMessage(value, true, true)).toContain('无法确认上传完成');
    value.version_id = 'caa7fa43-382c-4fbf-913e-903f497eeea9';
    expect(render(value, 'campaign')).toContain('后台已确认接收');
    expect(render(value, 'campaign')).toContain(value.version_id);
  });
  it('keeps waiting, failure, revocation and unknown states distinct from success', () => {
    const value = status(true);
    expect(tableSyncMessage(value, true, true)).toContain('尚未确认本轮上传完成');
    value.status = 'error'; value.error = 'table_sync_http_503';
    expect(render(value, 'campaign')).toContain('后台同步失败');
    expect(render(value, 'campaign')).toContain('table_sync_http_503');
    value.error = 'table_sync_consent_required';
    expect(tableSyncMessage(value, true, true)).toContain('等待后台重新检查');
    value.status = 'consent_revoked';
    expect(tableSyncMessage(value, true, true)).toContain('后续同步已停止');
    value.status = 'future_state'; value.error = null;
    expect(tableSyncMessage(value, true, false)).toContain('Unrecognized sync status');
  });
});
