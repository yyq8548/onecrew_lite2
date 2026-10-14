import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  getAuthCapabilities,
  getSafeSession,
  type SafeSession,
} from "@/lib/access-api";

import { useAuthCapabilities, useSafeSession } from "./use-access-state";

vi.mock("@/lib/access-api", async (importOriginal) => {
  const original = await importOriginal<typeof import("@/lib/access-api")>();
  return {
    ...original,
    getAuthCapabilities: vi.fn(),
    getSafeSession: vi.fn(),
  };
});

const anonymousSession = {
  authenticated: false,
  expiresAt: null,
  user: null,
} satisfies SafeSession;

function authenticatedSession(
  email: string,
  expiresAt = "2999-09-01T00:00:00.000Z",
): SafeSession {
  return {
    authenticated: true,
    expiresAt,
    user: {
      email,
      id: `user-${email}`,
      image: null,
      name: "Alex Chen",
    },
  };
}

describe("safe access state hooks", () => {
  beforeEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
    vi.mocked(getSafeSession).mockResolvedValue(anonymousSession);
    vi.mocked(getAuthCapabilities).mockResolvedValue({ googleSignInEnabled: false });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("starts in a loading state then restores the safe session", async () => {
    vi.mocked(getSafeSession).mockResolvedValue(authenticatedSession("alex@example.com"));

    render(<SessionProbe />);

    expect(screen.getByText("loading")).toBeInTheDocument();
    expect(await screen.findByText("alex@example.com")).toBeInTheDocument();
  });

  it("refreshes authoritatively at expiry and stops projecting Account state", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-26T12:00:00.000Z"));
    vi.mocked(getSafeSession)
      .mockResolvedValueOnce(
        authenticatedSession("alex@example.com", "2026-08-26T12:00:01.000Z"),
      )
      .mockResolvedValueOnce(anonymousSession);

    render(<SessionProbe />);
    await flushPromises();
    expect(screen.getByText("alex@example.com")).toBeInTheDocument();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });

    expect(getSafeSession).toHaveBeenCalledTimes(2);
    expect(screen.getByText("anonymous")).toBeInTheDocument();
  });

  it("does not schedule expiry refreshes for anonymous sessions", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-08-26T12:00:00.000Z"));

    render(<SessionProbe />);
    await flushPromises();
    expect(screen.getByText("anonymous")).toBeInTheDocument();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(31_536_000_000);
    });

    expect(getSafeSession).toHaveBeenCalledOnce();
  });

  it("refetches on window focus and when the document becomes visible", async () => {
    render(<SessionProbe />);
    expect(await screen.findByText("anonymous")).toBeInTheDocument();
    expect(getSafeSession).toHaveBeenCalledOnce();

    fireEvent.focus(window);
    await waitFor(() => expect(getSafeSession).toHaveBeenCalledTimes(2));

    const visibility = vi.spyOn(document, "visibilityState", "get");
    visibility.mockReturnValue("hidden");
    fireEvent(document, new Event("visibilitychange"));
    expect(getSafeSession).toHaveBeenCalledTimes(2);

    visibility.mockReturnValue("visible");
    fireEvent(document, new Event("visibilitychange"));
    await waitFor(() => expect(getSafeSession).toHaveBeenCalledTimes(3));
  });

  it("keeps the ready owner mounted while a background focus refresh is pending", async () => {
    const pending = deferred<SafeSession>();
    vi.mocked(getSafeSession)
      .mockResolvedValueOnce(authenticatedSession("alex@example.com"))
      .mockReturnValueOnce(pending.promise);

    render(<SessionProbe />);
    expect(await screen.findByText("alex@example.com")).toBeInTheDocument();

    fireEvent.focus(window);
    await waitFor(() => expect(getSafeSession).toHaveBeenCalledTimes(2));
    expect(screen.getByText("alex@example.com")).toBeInTheDocument();
    expect(screen.queryByText("loading")).not.toBeInTheDocument();

    pending.resolve(authenticatedSession("alex@example.com"));
    await act(async () => {
      await pending.promise;
    });
  });

  it("does not let an older concurrent response overwrite the latest refresh", async () => {
    const first = deferred<SafeSession>();
    const second = deferred<SafeSession>();
    vi.mocked(getSafeSession)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);

    render(<RefreshableSessionProbe />);
    await waitFor(() => expect(getSafeSession).toHaveBeenCalledOnce());
    fireEvent.click(screen.getByRole("button", { name: "刷新" }));
    expect(getSafeSession).toHaveBeenCalledTimes(2);

    second.resolve(authenticatedSession("latest@example.com"));
    await act(async () => {
      await second.promise;
    });
    expect(screen.getByText("latest@example.com")).toBeInTheDocument();

    first.resolve(authenticatedSession("stale@example.com"));
    await act(async () => {
      await first.promise;
    });
    expect(screen.getByText("latest@example.com")).toBeInTheDocument();
    expect(screen.queryByText("stale@example.com")).not.toBeInTheDocument();
  });

  it("ignores an in-flight session response after unmount", async () => {
    const pending = deferred<SafeSession>();
    vi.mocked(getSafeSession).mockReturnValueOnce(pending.promise);
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const { unmount } = render(<SessionProbe />);
    await waitFor(() => expect(getSafeSession).toHaveBeenCalledOnce());

    unmount();
    pending.resolve(authenticatedSession("after-unmount@example.com"));
    await act(async () => {
      await pending.promise;
    });

    expect(consoleError).not.toHaveBeenCalled();
  });

  it("enables Google from the capability endpoint", async () => {
    vi.mocked(getAuthCapabilities).mockResolvedValue({ googleSignInEnabled: true });
    render(<CapabilityProbe />);

    expect(screen.getByText("disabled")).toBeInTheDocument();
    expect(await screen.findByText("enabled")).toBeInTheDocument();
  });

  it("fails closed to email-only when capability lookup fails", async () => {
    vi.mocked(getAuthCapabilities).mockRejectedValue(new Error("offline"));
    render(<CapabilityProbe />);

    expect(await screen.findByText("disabled")).toBeInTheDocument();
    expect(getAuthCapabilities).toHaveBeenCalledOnce();
  });
});

function SessionProbe() {
  const { state } = useSafeSession();
  return <span>{sessionStateLabel(state)}</span>;
}

function RefreshableSessionProbe() {
  const { refresh, state } = useSafeSession();
  return (
    <>
      <button type="button" onClick={() => void refresh().catch(() => undefined)}>
        刷新
      </button>
      <span>{sessionStateLabel(state)}</span>
    </>
  );
}

function sessionStateLabel(state: ReturnType<typeof useSafeSession>["state"]) {
  if (state.status !== "ready") {
    return state.status;
  }
  return state.session.authenticated ? state.session.user.email : "anonymous";
}

function CapabilityProbe() {
  const { googleSignInEnabled } = useAuthCapabilities();
  return <span>{googleSignInEnabled ? "enabled" : "disabled"}</span>;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

async function flushPromises() {
  await act(async () => {
    await Promise.resolve();
  });
}
