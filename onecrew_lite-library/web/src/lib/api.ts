import { hc, parseResponse } from 'hono/client';
import type { AppType } from 'onecrew/rpc';
import { clearUnifiedSession, connectUnifiedSession, unifiedRuntimeMode } from './unified-runtime.js';

const signalAccessState = (status: number, body: unknown) => {
  if (!body || typeof body !== 'object' || !('error' in body)) return;
  const error = body.error;
  if (!error || typeof error !== 'object' || !('code' in error)) return;
  if (
    (status === 401 && error.code === 'access.login_required') ||
    (status === 503 && error.code === 'access.unavailable') ||
    (status === 409 && error.code === 'access.account_changed')
  ) {
    window.dispatchEvent(new Event('onecrew:access-changed'));
  }
};

let businessAccountId: string | null = null;
let businessAccountGeneration = 0;

export const setBusinessAccountId = (accountId: string | null) => {
  if (businessAccountId === accountId) return false;
  businessAccountId = accountId;
  businessAccountGeneration += 1;
  return true;
};

const assertCurrentBusinessScope = (generation: number, accountId: string | null) => {
  if (!accountId || businessAccountGeneration !== generation || businessAccountId !== accountId) {
    throw new Error('Response belongs to a previous account session');
  }
};

const isBusinessRequest = (input: RequestInfo | URL) => {
  const url = input instanceof Request ? input.url : String(input);
  const pathname = new URL(url, window.location.origin).pathname;
  return (
    pathname.startsWith('/api/') &&
    !pathname.startsWith('/api/v1/access/') &&
    !pathname.startsWith('/api/v1/system/')
  );
};

const customFetch: typeof fetch = async (input, init) => {
  const businessRequest = isBusinessRequest(input);
  const accountId = businessRequest ? businessAccountId : null;
  const generation = businessAccountGeneration;
  if (businessRequest && !accountId) {
    throw new Error('Sign in before requesting account data');
  }
  const headers = new Headers(input instanceof Request ? input.headers : undefined);
  new Headers(init?.headers).forEach((value, key) => headers.set(key, value));
  if (businessRequest && accountId) headers.set('X-OneCrew-Account-Id', accountId);
  if (unifiedRuntimeMode) {
    const session = await connectUnifiedSession();
    if (businessRequest) assertCurrentBusinessScope(generation, accountId);
    if (businessRequest && session.accountId !== accountId)
      throw new Error('Response belongs to a previous account session');
    headers.set('X-OneCrew-Account-Id', session.accountId);
    headers.set('X-OneCrew-Auth-Epoch', session.epoch);
    const method = init?.method ?? (input instanceof Request ? input.method : 'GET');
    if (!['GET', 'HEAD'].includes(method.toUpperCase()))
      headers.set('X-OneCrew-CSRF', session.csrf);
  }
  const response = await fetch(input, {
    ...init,
    headers,
    ...(unifiedRuntimeMode ? { credentials: 'same-origin', cache: 'no-store' } : {}),
  });
  if (businessRequest) assertCurrentBusinessScope(generation, accountId);
  if (!response.ok) {
    const body = await response
      .clone()
      .json()
      .catch(() => undefined);
    const error = body && typeof body === 'object' && 'error' in body ? body.error : undefined;
    const code = typeof error === 'string' ? error : error && typeof error === 'object' && 'code' in error ? error.code : undefined;
    if (unifiedRuntimeMode && (
      response.status === 401 && (code === 'local.invalid_session' || code === 'local.unauthorized' || code === undefined)
      || response.status === 409 && code === 'access.account_changed'
    )) {
      clearUnifiedSession();
      window.dispatchEvent(new Event('onecrew:access-changed'));
    }
    if (businessRequest) assertCurrentBusinessScope(generation, accountId);
    signalAccessState(response.status, body);
    if (body && typeof body === 'object' && 'error' in body) {
      const error = body.error;
      if (error && typeof error === 'object' && 'message' in error) {
        const message = error.message;
        if (typeof message === 'string' && message) throw new Error(message);
      }
    }
    throw new Error(`HTTP ${response.status}`);
  }
  return response;
};

export const client = hc<AppType>('/', { fetch: customFetch });
export const api = client.api.v1;

export const request = parseResponse;

export async function readBrowserConnection(signal: AbortSignal): Promise<{ connected: boolean }> {
  if (!unifiedRuntimeMode) return request(api.browser.status.$get({}, { init: { signal } }));
  const response = await customFetch('/api/commands/onecrew_opencli_setup_status', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}',
    signal: AbortSignal.any([signal, AbortSignal.timeout(30_000)]),
  });
  if (!response.ok) throw new Error('Browser connection status unavailable');
  const envelope = await response.json();
  if (envelope?.ok !== true || typeof envelope.result?.daemon_running !== 'boolean' || typeof envelope.result?.extension_connected !== 'boolean') throw new Error('Invalid browser connection status');
  return { connected: envelope.result.daemon_running && envelope.result.extension_connected };
}

export const downloadFile = async (response: Promise<Response>) => {
  const generation = businessAccountGeneration;
  const accountId = businessAccountId;
  assertCurrentBusinessScope(generation, accountId);
  const result = await response;
  assertCurrentBusinessScope(generation, accountId);
  const disposition = result.headers.get('content-disposition') ?? '';
  const encoded = disposition.match(/filename\*=UTF-8''([^;]+)/i)?.[1];
  const plain = disposition.match(/filename="?([^";]+)"?/i)?.[1];
  let filename = plain ?? null;
  if (encoded) {
    try {
      filename = decodeURIComponent(encoded);
    } catch {
      filename = encoded;
    }
  }
  const blob = await result.blob();
  assertCurrentBusinessScope(generation, accountId);
  return { blob, filename };
};
