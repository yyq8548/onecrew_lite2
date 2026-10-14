import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { describe, expect, it, vi } from "vitest";

import type { SafeSession } from "@/lib/access-api";
import { signOutCurrentSession } from "@/lib/auth-actions";

import { App, AppRoutes } from "./app";

vi.mock("@/lib/auth-actions", () => ({
  signInWithEmail: vi.fn(),
  signUpWithEmail: vi.fn(),
  startGoogleSignIn: vi.fn(),
  signOutCurrentSession: vi.fn(),
}));

vi.mock("@/features/account/device-access-panel", () => ({
  DeviceAccessPanel: () => <section aria-label="设备管理">设备管理</section>,
}));

vi.mock("@/features/pairing/pairing-authorization-page", () => ({
  PairingAuthorizationPage: () => <main>Pairing confirmation</main>,
}));

vi.mock("@/features/pairing/pairing-rate-limited-page", () => ({
  PairingRateLimitedPage: () => <main>Pairing rate limited</main>,
}));

const anonymousSession = {
  authenticated: false,
  expiresAt: null,
  user: null,
} satisfies SafeSession;

const authenticatedSession = {
  authenticated: true,
  expiresAt: "2999-09-01T00:00:00.000Z",
  user: {
    id: "user-1",
    name: "Alex Chen",
    email: "alex@example.com",
    image: null,
  },
} satisfies SafeSession;

