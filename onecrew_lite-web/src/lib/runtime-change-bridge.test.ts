import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hooks = vi.hoisted(() => ({
  cleanup: undefined as undefined | (() => void),
  queryClient: {},
  invalidate: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("react", () => ({
  useEffect: (effect: () => void | (() => void)) => { hooks.cleanup = effect() || undefined; },
}));
vi.mock("@tanstack/react-query", () => ({ useQueryClient: () => hooks.queryClient }));
vi.mock("@/lib/local-runtime", () => ({
  isRuntimeAvailable: () => true,
  connectLocalRuntime: async () => undefined,
}));
vi.mock("@/lib/runtime-change", () => ({
  RUNTIME_CHANGE_EVENT: "runtime://changed",
  invalidateRuntimeChangeScopes: hooks.invalidate,
}));

import { RuntimeChangeBridge } from "./runtime-change-bridge";
import { listen } from "./runtime-events";

class TestEventSource {
  static instances: TestEventSource[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  closed = false;

  constructor(readonly url: string, readonly options: { withCredentials: boolean }) {
    TestEventSource.instances.push(this);
  }

  close() { this.closed = true; }

  change(sequence: number, scopes: string[]) {
    this.onmessage?.({ data: JSON.stringify({ event: "runtime://changed", payload: { sequence, scopes } }) });
  }
}

async function mount() {
  RuntimeChangeBridge();
  // listen waits for the loopback connection before registering its listener.
  await Promise.resolve();
  return TestEventSource.instances[0]!;
}

describe("runtime change bridge over the shared SSE connection", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    hooks.invalidate.mockClear();
    hooks.cleanup = undefined;
    TestEventSource.instances = [];
    vi.stubGlobal("EventSource", TestEventSource);
    vi.stubGlobal("window", { setTimeout, clearTimeout });
  });

  afterEach(async () => {
    hooks.cleanup?.();
    await Promise.resolve();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("coalesces mixed owner scopes in one tick without interpreting sequence", async () => {
    const source = await mount();
    source.change(1, ["service_credentials"]);
    source.change(2, ["email_folders", "service_credentials"]);
    source.change(0, ["all"]);
    expect(hooks.invalidate).not.toHaveBeenCalled();
    vi.runAllTimers();
    expect(hooks.invalidate).toHaveBeenCalledExactlyOnceWith(hooks.queryClient, [
      "service_credentials", "email_folders", "all",
    ]);
  });

  it("shares one authenticated EventSource and invalidates all on each open", async () => {
    const source = await mount();
    const unlisten = await listen("creator-sync://pairing-status-changed", vi.fn());
    try {
      expect(TestEventSource.instances).toHaveLength(1);
      expect(source.url).toBe("/api/events");
      expect(source.options).toEqual({ withCredentials: true });
      source.onopen?.();
      vi.runAllTimers();
      source.onopen?.();
      vi.runAllTimers();
      expect(hooks.invalidate).toHaveBeenCalledTimes(2);
      expect(hooks.invalidate).toHaveBeenNthCalledWith(1, hooks.queryClient, ["all"]);
      expect(hooks.invalidate).toHaveBeenNthCalledWith(2, hooks.queryClient, ["all"]);
      hooks.cleanup?.();
      hooks.cleanup = undefined;
      await Promise.resolve();
      expect(source.closed).toBe(false);
    } finally {
      unlisten();
    }
    expect(source.closed).toBe(true);
  });

  it("unmount cancels pending invalidation and closes the last subscription", async () => {
    const source = await mount();
    source.change(1, ["conversations"]);
    hooks.cleanup?.();
    hooks.cleanup = undefined;
    await Promise.resolve();
    vi.runAllTimers();
    expect(hooks.invalidate).not.toHaveBeenCalled();
    expect(source.closed).toBe(true);
    source.change(2, ["creators"]);
    vi.runAllTimers();
    expect(hooks.invalidate).not.toHaveBeenCalled();
  });
});
