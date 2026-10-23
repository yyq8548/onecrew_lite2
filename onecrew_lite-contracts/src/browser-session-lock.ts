export async function withBrowserSessionLock<T>(connect: () => Promise<T>): Promise<T> {
  return await navigator.locks.request('onecrew.local-session', { mode: 'exclusive', signal: AbortSignal.timeout(10_000) }, connect);
}
