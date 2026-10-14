import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DistributionPanel } from "./distribution-panel";
import { DownloadPage } from "./download-page";
import * as api from "@/lib/distribution-api";
import i18n from "@/lib/i18n";
vi.mock("@/lib/distribution-api", async () => ({ ...await vi.importActual("@/lib/distribution-api"), getDistributionApplication: vi.fn(), submitDistributionApplication: vi.fn(), createDistributionLink: vi.fn(), getDistributionPreview: vi.fn(), restoreCliInstallLink: vi.fn() }));
const user = { id: "user", name: "Alex", email: "alex@example.com", image: null };
const empty = { cliRelease: { status: "published" as const, version: "0.5.21", targets: ["aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-pc-windows-msvc"] as api.DistributionApplication["cliRelease"]["targets"] }, applicationsOpen: true, settingRevision: 1, latestRelease: { version: "0.5.17", notes: "", pubDate: "2026-09-06T00:00:00.000Z" }, application: null, entitlement: null, email: user.email };
const application = { id: "a", displayName: "Alex", purpose: "Creator partnerships", company: null, reviewReason: null, revision: 1, createdAt: 1, updatedAt: 1 };
beforeEach(async () => { vi.resetAllMocks(); await i18n.changeLanguage("zh-CN"); vi.mocked(api.getDistributionApplication).mockResolvedValue(empty); vi.mocked(api.restoreCliInstallLink).mockResolvedValue({link:null}); });
function panel(entry = "/account") { return render(<MemoryRouter initialEntries={[entry]}><DistributionPanel user={user} /></MemoryRouter>); }
describe("early access application", () => {
  it("shows application on first screen and autofills only current email and name", async () => {
    panel("/account?apply=1");
    expect(await screen.findByLabelText("登录邮箱")).toHaveValue(user.email);
    expect(screen.getByLabelText("登录邮箱")).toHaveAttribute("readonly");
    expect(screen.getByLabelText("怎么称呼你")).toHaveValue("Alex");
    expect(screen.getByLabelText("主要用途")).toBeRequired();
    expect(screen.queryByLabelText(/电话|地址|身份证/)).not.toBeInTheDocument();
  });
  it("reads back pending state only after a successful submission", async () => {
    panel("/account?apply=1"); await screen.findByLabelText("主要用途");
    vi.mocked(api.submitDistributionApplication).mockResolvedValue({ ...empty, application: { ...application, status: "pending" } });
    vi.mocked(api.getDistributionApplication).mockResolvedValue({ ...empty, application: { ...application, status: "pending" } });
    await userEvent.type(screen.getByLabelText("主要用途"), "Creator partnerships");
    await userEvent.click(screen.getByRole("button", { name: "提交申请" }));
    expect(await screen.findByText("审核中")).toBeInTheDocument();
    expect(api.getDistributionApplication).toHaveBeenCalledTimes(2);
    expect(api.submitDistributionApplication).toHaveBeenCalledWith({ displayName: "Alex", purpose: "Creator partnerships", company: "" }, expect.any(String));
  });
  it("reuses an idempotency key after an uncertain submission", async () => {
    panel("/account?apply=1"); await screen.findByLabelText("主要用途");
    vi.mocked(api.submitDistributionApplication).mockRejectedValue(new Error("network"));
    await userEvent.type(screen.getByLabelText("主要用途"), "Creator partnerships");
    await userEvent.click(screen.getByRole("button", { name: "提交申请" })); await screen.findByRole("alert");
    await userEvent.click(screen.getByRole("button", { name: "提交申请" }));
    await waitFor(() => expect(api.submitDistributionApplication).toHaveBeenCalledTimes(2));
    expect(vi.mocked(api.submitDistributionApplication).mock.calls[0][1]).toBe(vi.mocked(api.submitDistributionApplication).mock.calls[1][1]);
  });
  it.each(["response", "readback"] as const)("starts a new intent after an unknown %s is confirmed pending then rejected", async failure => {
    panel("/account?apply=1"); await screen.findByLabelText("主要用途");
    const pending = { ...empty, application: { ...application, status: "pending" as const } };
    if (failure === "response") {
      vi.mocked(api.submitDistributionApplication).mockRejectedValueOnce(new Error("response lost after commit"));
    } else {
      vi.mocked(api.submitDistributionApplication).mockResolvedValueOnce(pending);
      vi.mocked(api.getDistributionApplication).mockRejectedValueOnce(new Error("readback failed"));
    }
    await userEvent.type(screen.getByLabelText("主要用途"), application.purpose);
    await userEvent.click(screen.getByRole("button", { name: "提交申请" }));
    await screen.findByRole("alert");
    const firstCall = vi.mocked(api.submitDistributionApplication).mock.calls[0];
    await userEvent.click(screen.getByRole("button", { name: "取消" }));

    vi.mocked(api.getDistributionApplication).mockResolvedValue(pending);
    await userEvent.click(screen.getByRole("button", { name: "刷新状态" }));
    expect(await screen.findByText("审核中", { selector: '[data-slot="badge"]' })).toBeInTheDocument();
    vi.mocked(api.getDistributionApplication).mockResolvedValue({ ...empty, application: { ...application, status: "rejected", revision: 2 } });
    await userEvent.click(screen.getByRole("button", { name: "刷新状态" }));
    await userEvent.click(await screen.findByRole("button", { name: "更新申请" }));
    expect(screen.getByLabelText("主要用途")).toHaveValue(application.purpose);

    const reapplied = { ...empty, application: { ...application, status: "pending" as const, revision: 3 } };
    vi.mocked(api.submitDistributionApplication).mockResolvedValue(reapplied);
    vi.mocked(api.getDistributionApplication).mockResolvedValue(reapplied);
    await userEvent.click(screen.getByRole("button", { name: "提交申请" }));
    expect(await screen.findByText("审核中", { selector: '[data-slot="badge"]' })).toBeInTheDocument();
    const secondCall = vi.mocked(api.submitDistributionApplication).mock.calls[1];
    expect(secondCall[0]).toEqual(firstCall[0]);
    expect(secondCall[1]).not.toBe(firstCall[1]);
  });
  it("keeps the original key when a refresh cannot establish a newer application revision", async () => {
    const rejected = { ...empty, application: { ...application, status: "rejected" as const, revision: 2 } };
    vi.mocked(api.getDistributionApplication).mockResolvedValue(rejected);
    vi.mocked(api.submitDistributionApplication).mockRejectedValue(new Error("unknown"));
    panel("/account?apply=1"); await screen.findByLabelText("主要用途");
    await userEvent.click(screen.getByRole("button", { name: "提交申请" })); await screen.findByRole("alert");
    await userEvent.click(screen.getByRole("button", { name: "取消" }));
    await userEvent.click(screen.getByRole("button", { name: "刷新状态" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "刷新状态" })).toBeEnabled());
    await userEvent.click(screen.getByRole("button", { name: "更新申请" }));
    await userEvent.click(screen.getByRole("button", { name: "提交申请" }));
    await waitFor(() => expect(api.submitDistributionApplication).toHaveBeenCalledTimes(2));
    expect(vi.mocked(api.submitDistributionApplication).mock.calls[1][1]).toBe(vi.mocked(api.submitDistributionApplication).mock.calls[0][1]);
  });
  it("does not reuse an uncertain intent for a different signed-in user", async () => {
    const rejected = { ...empty, application: { ...application, status: "rejected" as const, revision: 2 } };
    vi.mocked(api.getDistributionApplication).mockResolvedValue(rejected);
    vi.mocked(api.submitDistributionApplication).mockRejectedValue(new Error("unknown"));
    const view = panel("/account?apply=1"); await screen.findByLabelText("主要用途");
    await userEvent.click(screen.getByRole("button", { name: "提交申请" })); await screen.findByRole("alert");
    view.rerender(<MemoryRouter initialEntries={["/account?apply=1"]}><DistributionPanel user={{ ...user, id: "different-user" }} /></MemoryRouter>);
    await screen.findByLabelText("主要用途");
    await userEvent.click(screen.getByRole("button", { name: "提交申请" }));
    await waitFor(() => expect(api.submitDistributionApplication).toHaveBeenCalledTimes(2));
    expect(vi.mocked(api.submitDistributionApplication).mock.calls[1][0]).toEqual(vi.mocked(api.submitDistributionApplication).mock.calls[0][0]);
    expect(vi.mocked(api.submitDistributionApplication).mock.calls[1][1]).not.toBe(vi.mocked(api.submitDistributionApplication).mock.calls[0][1]);
  });
  it.each(["pending", "rejected", "approved", "revoked"] as const)("renders the %s server state", async status => {
    vi.mocked(api.getDistributionApplication).mockResolvedValue({ ...empty, application: { ...application, status: status === "revoked" ? "approved" : status }, entitlement: status === "revoked" ? { status: "revoked", revision: 2 } : status === "approved" ? { status: "active", revision: 1 } : null });
    panel(); const labels = { pending: "审核中", rejected: "申请未通过", approved: "已获批", revoked: "体验权限已撤销" };
    expect(await screen.findByText(labels[status], { selector: '[data-slot="badge"]' })).toBeInTheDocument();
    expect(screen.queryByText(application.purpose)).not.toBeInTheDocument();
    if (status === "approved") expect(await screen.findByRole("button", { name: "生成并复制安装提示词" })).toBeEnabled();
    else expect(screen.queryByRole("button", { name: "生成并复制安装提示词" })).not.toBeInTheDocument();
  });
  it("shows the user-visible rejection reason while preserving the application for editing", async () => {
    const reviewReason = "请补充计划合作的创作者类型。\n<img src=x onerror=alert(1)>";
    vi.mocked(api.getDistributionApplication).mockResolvedValue({ ...empty, application: { ...application, status: "rejected", reviewReason } });
    panel("/account?apply=1");
    expect(await screen.findByRole("heading", { name: "审核原因" })).toBeInTheDocument();
    expect(screen.getByText(/请补充计划合作的创作者类型/)).toHaveTextContent("<img src=x onerror=alert(1)>");
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.getByLabelText("主要用途")).toHaveValue(application.purpose);
    expect(screen.getByLabelText("怎么称呼你")).toHaveValue(application.displayName);
  });
  it("retains typed fields when applications close and restores submission when they reopen", async () => {
    panel("/account?apply=1"); await screen.findByLabelText("主要用途");
    await userEvent.type(screen.getByLabelText("主要用途"), "Keep this draft after the switch changes");
    vi.mocked(api.getDistributionApplication).mockResolvedValue({ ...empty, applicationsOpen: false, settingRevision: 2 });
    await userEvent.click(screen.getByRole("button", { name: "刷新状态" }));
    expect(await screen.findByRole("button", { name: "申请暂未开放" })).toBeDisabled();
    expect(screen.queryByLabelText("主要用途")).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: "查看使用指南" })).toHaveAttribute("href", "https://onecrewai.io/zh/resources/user-guide");
    expect(api.submitDistributionApplication).not.toHaveBeenCalled();
    vi.mocked(api.getDistributionApplication).mockResolvedValue({ ...empty, applicationsOpen: true, settingRevision: 3 });
    await userEvent.click(screen.getByRole("button", { name: "刷新状态" }));
    expect(await screen.findByRole("button", { name: "提交申请" })).toBeEnabled();
    expect(screen.getByLabelText("主要用途")).toHaveValue("Keep this draft after the switch changes");
  });
  it("honors a server-side closure race and does not reopen after a failed refresh", async () => {
    panel("/account?apply=1"); await screen.findByLabelText("主要用途");
    await userEvent.type(screen.getByLabelText("主要用途"), application.purpose);
    vi.mocked(api.submitDistributionApplication).mockRejectedValue(new api.DistributionApiError(403, "applications_closed"));
    await userEvent.click(screen.getByRole("button", { name: "提交申请" }));
    expect(await screen.findByRole("button", { name: "申请暂未开放" })).toBeDisabled();
    vi.mocked(api.getDistributionApplication).mockRejectedValue(new Error("network"));
    await userEvent.click(screen.getByRole("button", { name: "刷新状态" }));
    await screen.findByRole("alert");
    expect(screen.getByRole("button", { name: "申请暂未开放" })).toBeDisabled();
    expect(screen.queryByLabelText("主要用途")).not.toBeInTheDocument();
  });
  it("keeps rejection feedback and the application visible while reapplications are closed", async () => {
    vi.mocked(api.getDistributionApplication).mockResolvedValue({ ...empty, applicationsOpen: false, application: { ...application, status: "rejected", reviewReason: "Please add your intended use." } });
    panel();
    expect(await screen.findByText("Please add your intended use.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "申请暂未开放" })).toBeDisabled();
    expect(screen.queryByText(application.purpose)).not.toBeInTheDocument();
  });
  it("keeps approved download access available while applications are closed", async () => {
    vi.mocked(api.getDistributionApplication).mockResolvedValue({ ...empty, applicationsOpen: false, application: { ...application, status: "approved" }, entitlement: { status: "active", revision: 1 } });
    panel(); expect(await screen.findByRole("button", { name: "生成并复制安装提示词" })).toBeEnabled();
  });
  it("shows the CLI release without inventing link metadata during an account refresh", async () => {
    const approved = { ...empty, latestRelease: null, application: { ...application, status: "approved" as const }, entitlement: { status: "active" as const, revision: 1 } };
    vi.mocked(api.getDistributionApplication).mockResolvedValue(approved);
    panel(); expect(await screen.findByRole("button", { name: "生成并复制安装提示词" })).toBeEnabled();
    expect(screen.getByText("安装包准备中")).toBeInTheDocument();
    expect(screen.getByText("OneCrew CLI 0.5.21")).toBeInTheDocument();
    expect(screen.getByText(/macOS · Apple Silicon/)).toHaveTextContent("Windows · x64");
    vi.mocked(api.getDistributionApplication).mockResolvedValue(approved);
    await userEvent.click(screen.getByRole("button", {name:"刷新状态"}));
    expect(screen.queryByText(/私人链接有效期至/)).not.toBeInTheDocument();
    expect(screen.queryByText(/刷新页面后无法恢复/)).not.toBeInTheDocument();
    expect(api.createDistributionLink).not.toHaveBeenCalled();
  });
  it("allows application collection before a package is published when the application gate is open", async () => {
    vi.mocked(api.getDistributionApplication).mockResolvedValue({ ...empty, latestRelease: null });
    panel(); expect(await screen.findByRole("button", { name: "申请体验" })).toBeEnabled();
  });
  it("does not issue a ticket until an approved user requests it", async () => {
    vi.mocked(api.getDistributionApplication).mockResolvedValue({ ...empty, application: { ...application, status: "approved" }, entitlement: { status: "active", revision: 1 } });
    panel(); await screen.findByRole("button", { name: "生成并复制安装提示词" });
    expect(api.createDistributionLink).not.toHaveBeenCalled();
  });
  it("retries an uncertain desktop link with the same request id", async () => {
    vi.mocked(api.getDistributionApplication).mockResolvedValue({ ...empty, application: { ...application, status: "approved" }, entitlement: { status: "active", revision: 1 } });
    vi.mocked(api.createDistributionLink).mockRejectedValueOnce(new Error("response lost"));
    vi.mocked(api.createDistributionLink).mockResolvedValueOnce({ url: `/d/${"A".repeat(43)}`, expiresAt: 1893456000000 });
    panel();
    await userEvent.click(await screen.findByRole("button", { name: "前往下载" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("暂时无法生成下载链接");
    await userEvent.click(screen.getByRole("button", { name: "前往下载" }));
    await waitFor(() => expect(api.createDistributionLink).toHaveBeenCalledTimes(2));
    expect(vi.mocked(api.createDistributionLink).mock.calls[1]![0]).toBe(vi.mocked(api.createDistributionLink).mock.calls[0]![0]);
  });
});
describe("download page", () => {
  it("offers a published Windows setup as a direct download", async () => {
    const token = "A".repeat(43);
    vi.mocked(api.getDistributionPreview).mockResolvedValue({
      expiresAt: 1893456000000,
      release: { version: "0.6.3", notes: "", pubDate: "2026-09-24T00:00:00Z" },
      assets: [{ id: "setup", filename: "OneCrew-Setup-0.6.3-windows-x64.exe", platform: "windows", arch: "x86_64", kind: "installer", size: 100, downloadUrl: `/api/distribution/download-links/${token}/assets/setup` }],
    });
    render(<MemoryRouter initialEntries={[`/d/${token}`]}><Routes><Route path="/d/:token" element={<DownloadPage user={user} refreshSession={vi.fn()} />} /></Routes></MemoryRouter>);
    expect(await screen.findByText("Windows · x64")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "下载安装包" })).toHaveAttribute("href", `/api/distribution/download-links/${token}/assets/setup`);
  });
  it("shows an invalid short-link shell without requesting the strict token API", async () => {
    render(<MemoryRouter initialEntries={["/d/too-short"]}><Routes><Route path="/d/:token" element={<DownloadPage user={user} refreshSession={vi.fn()} />} /></Routes></MemoryRouter>);
    expect(screen.getByRole("heading", { name: "下载链接格式无效" })).toBeInTheDocument();
    expect(api.getDistributionPreview).not.toHaveBeenCalled();
    expect(screen.queryByRole("link", { name: "下载安装包" })).not.toBeInTheDocument();
  });
  it.each([["link_expired", "下载链接已过期"], ["entitlement_revoked", "下载权限已撤销"], ["release_unavailable", "安装包尚未发布"], ["link_not_found", "下载链接不可用"], ["rate_limited", "暂时无法验证下载权限"]] as const)("renders %s without a download action", async (code, title) => {
    vi.mocked(api.getDistributionPreview).mockRejectedValue(new api.DistributionApiError(404, code));
    render(<MemoryRouter initialEntries={[`/d/${"A".repeat(43)}`]}><Routes><Route path="/d/:token" element={<DownloadPage user={user} refreshSession={vi.fn()} />} /></Routes></MemoryRouter>);
    expect(await screen.findByRole("heading", { name: title })).toBeInTheDocument(); expect(screen.queryByRole("link", { name: "下载安装包" })).not.toBeInTheDocument();
  });
  it("renders English through the same locale system", async () => {
    await i18n.changeLanguage("en-US"); panel(); expect(await screen.findByRole("button", { name: "Apply for early access" })).toBeInTheDocument();
  });
});
