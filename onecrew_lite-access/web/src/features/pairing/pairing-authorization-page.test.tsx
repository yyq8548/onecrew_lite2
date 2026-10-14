import { render, screen } from "@testing-library/react";
import { StrictMode } from "react";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { PairingAuthorizationPage } from "@/features/pairing/pairing-authorization-page";
import {
  AccessApiError,
  getDesktopPairingPreview,
  type PairingPreview,
} from "@/lib/access-api";

vi.mock("@/lib/access-api", async (importOriginal) => {
  const original = await importOriginal<typeof import("@/lib/access-api")>();
  return { ...original, getDesktopPairingPreview: vi.fn() };
});

const preview: PairingPreview = {
  approvalToken: "reacher_pairing_approval_v1." + "A".repeat(180),
  expiresAt: "2999-08-31T12:05:00.000Z",
  request: {
    appName: "Agent Platform",
    appVersion: "1.2.3",
    callbackPort: "49152",
    label: "Studio Mac",
    platform: "macos",
    platformLabel: "macOS",
    bindingIp: null,
    bindingLocation: { country: null, region: null, city: null, colo: null },
  },
};

describe("Desktop loopback pairing authorization page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(getDesktopPairingPreview).mockResolvedValue(preview);
  });

  it("loads only a preview and exposes one explicit same-origin approval form", async () => {
    renderPage();
    expect(await screen.findByText("Studio Mac")).toBeInTheDocument();
    expect(getDesktopPairingPreview).toHaveBeenCalledOnce();
    const submit = screen.getByRole("button", {
      name: "确认绑定并返回 OneCrew Desktop",
    });
    const form = submit.closest("form");
    expect(form).toHaveAttribute("action", "/api/pairing/authorize");
    expect(form).toHaveAttribute("method", "post");
    expect(
      form?.querySelector('input[type="hidden"][name="approvalToken"]'),
    ).toHaveValue(preview.approvalToken);
    expect(screen.getByText("49152")).toBeInTheDocument();
    expect(document.body).not.toHaveTextContent("reacher_pairing_v1");
  });

  it("never creates another preview-side effect during StrictMode replay", async () => {
    render(
      <StrictMode>
        <MemoryRouter initialEntries={["/pairing/authorize?state=request"]}>
          <PairingAuthorizationPage
            user={{ email: "alex@example.com", id: "user-1", image: null, name: "Alex" }}
          />
        </MemoryRouter>
      </StrictMode>,
    );
    expect(await screen.findByText("Studio Mac")).toBeInTheDocument();
    expect(screen.queryByText(/打开 OneCrew Desktop/)).not.toBeInTheDocument();
  });

  it("renders only a fixed safe preview error", async () => {
    const sentinel = "http://127.0.0.1:49152/private-code";
    vi.mocked(getDesktopPairingPreview).mockRejectedValue(new Error(sentinel));
    renderPage();
    expect(await screen.findByText(/绑定链接无效或已过期/)).toBeInTheDocument();
    expect(document.body).not.toHaveTextContent(sentinel);
    expect(screen.queryByRole("button", { name: /确认绑定/ })).not.toBeInTheDocument();
  });

  it("renders a rate limit before exposing the confirmation form", async () => {
    vi.mocked(getDesktopPairingPreview).mockRejectedValue(
      new AccessApiError(429, "rate_limited"),
    );
    renderPage();
    expect(await screen.findByText(/绑定尝试过于频繁/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /确认绑定/ })).not.toBeInTheDocument();
  });

  it("has accessible account, metadata and responsive action contracts", async () => {
    renderPage();
    expect(await screen.findByRole("heading", { level: 1, name: "确认连接 OneCrew Desktop" })).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "OneCrew" })).toHaveAttribute("src", "/onecrew-logo-pairing.png");
    expect(screen.getByText(/alex@example.com/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "确认绑定并返回 OneCrew Desktop" })).toHaveClass("w-full", "sm:w-auto");
    expect(document.body).not.toHaveTextContent("Agent Platform");
    expect(document.body).not.toHaveTextContent("Reacher");
    expect(document.body).not.toHaveTextContent(/页面加载不会授权|授权code有效期/);
    expect(screen.getByText("1.2.3")).toBeInTheDocument();
  });
});

function renderPage() {
  return render(
    <MemoryRouter initialEntries={["/pairing/authorize?state=request"]}>
      <PairingAuthorizationPage
        user={{ email: "alex@example.com", id: "user-1", image: null, name: "Alex" }}
      />
    </MemoryRouter>,
  );
}
