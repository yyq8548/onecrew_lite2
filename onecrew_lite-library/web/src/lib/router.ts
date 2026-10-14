import { useCallback, useEffect, useState } from 'react';
export type Route =
  | { kind: 'login'; returnTo: string }
  | { kind: 'auth-complete' }
  | { kind: 'library'; search: string }
  | { kind: 'detail'; tableId: string; search: string }
  | { kind: 'connections' };
export const query = (value: string) => (value ? `?q=${encodeURIComponent(value)}` : '');
export function getLoginReturnPath(value: string | null): string {
  if (
    !value ||
    !value.startsWith('/') ||
    value.startsWith('//') ||
    value.includes('\\') ||
    [...value].some((character) => character.charCodeAt(0) < 0x20)
  ) {
    return '/library';
  }
  let url: URL;
  try {
    url = new URL(value, 'https://onecrew.invalid');
  } catch {
    return '/library';
  }
  if (url.origin !== 'https://onecrew.invalid') return '/library';
  if (url.pathname === '/connections') return '/connections';
  if (url.pathname !== '/library' && !/^\/library\/[^/]+$/.test(url.pathname)) {
    return '/library';
  }
  if (url.pathname !== '/library') {
    try {
      const tableId = decodeURIComponent(url.pathname.slice('/library/'.length));
      if (tableId.includes('/') || tableId.includes('\\')) return '/library';
    } catch {
      return '/library';
    }
  }
  const search = url.searchParams.get('q') ?? '';
  return `${url.pathname}${query(search)}`;
}
export const readRoute = (): Route => {
  const url = new URL(window.location.href);
  const search = url.searchParams.get('q') ?? '';
  if (url.pathname === '/') {
    window.history.replaceState({}, '', `/library${query(search)}`);
    return { kind: 'library', search };
  }
  if (url.pathname === '/auth/complete') return { kind: 'auth-complete' };
  if (url.pathname === '/login') {
    return { kind: 'login', returnTo: getLoginReturnPath(url.searchParams.get('returnTo')) };
  }
  if (url.pathname === '/connections') return { kind: 'connections' };
  const match = url.pathname.match(/^\/library\/([^/]+)$/);
  return match
    ? { kind: 'detail', tableId: decodeURIComponent(match[1]!), search }
    : { kind: 'library', search };
};
export function useRoute() {
  const [route, setRoute] = useState<Route>(() => readRoute());
  useEffect(() => {
    const onPop = () => setRoute(readRoute());
    window.addEventListener('popstate', onPop);
    return () => window.removeEventListener('popstate', onPop);
  }, []);
  const navigate = useCallback((path: string, replace = false) => {
    if (replace) window.history.replaceState({}, '', path);
    else window.history.pushState({}, '', path);
    setRoute(readRoute());
  }, []);
  return { route, navigate };
}
