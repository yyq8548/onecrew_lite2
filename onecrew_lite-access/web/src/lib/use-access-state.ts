import { useCallback, useEffect, useRef, useState } from "react";

import {
  getAuthCapabilities,
  getSafeSession,
  isSafeSessionAuthenticated,
  type SafeSession,
} from "@/lib/access-api";

const ANONYMOUS_SESSION: SafeSession = {
  authenticated: false,
  expiresAt: null,
  user: null,
};

const MAX_TIMEOUT_MS = 2_147_483_647;

export type SafeSessionState =
  | { status: "loading" }
  | { status: "error" }
  | { status: "ready"; session: SafeSession };

export function useSafeSession() {
  const [state, setState] = useState<SafeSessionState>({ status: "loading" });
  const mounted = useRef(false);
  const requestSequence = useRef(0);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      requestSequence.current += 1;
    };
  }, []);

  const refresh = useCallback(async (
    options: { background?: boolean } = {},
  ) => {
    const requestId = ++requestSequence.current;
    const background = options.background === true;
    if (mounted.current && !background) {
      setState({ status: "loading" });
    }

    try {
      const session = await getSafeSession();
      const currentSession = isSafeSessionAuthenticated(session)
        ? session
        : ANONYMOUS_SESSION;
      if (mounted.current && requestId === requestSequence.current) {
        setState({ session: currentSession, status: "ready" });
      }
      return currentSession;
    } catch (error) {
      if (mounted.current && requestId === requestSequence.current) {
        setState((current) =>
          background && current.status === "ready"
            ? current
            : { status: "error" },
        );
      }
      throw error;
    }
  }, []);

  useEffect(() => {
    void refresh().catch(() => undefined);
  }, [refresh]);

  useEffect(() => {
    function refreshFromBrowserState() {
      void refresh({ background: true }).catch(() => undefined);
    }

    function refreshWhenVisible() {
      if (document.visibilityState === "visible") {
        refreshFromBrowserState();
      }
    }

    window.addEventListener("focus", refreshFromBrowserState);
    document.addEventListener("visibilitychange", refreshWhenVisible);
    return () => {
      window.removeEventListener("focus", refreshFromBrowserState);
      document.removeEventListener("visibilitychange", refreshWhenVisible);
    };
  }, [refresh]);

  const expiresAt =
    state.status === "ready" && state.session.authenticated
      ? state.session.expiresAt
      : null;

  useEffect(() => {
    if (!expiresAt) {
      return;
    }
    const expiration: string = expiresAt;

    let timeoutId: number | undefined;

    function scheduleExpirationCheck() {
      const remaining = Date.parse(expiration) - Date.now();
      if (remaining <= 0) {
        void refresh().catch(() => undefined);
        return;
      }

      timeoutId = window.setTimeout(
        scheduleExpirationCheck,
        Math.min(remaining, MAX_TIMEOUT_MS),
      );
    }

    scheduleExpirationCheck();
    return () => {
      if (timeoutId !== undefined) {
        window.clearTimeout(timeoutId);
      }
    };
  }, [expiresAt, refresh]);

  return { refresh, state };
}

export function useAuthCapabilities() {
  const [googleSignInEnabled, setGoogleSignInEnabled] = useState(false);
  const [emailOtpEnabled, setEmailOtpEnabled] = useState(false);

  useEffect(() => {
    let active = true;

    void getAuthCapabilities()
      .then((capabilities) => {
        if (active) {
          setGoogleSignInEnabled(capabilities.googleSignInEnabled);
          setEmailOtpEnabled(capabilities.emailOtpEnabled ?? false);
        }
      })
      .catch(() => {
        if (active) {
          setGoogleSignInEnabled(false);
          setEmailOtpEnabled(false);
        }
      });

    return () => {
      active = false;
    };
  }, []);

  return { emailOtpEnabled, googleSignInEnabled };
}
