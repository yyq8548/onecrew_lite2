/// <reference types="vite/client" />
import { withBrowserSessionLock } from '@onecrew/runtime-contracts/browser-session-lock';
export const unifiedRuntimeMode = typeof window !== 'undefined' && import.meta.env.VITE_ONECREW_UNIFIED === '1';
type UnifiedSession = Readonly<{ accountId: string; epoch: string; csrf: string }>;
let current: UnifiedSession | null = null;
let connecting: Promise<UnifiedSession> | null = null;

export function clearUnifiedSession(): void {
  current = null;
  connecting = null;
}

export async function connectUnifiedSession(): Promise<UnifiedSession> {
  if (!unifiedRuntimeMode) throw new Error('The unified runtime build is required');
  if (current) return current;
  if (connecting) return connecting;
  const flight = withBrowserSessionLock(async () => {
    const url = new URL(window.location.href),
      fragment = new URLSearchParams(url.hash.slice(1));
    const tickets = fragment.getAll('onecrew_ticket');
    if (tickets.length > 1) throw new Error('Invalid workspace ticket');
    if (tickets.length === 1) {
      fragment.delete('onecrew_ticket');
      url.hash = fragment.toString();
      window.history.replaceState(window.history.state, '', url);
      const exchange = await fetch('/api/local/session', {
        method: 'POST',
        credentials: 'same-origin',
        cache: 'no-store',
        signal: AbortSignal.timeout(10_000),
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ ticket: tickets[0] }),
      });
      if (!exchange.ok) throw new Error('Open Library again from OneCrew');
    }
    const response = await fetch('/api/session', { credentials: 'same-origin', cache: 'no-store', signal: AbortSignal.timeout(10_000) });
    if (!response.ok) throw new Error('Open Library again from OneCrew');
    const value: unknown = await response.json();
    if (
      !value ||
      typeof value !== 'object' ||
      !('ok' in value) ||
      value.ok !== true ||
      !('result' in value) ||
      !value.result ||
      typeof value.result !== 'object'
    )
      throw new Error('Invalid unified session');
    const result = value.result;
    if (
      !('protocol_version' in result) ||
      result.protocol_version !== 2 ||
      !('transport' in result) ||
      result.transport !== 'local_http' ||
      !('accountId' in result) ||
      typeof result.accountId !== 'string' ||
      !result.accountId ||
      !('epoch' in result) ||
      typeof result.epoch !== 'string' ||
      !result.epoch ||
      !('csrf' in result) ||
      typeof result.csrf !== 'string' ||
      !result.csrf
    )
      throw new Error('Invalid unified session');
    return Object.freeze({ accountId: result.accountId, epoch: result.epoch, csrf: result.csrf });
  });
  connecting = flight;
  return flight.then(
    (session) => {
      if (connecting !== flight) throw new Error('The account session changed');
      current = session;
      return session;
    },
    (error) => {
      if (connecting === flight) clearUnifiedSession();
      throw error;
    },
  );
}
