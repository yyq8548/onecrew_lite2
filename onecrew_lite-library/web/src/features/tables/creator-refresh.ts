/** Bind publication notifications to this mounted view, without polling. */
export function subscribeCreatorRefresh(
  watch: (signal: AbortSignal, changed: () => void) => Promise<void>,
  refresh: () => void,
) {
  const controller = new AbortController();
  let dirty = false;
  let watching = false;
  let retry: ReturnType<typeof setTimeout> | undefined;
  let retryDelay = 1000;
  let lastRefresh = Date.now();
  const refreshWhenVisible = () => {
    if (controller.signal.aborted || document.visibilityState !== 'visible') return;
    if (!dirty && Date.now() - lastRefresh < 1000) return;
    lastRefresh = Date.now();
    dirty = false;
    refresh();
  };
  const connect = () => {
    if (watching || controller.signal.aborted) return;
    if (retry !== undefined) clearTimeout(retry);
    retry = undefined;
    watching = true;
    void watch(controller.signal, () => {
      if (controller.signal.aborted) return;
      retryDelay = 1000;
      dirty = true;
      refreshWhenVisible();
    })
      .catch(() => {
        /* Transport failures preserve data; retry only the notification stream. */
      })
      .finally(() => {
        watching = false;
        // Reconcile publications missed while disconnected, including normal EOF.
        dirty = true;
        if (!controller.signal.aborted && document.visibilityState === 'visible') {
          retry = setTimeout(() => {
            retry = undefined;
            if (document.visibilityState === 'visible') connect();
          }, retryDelay);
          retryDelay = Math.min(retryDelay * 2, 30000);
        }
      });
  };
  const resume = () => {
    if (document.visibilityState !== 'visible' || controller.signal.aborted) return;
    connect();
    refreshWhenVisible();
  };
  const visible = () => {
    if (dirty || !watching) resume();
  };
  connect();
  window.addEventListener('focus', resume);
  window.addEventListener('online', resume);
  document.addEventListener('visibilitychange', visible);
  return () => {
    controller.abort();
    if (retry !== undefined) clearTimeout(retry);
    window.removeEventListener('focus', resume);
    window.removeEventListener('online', resume);
    document.removeEventListener('visibilitychange', visible);
  };
}
