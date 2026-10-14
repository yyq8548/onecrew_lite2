import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock, listenMock, unlistenMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listenMock: vi.fn(),
  unlistenMock: vi.fn(),
}));
vi.mock("@/lib/local-runtime", () => ({ invoke: invokeMock }));
vi.mock("@/lib/runtime-events", () => ({ listen: listenMock }));

import {
  beginCreatorSyncPairing,
  cancelCreatorSyncPairing,
  CREATOR_SYNC_PAIRING_STATUS_CHANGED_EVENT,
  readCreatorSyncPairingStatus,
  reopenCreatorSyncPairing,
  subscribeCreatorSyncPairingStatus,
  pairingAuthorizationUrl,
} from "./creator-sync-pairing";

const projection = {
  browser_opened: false,
  callback_received: false,
  issue: null,
  flow_id: null,
  status: {
    action: "begin_pairing",
    credential_id: null,
    installation_id: null,
    issue: null,
    state: "pairing_required",
  },
} as const;

const pendingProjection = { ...projection, flow_id: "flow-1", status: { ...projection.status, state: "pairing_pending", action: "await_pairing_callback" } } as const;
const authorizeUrl = "https://example.com/authorize?state=synthetic-test-state";

describe("Creator Sync browser pairing client", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.mockReset();
    unlistenMock.mockReset();
    listenMock.mockResolvedValue(unlistenMock);
    vi.stubGlobal("window", { open: vi.fn(() => ({ opener: null, location: { replace: vi.fn() }, close: vi.fn() })) });
  });
  afterEach(() => vi.unstubAllGlobals());

  it("owns the four exact no-input command envelopes", async () => {
    invokeMock.mockImplementation((command: string) => Promise.resolve(
      command === "creator_sync_pairing_begin" || command === "creator_sync_pairing_reopen"
        ? { authorize_url: authorizeUrl, projection: pendingProjection } : projection,
    ));
    await readCreatorSyncPairingStatus();
    await beginCreatorSyncPairing();
    await reopenCreatorSyncPairing();
    await cancelCreatorSyncPairing();
    expect(invokeMock.mock.calls).toEqual([
      ["creator_sync_pairing_status"],
      ["creator_sync_pairing_begin"],
      ["creator_sync_pairing_reopen"],
      ["creator_sync_pairing_cancel"],
    ]);
  });

  it("retains a normal authorization link when the browser blocks a popup", async () => {
    vi.mocked(window.open).mockReturnValue(null);
    invokeMock.mockResolvedValue({ authorize_url: authorizeUrl, projection: pendingProjection });
    await beginCreatorSyncPairing();
    expect(pairingAuthorizationUrl("flow-1")).toBe(authorizeUrl);
    expect(pairingAuthorizationUrl("other-flow")).toBeUndefined();
    invokeMock.mockResolvedValue(projection);
    await cancelCreatorSyncPairing();
    expect(pairingAuthorizationUrl("flow-1")).toBeUndefined();
  });

  it("never opens non-HTTPS or credential-bearing authorization URLs", async () => {
    for (const url of ["javascript:alert(1)", "https://user:password@example.com/login"]) {
      invokeMock.mockResolvedValue({ authorize_url: url, projection: pendingProjection });
      await expect(beginCreatorSyncPairing()).rejects.toThrow("invalid authorization URL");
    }
  });

  it("fails closed on an untyped projection", async () => {
    invokeMock.mockResolvedValue({ ...projection, authorize_url: "must-not-pass" });
    await expect(readCreatorSyncPairingStatus()).rejects.toThrow("invalid projection");
  });

  it("forwards only validated safe projections from the service event", async () => {
    const onProjection = vi.fn();
    const unlisten = await subscribeCreatorSyncPairingStatus(onProjection);

    expect(CREATOR_SYNC_PAIRING_STATUS_CHANGED_EVENT).toBe(
      "creator-sync://pairing-status-changed",
    );
    expect(listenMock).toHaveBeenCalledWith(
      CREATOR_SYNC_PAIRING_STATUS_CHANGED_EVENT,
      expect.any(Function),
    );

    const receive = listenMock.mock.calls[0]?.[1] as (event: { payload: unknown }) => void;
    receive({ payload: projection });
    receive({ payload: { ...projection, authorize_url: "must-not-pass" } });
    receive({ payload: { ...projection, pairing_code: "must-not-pass" } });
    receive({ payload: { status: { state: "active" } } });

    expect(onProjection).toHaveBeenCalledOnce();
    expect(onProjection).toHaveBeenCalledWith(projection);
    unlisten();
    expect(unlistenMock).toHaveBeenCalledOnce();
  });
});
