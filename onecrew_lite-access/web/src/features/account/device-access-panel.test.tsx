import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { DeviceAccessPanel } from "@/features/account/device-access-panel";
import {
  deleteAccessDevice,
  getAccessDeviceStatus,
  listAccessDevices,
  revokeAccessDevice,
  type DeviceSummary,
} from "@/lib/access-api";

vi.mock("@/lib/access-api", async (importOriginal) => {
  const original = await importOriginal<typeof import("@/lib/access-api")>();
  return {
    ...original,
    deleteAccessDevice: vi.fn(),
    getAccessDeviceStatus: vi.fn(),
    listAccessDevices: vi.fn(),
    revokeAccessDevice: vi.fn(),
  };
});

const refreshSession = vi.fn().mockResolvedValue({
  authenticated: true,
  expiresAt: "2999-09-01T00:00:00.000Z",
  user: { email: "alex@example.com", id: "user-1", image: null, name: "Alex" },
});

describe("paired device management", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listAccessDevices).mockResolvedValue({ devices: [], nextCursor: null });
    vi.mocked(getAccessDeviceStatus).mockResolvedValue(
      deviceSummary({ pairedAt: "2026-08-31T12:01:00.000Z", status: "active" }),
    );
    vi.mocked(revokeAccessDevice).mockResolvedValue(
      deviceSummary({ revokedAt: "2026-08-31T12:02:00.000Z", status: "revoked" }),
    );
    vi.mocked(deleteAccessDevice).mockResolvedValue("dev_000000000000000000000000");
  });

  it("keeps device management free of manual-key and pairing implementation detail", async () => {
    renderPanel();
    expect(await screen.findByText("暂无设备")).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "已连接设备" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "刷新全部设备状态" })).not.toBeInTheDocument();
    expect(document.body).not.toHaveTextContent(/PKCE|verifier|approval token|Creator Sync|同步/);
    expect(screen.queryByLabelText("设备名称")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /生成|复制/ })).not.toBeInTheDocument();
  });

  it("maps the legacy default device label only in the presentation layer", async () => {
    vi.mocked(listAccessDevices).mockResolvedValue({
      devices: [deviceSummary({ label: "Agent Platform Local" })],
      nextCursor: null,
    });
    renderPanel();
    expect(await screen.findByText("OneCrew Local")).toBeInTheDocument();
    expect(screen.queryByText("Agent Platform Local")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "解除绑定“OneCrew Local”" })).toBeInTheDocument();
  });

  it("loads cursor pages and keeps one state action per device", async () => {
    const first = deviceSummary();
    const second = deviceSummary({
      deviceId: "dev_111111111111111111111111",
      label: "Travel Laptop",
      pairingId: "pair_111111111111111111111111",
      platform: "windows",
    });
    vi.mocked(listAccessDevices)
      .mockResolvedValueOnce({ devices: [first], nextCursor: "dev_next/cursor" })
      .mockResolvedValueOnce({ devices: [second], nextCursor: null });
    const user = userEvent.setup();
    renderPanel();
    await screen.findByText("Studio Mac");
    await user.click(screen.getByRole("button", { name: "加载更多设备" }));
    expect(await screen.findByText("Travel Laptop")).toBeInTheDocument();
    expect(listAccessDevices).toHaveBeenNthCalledWith(
      2,
      "dev_next/cursor",
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    );
    expect(screen.queryByRole("button", { name: "刷新“Studio Mac”状态" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "解除绑定“Studio Mac”" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "解除绑定“Travel Laptop”" })).toBeInTheDocument();
  });

  it("confirms revoke and authoritatively refetches", async () => {
    const pending = deviceSummary();
    const revoked = { ...pending, revokedAt: "2026-08-31T12:02:00.000Z", status: "revoked" as const };
    vi.mocked(listAccessDevices)
      .mockResolvedValueOnce({ devices: [pending], nextCursor: null })
      .mockResolvedValueOnce({ devices: [revoked], nextCursor: null });
    vi.mocked(revokeAccessDevice).mockResolvedValue(revoked);
    const user = userEvent.setup();
    renderPanel();
    await screen.findByText("Studio Mac");
    await user.click(screen.getByRole("button", { name: "解除绑定“Studio Mac”" }));
    const dialog = screen.getByRole("alertdialog", { name: "解除绑定设备“Studio Mac”？" });
    expect(revokeAccessDevice).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole("button", { name: "确认解除绑定" }));
    await waitFor(() => expect(listAccessDevices).toHaveBeenCalledTimes(2));
    expect(await screen.findByText("已撤销")).toBeInTheDocument();
  });

  it("keeps the authoritative revoke response when refetch fails", async () => {
    const pending = deviceSummary();
    const revoked = { ...pending, revokedAt: "2026-08-31T12:02:00.000Z", status: "revoked" as const };
    vi.mocked(listAccessDevices)
      .mockResolvedValueOnce({ devices: [pending], nextCursor: null })
      .mockRejectedValueOnce(new Error("private-refresh-detail"));
    vi.mocked(revokeAccessDevice).mockResolvedValue(revoked);
    const user = userEvent.setup();
    renderPanel();
    await screen.findByText("Studio Mac");
    await user.click(screen.getByRole("button", { name: "解除绑定“Studio Mac”" }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认解除绑定" }));
    expect(await screen.findByText("已撤销")).toBeInTheDocument();
    expect(await screen.findByText(/当前状态来自服务端撤销响应/)).toBeInTheDocument();
    expect(document.body).not.toHaveTextContent("private-refresh-detail");
  });

  it("disables peer actions while an unbind is in flight", async () => {
    const first = deviceSummary();
    const second = deviceSummary({
      deviceId: "dev_111111111111111111111111",
      label: "Travel Laptop",
      pairingId: "pair_111111111111111111111111",
    });
    const revoke = deferred<DeviceSummary>();
    vi.mocked(listAccessDevices).mockResolvedValueOnce({
      devices: [first, second],
      nextCursor: "dev_next",
    });
    vi.mocked(revokeAccessDevice).mockReturnValue(revoke.promise);
    const user = userEvent.setup();
    renderPanel();
    await screen.findByText("Travel Laptop");
    await user.click(screen.getByRole("button", { name: "解除绑定“Studio Mac”" }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "确认解除绑定" }));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "解除绑定“Travel Laptop”" })).toBeDisabled();
    });
    revoke.resolve({ ...first, revokedAt: "2026-08-31T12:02:00.000Z", status: "revoked" });
    await act(async () => { await revoke.promise; });
  });

  it("deletes a revoked device and keeps the authoritative removal on refresh failure", async () => {
    const revoked = deviceSummary({
      pairedAt: "2026-08-31T12:01:00.000Z",
      revokedAt: "2026-08-31T12:02:00.000Z",
      status: "revoked",
    });
    vi.mocked(listAccessDevices)
      .mockResolvedValueOnce({ devices: [revoked], nextCursor: null })
      .mockRejectedValueOnce(new Error("private-delete-refresh"));
    const user = userEvent.setup();
    renderPanel();
    await screen.findByText("Studio Mac");
    await user.click(screen.getByRole("button", { name: "删除“Studio Mac”" }));
    const dialog = screen.getByRole("alertdialog", { name: "删除设备“Studio Mac”？" });
    await user.click(within(dialog).getByRole("button", { name: "确认删除" }));
    await waitFor(() => expect(deleteAccessDevice).toHaveBeenCalledWith(revoked.deviceId));
    expect(screen.queryByText("Studio Mac")).not.toBeInTheDocument();
    expect(await screen.findByText(/当前列表已采用服务端删除响应/)).toBeInTheDocument();
    expect(document.body).not.toHaveTextContent("private-delete-refresh");
  });

  it("keeps accessible headings, narrow actions and long-label revoke wrapping", async () => {
    const longLabel = "A".repeat(80);
    vi.mocked(listAccessDevices).mockResolvedValue({
      devices: [deviceSummary({ label: longLabel })],
      nextCursor: null,
    });
    const user = userEvent.setup();
    renderPanel();
    await screen.findByText(longLabel);
    expect(screen.getByRole("region", { name: "已连接设备" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "已连接设备" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: `解除绑定“${longLabel}”` })).toHaveClass("w-full", "sm:w-auto");
    await user.click(screen.getByRole("button", { name: `解除绑定“${longLabel}”` }));
    expect(within(screen.getByRole("alertdialog")).getByRole("heading")).toHaveClass("[overflow-wrap:anywhere]");
  });
});

function renderPanel() {
  return render(
    <DeviceAccessPanel
      ownerUserId="user-1"
      refreshSession={refreshSession}
    />,
  );
}

function deviceSummary(overrides: Partial<DeviceSummary> = {}): DeviceSummary {
  return {
    appVersion: "1.2.3",
    createdAt: "2026-08-31T12:00:00.000Z",
    deviceId: "dev_000000000000000000000000",
    label: "Studio Mac",
    pairedAt: null,
    pairingExpiresAt: "2026-08-31T12:01:30.000Z",
    pairingId: "pair_000000000000000000000000",
    pairingIp: null,
    pairingLocation: { country: null, region: null, city: null, colo: null },
    platform: "macos",
    revokedAt: null,
    status: "pending",
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => { resolve = resolvePromise; });
  return { promise, resolve };
}
