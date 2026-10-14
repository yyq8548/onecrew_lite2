import { useEffect } from "react";
import { isRuntimeAvailable } from "@/lib/local-runtime";
import { listen } from "@/lib/runtime-events";
import { useQueryClient } from "@tanstack/react-query";

import {
  RUNTIME_CHANGE_EVENT,
  invalidateRuntimeChangeScopes,
  type RuntimeChange,
  type RuntimeChangeScope,
} from "@/lib/runtime-change";

export function RuntimeChangeBridge() {
  const queryClient = useQueryClient();

  useEffect(() => {
    if (!isRuntimeAvailable()) return;

    let active = true;
    let flushTimer: number | null = null;
    const pendingScopes = new Set<RuntimeChangeScope>();

    const flush = () => {
      flushTimer = null;
      if (!active || pendingScopes.size === 0) return;
      const scopes = [...pendingScopes];
      pendingScopes.clear();
      void invalidateRuntimeChangeScopes(queryClient, scopes);
    };

    const unlistenPromise = listen<RuntimeChange>(RUNTIME_CHANGE_EVENT, ({ payload }) => {
      for (const scope of payload.scopes) pendingScopes.add(scope);
      if (flushTimer !== null || pendingScopes.size === 0) return;
      flushTimer = window.setTimeout(flush, 0);
    });
    void unlistenPromise.catch((error) => {
      console.error("failed to listen for runtime changes", error);
    });

    return () => {
      active = false;
      pendingScopes.clear();
      if (flushTimer !== null) window.clearTimeout(flushTimer);
      void unlistenPromise.then((unlisten) => unlisten()).catch(() => undefined);
    };
  }, [queryClient]);

  return null;
}