describe("AppRoutes safe session projection", () => {
  it("keeps the public root as the OneCrew landing page", () => {
    render(
      <MemoryRouter initialEntries={["/"]}>
        <App />
      </MemoryRouter>,
    );

    expect(
      screen.getByRole("heading", { name: "让 Codex 或 Claude 帮你找到对的人" }),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("link", { name: /立即体验/ })[0]).toHaveAttribute(
      "href",
      "https://access.onecrewai.io/account?apply=1&lng=zh-CN",
    );
  });

  it("renders the restored safe session", () => {
    render(
      <MemoryRouter initialEntries={["/account"]}>
        <AppRoutes
          googleSignInEnabled
          session={authenticatedSession}
          refreshSession={vi.fn().mockResolvedValue(authenticatedSession)}
        />
      </MemoryRouter>,
    );

    expect(screen.getByRole("heading", { name: "你好，Alex Chen" })).toBeInTheDocument();
    const logo = screen.getByRole("img", { name: "OneCrew" });
    expect(logo).toHaveAttribute(
      "src",
      "/onecrew-logo-dark.svg",
    );
    expect(logo.closest("a")).toHaveAttribute("href", "https://onecrewai.io/zh");
    expect(screen.getAllByText("alex@example.com").length).toBeGreaterThan(0);
    expect(screen.queryByText(/Reacher/)).not.toBeInTheDocument();
    expect(authenticatedSession).not.toHaveProperty("token");
    expect(authenticatedSession.user).not.toHaveProperty("token");
  });

  it("sends an anonymous account route to login and maps a global OAuth callback error", () => {
    render(
      <MemoryRouter initialEntries={["/account?error=unable_to_get_user_info&error_description=private"]}>
        <AppRoutes
          googleSignInEnabled
          session={anonymousSession}
          refreshSession={vi.fn().mockResolvedValue(anonymousSession)}
        />
      </MemoryRouter>,
    );

    expect(screen.getByRole("heading", { name: "登录" })).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("无法读取 Google 账号信息");
    expect(screen.queryByText("private")).not.toBeInTheDocument();
  });

  it("never renders Account from an expired authenticated projection", () => {
    render(
      <MemoryRouter initialEntries={["/account"]}>
        <AppRoutes
          googleSignInEnabled
          session={{ ...authenticatedSession, expiresAt: "2000-01-01T00:00:00.000Z" }}
          refreshSession={vi.fn().mockResolvedValue(anonymousSession)}
        />
      </MemoryRouter>,
    );

    expect(screen.getByRole("heading", { name: "登录" })).toBeInTheDocument();
    expect(screen.queryByText("当前官网会话有效")).not.toBeInTheDocument();
  });

  it("projects the Google capability into auth routes", () => {
    const { rerender } = render(
      <MemoryRouter initialEntries={["/login"]}>
        <AppRoutes
          googleSignInEnabled={false}
          session={anonymousSession}
          refreshSession={vi.fn().mockResolvedValue(anonymousSession)}
        />
      </MemoryRouter>,
    );

    expect(screen.queryByRole("button", { name: "使用 Google 继续" })).not.toBeInTheDocument();

    rerender(
      <MemoryRouter initialEntries={["/login"]}>
        <AppRoutes
          googleSignInEnabled
          session={anonymousSession}
          refreshSession={vi.fn().mockResolvedValue(anonymousSession)}
        />
      </MemoryRouter>,
    );

    expect(screen.getByRole("button", { name: "使用 Google 继续" })).toBeInTheDocument();
  });

  it("logs out through the safe route then refreshes the authoritative session", async () => {
    vi.mocked(signOutCurrentSession).mockResolvedValue({ ok: true });
    const refreshSession = vi.fn().mockResolvedValue(anonymousSession);
    const user = userEvent.setup();
    render(
      <MemoryRouter initialEntries={["/account"]}>
        <AppRoutes
          googleSignInEnabled
          session={authenticatedSession}
          refreshSession={refreshSession}
        />
      </MemoryRouter>,
    );

    await user.click(screen.getByRole("button", { name: "退出登录" }));

    expect(signOutCurrentSession).toHaveBeenCalledOnce();
    expect(refreshSession).toHaveBeenCalledOnce();
  });

  it("keeps the Desktop pairing query through email authentication", () => {
    const search =
      "?state=abc&code_challenge=def&code_challenge_method=S256&callback_port=49152&platform=macos&app_version=1.2.3&label=Mac";
    render(
      <MemoryRouter initialEntries={[`/pairing/authorize${search}`]}>
        <AppRoutes
          googleSignInEnabled={false}
          session={anonymousSession}
          refreshSession={vi.fn().mockResolvedValue(authenticatedSession)}
        />
      </MemoryRouter>,
    );
    expect(screen.getByRole("heading", { name: "登录" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "创建账号" })).toHaveAttribute(
      "href",
      `/pairing/authorize${search}#register`,
    );
    expect(screen.queryByRole("button", { name: "使用 Google 继续" })).not.toBeInTheDocument();
  });

  it("renders the pairing confirmation only for an authenticated session", () => {
    render(
      <MemoryRouter initialEntries={["/pairing/authorize?state=request"]}>
        <AppRoutes
          googleSignInEnabled={false}
          session={authenticatedSession}
          refreshSession={vi.fn().mockResolvedValue(authenticatedSession)}
        />
      </MemoryRouter>,
    );
    expect(screen.getByText("Pairing confirmation")).toBeInTheDocument();
  });

  it("keeps the pairing rate-limit result behind the authenticated route", () => {
    const { unmount } = render(
      <MemoryRouter initialEntries={["/pairing/rate-limited"]}>
        <AppRoutes
          googleSignInEnabled={false}
          session={anonymousSession}
          refreshSession={vi.fn().mockResolvedValue(anonymousSession)}
        />
      </MemoryRouter>,
    );
    expect(screen.getByRole("heading", { name: "登录" })).toBeInTheDocument();
    unmount();

    render(
      <MemoryRouter initialEntries={["/pairing/rate-limited"]}>
        <AppRoutes
          googleSignInEnabled={false}
          session={authenticatedSession}
          refreshSession={vi.fn().mockResolvedValue(authenticatedSession)}
        />
      </MemoryRouter>,
    );
    expect(screen.getByText("Pairing rate limited")).toBeInTheDocument();
  });
});
