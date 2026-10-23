import { beforeEach, describe, expect, it, vi } from 'vitest';
const invoke = vi.hoisted(() => vi.fn());
vi.mock('./local-runtime', () => ({ invoke }));
import { acceptDefaultSyncAgreement, type TableSyncStatus } from './table-sync';

const status = (overrides: Partial<TableSyncStatus['capabilities']> = {}): TableSyncStatus => ({
  enabled: false, status: 'disabled', error: null, version_id: null,
  capabilities: { consent_management_enabled: true, upload_enabled: false, consent_current: false,
    policy_fingerprint: 'policy-v1', policy: { document_url: 'https://example.test/policy', deletion_url: 'https://example.test/delete', purpose: 'usage_analytics', retention_days: null },
    ...overrides },
});
describe('default agreement acceptance', () => {
  beforeEach(() => invoke.mockReset());
  it('waits for confirmed acceptance and freezes the displayed policy and operation', async () => {
    let resolve!: (value: unknown) => void;
    invoke.mockReturnValue(new Promise(done => { resolve = done; }));
    let complete = false;
    const request = acceptDefaultSyncAgreement(status(), 'same-operation').then(() => { complete = true; });
    await Promise.resolve();
    expect(complete).toBe(false);
    expect(invoke).toHaveBeenCalledWith('onecrew_table_sync_consent', { input: {
      action: 'grant', operation_id: 'same-operation', policy_fingerprint: 'policy-v1',
    } });
    resolve({ consent: { status: 'granted' } });
    await request;
    expect(complete).toBe(true);
  });
  it('reuses a current grant for both required scopes', async () => {
    await acceptDefaultSyncAgreement(status({ upload_enabled: true, consent_current: true,
      consent: { status: 'granted', scopes: ['dynamic_table_records', 'dynamic_table_schema'] } }), 'unused');
    expect(invoke).not.toHaveBeenCalled();
  });
  it('requires a new confirmation when the existing grant covers only schema', async () => {
    invoke.mockResolvedValue({ consent: { status: 'granted' } });
    await acceptDefaultSyncAgreement(status({ upload_enabled: true, consent_current: true,
      consent: { status: 'granted', scopes: ['dynamic_table_schema'] } }), 'new-operation');
    expect(invoke).toHaveBeenCalledOnce();
  });
  it('does not report success for an unknown outcome or missing policy', async () => {
    invoke.mockResolvedValue({ consent: { status: 'revoked' } });
    await expect(acceptDefaultSyncAgreement(status(), 'operation')).rejects.toThrow('table_sync_consent_unconfirmed');
    invoke.mockReset();
    await expect(acceptDefaultSyncAgreement(status({ policy: null }), 'operation')).rejects.toThrow('table_sync_policy_unavailable');
    expect(invoke).not.toHaveBeenCalled();
  });
  it('keeps the same operation identity for a retry after a transport failure', async () => {
    invoke.mockRejectedValueOnce(new Error('transport')).mockResolvedValueOnce({ consent: { status: 'granted' } });
    await expect(acceptDefaultSyncAgreement(status(), 'retry-operation')).rejects.toThrow('transport');
    await acceptDefaultSyncAgreement(status(), 'retry-operation');
    expect(invoke.mock.calls[0]).toEqual(invoke.mock.calls[1]);
  });
});
