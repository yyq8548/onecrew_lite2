import { hc, parseResponse } from 'hono/client';
import type { AppType } from 'onecrew/rpc';

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
  const response = await fetch(input, { ...init, headers });
  if (businessRequest) assertCurrentBusinessScope(generation, accountId);
  if (!response.ok) {
    const body = await response
      .clone()
      .json()
      .catch(() => undefined);
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
