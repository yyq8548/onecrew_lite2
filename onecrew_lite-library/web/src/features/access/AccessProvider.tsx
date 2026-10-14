import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { api, request, setBusinessAccountId } from '@/lib/api';
import type { InferResponseType } from 'hono/client';

export type AccessStatus = InferResponseType<typeof api.access.status.$get, 200>;
export type AccessState = AccessStatus['state'];
type AccessContextValue = {
  status: AccessStatus | null;
  refresh: () => Promise<void>;
  login: (popup?: Window | null) => Promise<string | null>;
  logout: () => Promise<void>;
};
const AccessContext = createContext<AccessContextValue | null>(null);
const requestHeaders = {
  'X-OneCrew-Request': '1',
  'Content-Type': 'application/json',
};
const accessChannelName = 'onecrew:access-invalidation';

export function AccessProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<AccessStatus | null>(null);
  const statusRef = useRef(status);
  statusRef.current = status;
  const generation = useRef(0);
  const mounted = useRef(false);
  const activeAccountId = useRef<string | null>(null);
  const accessChannel = useRef<BroadcastChannel | null>(null);
  const refreshFlight = useRef<{ generation: number; promise: Promise<void> } | null>(null);
  const broadcastInvalidation = useCallback(() => {
    accessChannel.current?.postMessage({ type: 'invalidate' });
  }, []);
  const applyStatus = useCallback(
    (next: AccessStatus, notifyOtherTabs = true) => {
      const nextAccountId = next.state === 'signed_in' ? (next.user?.id ?? null) : null;
      const previousAccountId = activeAccountId.current;
      setBusinessAccountId(nextAccountId);
      activeAccountId.current = nextAccountId;
      if (notifyOtherTabs && previousAccountId && previousAccountId !== nextAccountId) {
        broadcastInvalidation();
      }
      statusRef.current = next;
      setStatus(next);
    },
    [broadcastInvalidation],
  );
  const invalidateScope = useCallback(
    (broadcast: boolean) => {
      const previousAccountId = activeAccountId.current;
      if (!previousAccountId && statusRef.current?.state === 'unavailable') return false;
      activeAccountId.current = null;
      setBusinessAccountId(null);
      generation.current += 1;
      const next: AccessStatus = { state: 'unavailable', user: null };
      statusRef.current = next;
      setStatus(next);
      if (broadcast) broadcastInvalidation();
      return true;
    },
    [broadcastInvalidation],
  );
  const refresh = useCallback(async () => {
    const requestGeneration = generation.current;
    if (refreshFlight.current?.generation === requestGeneration) {
      return refreshFlight.current.promise;
    }
    const promise = Promise.resolve()
      .then(async () => {
        try {
          const next = await request(api.access.status.$get({}, { headers: requestHeaders }));
          if (mounted.current && generation.current === requestGeneration) applyStatus(next);
        } catch {
          if (mounted.current && generation.current === requestGeneration) {
            applyStatus({ state: 'unavailable', user: null });
          }
        }
      })
      .finally(() => {
        if (refreshFlight.current?.promise === promise) refreshFlight.current = null;
      });
    refreshFlight.current = { generation: requestGeneration, promise };
    return promise;
  }, [applyStatus]);
  useEffect(() => {
    mounted.current = true;
    const channel = new BroadcastChannel(accessChannelName);
    accessChannel.current = channel;
    channel.onmessage = (event: MessageEvent<unknown>) => {
      if (
        !event.data ||
        typeof event.data !== 'object' ||
        !('type' in event.data) ||
        event.data.type !== 'invalidate'
      ) {
        return;
      }
      invalidateScope(false);
      void refresh();
    };
    void refresh();
    const onFocus = () => void refresh();
    const onAccessChanged = () => {
      invalidateScope(true);
      void refresh();
    };
    window.addEventListener('focus', onFocus);
    window.addEventListener('onecrew:access-changed', onAccessChanged);
    return () => {
      mounted.current = false;
      generation.current += 1;
      channel.close();
      accessChannel.current = null;
      window.removeEventListener('focus', onFocus);
      window.removeEventListener('onecrew:access-changed', onAccessChanged);
    };
  }, [invalidateScope, refresh]);
  useEffect(() => {
    const interval =
      status?.state === 'signed_in'
        ? 30_000
        : status?.state === 'authorizing' || status?.state === 'unavailable'
          ? 3_000
          : null;
    if (interval === null) return;
    const timer = window.setInterval(() => {
      if (document.visibilityState === 'visible') void refresh();
    }, interval);
    return () => window.clearInterval(timer);
  }, [refresh, status?.state]);
  const login = useCallback(
    async (popup?: Window | null) => {
      invalidateScope(false);
      const requestGeneration = ++generation.current;
      try {
        const result = await request(api.access.login.$post({}, { headers: requestHeaders }));
        if (generation.current !== requestGeneration) return null;
        if (popup) popup.location.href = result.authorize_url;
        const next: AccessStatus = { state: 'authorizing', user: statusRef.current?.user ?? null };
        statusRef.current = next;
        setStatus(next);
        broadcastInvalidation();
        return result.authorize_url;
      } catch (error) {
        if (generation.current === requestGeneration) void refresh();
        throw error;
      }
    },
    [broadcastInvalidation, invalidateScope, refresh],
  );
  const logout = useCallback(async () => {
    invalidateScope(false);
    const requestGeneration = ++generation.current;
    try {
      const next = await request(api.access.logout.$post({}, { headers: requestHeaders }));
      if (generation.current === requestGeneration) {
        applyStatus(next, false);
        broadcastInvalidation();
      }
    } catch (error) {
      if (generation.current === requestGeneration) void refresh();
      throw error;
    }
  }, [applyStatus, broadcastInvalidation, invalidateScope, refresh]);
  const value = useMemo(
    () => ({ status, refresh, login, logout }),
    [status, refresh, login, logout],
  );
  return <AccessContext.Provider value={value}>{children}</AccessContext.Provider>;
}
// The hook intentionally lives beside its provider so access state has one public entry point.
// eslint-disable-next-line react-refresh/only-export-components
export function useAccess() {
  const value = useContext(AccessContext);
  if (!value) throw new Error('useAccess must be used inside AccessProvider');
  return value;
}
