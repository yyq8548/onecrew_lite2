import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, useLocation } from "react-router";
import { beforeEach, expect, it, vi } from "vitest";
import { AccountPage } from "./account-page";
import { AccessLanguageToggle } from "@/features/auth/access-language-toggle";
import { getDistributionApplication, submitDistributionApplication } from "@/lib/distribution-api";
import { loadAccountSettings } from "@/lib/account-actions";
import i18n from "@/lib/i18n";
vi.mock("@/lib/distribution-api", async () => ({ ...await vi.importActual("@/lib/distribution-api"), getDistributionApplication: vi.fn(), submitDistributionApplication: vi.fn() }));
vi.mock("@/lib/account-actions", async () => ({ ...await vi.importActual("@/lib/account-actions"), loadAccountSettings: vi.fn() }));
vi.mock("@/features/account/device-access-panel", () => ({ DeviceAccessPanel: () => <div>Paired device management</div> }));
const user = { id: "u", name: "Actual User", email: "actual@example.com", image: null };
const projection = { cliRelease: { status: "not_published" as const, version: null, targets: [] }, applicationsOpen: true, settingRevision: 1, latestRelease: null, application: null, entitlement: null, email: user.email };
function Location() { const location = useLocation(); return <output data-testid="location">{location.pathname}{location.search}</output>; }
function page(path = "/account") {
  render(<MemoryRouter initialEntries={[path]}><Location /><AccountPage user={user} emailOtpEnabled refreshSession={vi.fn()} /></MemoryRouter>);
}
beforeEach(async () => {
  vi.resetAllMocks(); await i18n.changeLanguage("zh-CN");
  vi.mocked(getDistributionApplication).mockResolvedValue(projection);
  vi.mocked(loadAccountSettings).mockResolvedValue({ hasPassword: true, user });
});
it("defaults to the overview using actual identity and six panel buttons without fake device counts", async () => {
  page("/account?panel=invalid");
  expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Actual User");
  expect(screen.getAllByText(user.email)).toHaveLength(2);
  expect(screen.queryByText(/Alex Chen|3 台设备|当前活跃/)).not.toBeInTheDocument();
  const navigation = screen.getByRole("navigation", { name: "账号设置" });
  expect(within(navigation).getAllByRole("button")).toHaveLength(6);
  await waitFor(() => expect(getDistributionApplication).toHaveBeenCalledTimes(1));
});
it("keeps the application draft while switching panel and language, with apply taking priority", async () => {
  const actor = userEvent.setup(); page("/account?apply=1&panel=profile&lng=zh-CN");
  await screen.findByLabelText("主要用途");
  await actor.type(screen.getByLabelText("主要用途"), "My actual purpose");
  const navigation = screen.getByRole("navigation", { name: "账号设置" });
  await actor.click(within(navigation).getByRole("button", { name: "个人资料" }));
  expect(screen.getByTestId("location")).toHaveTextContent("panel=profile");
  expect(screen.getByTestId("location")).not.toHaveTextContent("apply=");
  await actor.click(screen.getByRole("button", { name: "切换到 English" }));
  expect(screen.getByTestId("location")).toHaveTextContent("panel=profile&lng=en-US");
  await actor.click(within(screen.getByRole("navigation", { name: "Account settings" })).getByRole("button", { name: "Access & downloads" }));
  expect(screen.getByRole("textbox", { name: "What will you use OneCrew for?" })).toHaveValue("My actual purpose");
  expect(submitDistributionApplication).not.toHaveBeenCalled();
});
it("hides closed application fields even with apply=1 and reads current status", async () => {
  vi.mocked(getDistributionApplication).mockResolvedValue({ ...projection, applicationsOpen: false });
  page("/account?apply=1");
  expect(await screen.findByRole("button", { name: "申请暂未开放" })).toBeDisabled();
  expect(screen.queryByLabelText("主要用途")).not.toBeInTheDocument();
  expect(screen.getByRole("link", { name: "查看使用指南" })).toBeInTheDocument();
});
it("updates the login continuation language without losing its account panel", async () => {
  render(<MemoryRouter initialEntries={["/login?returnTo=%2Faccount%3Fpanel%3Demail%26lng%3Dzh-CN"]}><Location /><AccessLanguageToggle /></MemoryRouter>);
  await userEvent.click(screen.getByRole("button"));
  const url = new URL(screen.getByTestId("location").textContent!, "https://access.invalid");
  expect(url.searchParams.get("returnTo")).toBe("/account?panel=email&lng=en-US");
});
