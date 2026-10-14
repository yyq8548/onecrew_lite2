import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AppRoutes } from "@/access-app";
import i18n from "@/lib/i18n";
import { getDistributionPreview } from "@/lib/distribution-api";
const token = "A".repeat(43);
vi.mock("@/features/account/account-page", () => ({ AccountPage: () => <main>Account home</main> }));
vi.mock("@/lib/distribution-api", async () => ({ ...await vi.importActual("@/lib/distribution-api"), getDistributionPreview: vi.fn() }));
const anonymous = { authenticated: false as const, expiresAt: null, user: null };
const session = { authenticated: true as const, expiresAt: "2999-09-01T00:00:00.000Z", user: { id: "u", name: "Alex", email: "alex@example.com", image: null } };
beforeEach(async () => { vi.mocked(getDistributionPreview).mockReset().mockResolvedValue({ expiresAt: 1893456000000, release: { version: "1.0.0", notes: "", pubDate: "2026-09-06T00:00:00.000Z" }, assets: [] }); await i18n.changeLanguage("zh-CN"); });
function routes(path: string, authenticated = false) { render(<MemoryRouter initialEntries={[path]}><AppRoutes googleSignInEnabled={true} session={authenticated ? session : anonymous} refreshSession={vi.fn()} /></MemoryRouter>); }
describe("distribution login continuation", () => {
  it("renders malformed download links directly without login or a link API call", () => {
    routes("/d/short");
    expect(screen.getByRole("heading", { name: "下载链接格式无效" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "登录" })).not.toBeInTheDocument();
    expect(getDistributionPreview).not.toHaveBeenCalled();
  });
  it("requires login for short links and preserves the path through registration", () => {
    routes(`/d/${token}`);
    expect(screen.getByRole("heading", { name: "登录" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "创建账号" })).toHaveAttribute("href", `/register?${new URLSearchParams({ returnTo: `/d/${token}` })}`);
    expect(screen.queryByRole("heading", { name: "下载 OneCrew" })).not.toBeInTheDocument();
    expect(getDistributionPreview).not.toHaveBeenCalled();
  });
  it("returns authenticated sessions to the validated download destination", () => {
    routes(`/login?${new URLSearchParams({ returnTo: `/d/${token}` })}`, true);
    expect(screen.getByRole("heading", { name: "下载 OneCrew" })).toBeInTheDocument();
  });
  it("discards foreign return destinations", () => {
    routes("/login?returnTo=https%3A%2F%2Fevil.test", true);
    expect(screen.getByText("Account home")).toBeInTheDocument();
  });
  it("preserves the landing application intent across registration", () => {
    routes("/account?apply=1&lng=zh-CN");
    expect(screen.getByRole("link", { name: "创建账号" })).toHaveAttribute("href", "/register?returnTo=%2Faccount%3Fapply%3D1%26lng%3Dzh-CN");
  });
});
