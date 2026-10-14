import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, afterEach, it, expect, vi } from "vitest";
import { CliInstallButton } from "./cli-install-button";
import * as api from "@/lib/distribution-api";
import i18n from "@/lib/i18n";
vi.mock("@/lib/distribution-api", async () => ({ ...await vi.importActual("@/lib/distribution-api"), createCliInstallLink: vi.fn(), restoreCliInstallLink: vi.fn() }));
const release: api.DistributionApplication["cliRelease"] = { status: "published", version: "0.5.22", targets: ["aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-pc-windows-msvc"] };
const link = (): api.CliInstallLink => ({ url: `/cli/install/v1.${"a".repeat(100)}`, expiresAt: Date.now() + 86400000, prompt: "Current server installation instructions" });
afterEach(() => vi.useRealTimers());
beforeEach(async () => { vi.resetAllMocks(); await i18n.changeLanguage("zh-CN"); vi.mocked(api.restoreCliInstallLink).mockResolvedValue({ link: null }); });

it("copies the server prompt and keeps a generated link when clipboard copying fails", async () => {
  const actor = userEvent.setup(), value = link();
  vi.mocked(api.createCliInstallLink).mockResolvedValue(value);
  const write = vi.spyOn(navigator.clipboard, "writeText").mockRejectedValueOnce(new Error("clipboard")).mockResolvedValue(undefined);
  render(<CliInstallButton release={release} />);
  await actor.click(await screen.findByRole("button", { name: "生成并复制安装提示词" })); await screen.findByRole("alert");
  await actor.click(screen.getByRole("button", { name: "再次复制安装提示词" })); await screen.findByText("已复制安装提示词");
  expect(api.createCliInstallLink).toHaveBeenCalledTimes(1);
  expect(write).toHaveBeenLastCalledWith(value.prompt);
  expect(screen.getByText(value.prompt, { selector: "code" })).toBeInTheDocument();
});
it("restores a valid link on reload without generation, copying or a new expiry", async () => {
  const actor = userEvent.setup(), value = link(), write = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue(undefined);
  vi.mocked(api.restoreCliInstallLink).mockResolvedValue({ link: value });
  const first = render(<CliInstallButton release={release} />);
  await screen.findByText(value.prompt, { selector: "code" });
  const expiry = screen.getByText(/私人链接有效期至/).textContent;
  first.unmount(); render(<CliInstallButton release={release} />);
  await screen.findByText(value.prompt, { selector: "code" });
  expect(screen.getByText(/私人链接有效期至/).textContent).toBe(expiry);
  expect(api.createCliInstallLink).not.toHaveBeenCalled(); expect(write).not.toHaveBeenCalled();
  await actor.click(screen.getByRole("button", { name: "再次复制安装提示词" }));
  expect(write).toHaveBeenCalledWith(value.prompt);
  expect(screen.queryByRole("button", { name: /重新生成提示词/ })).not.toBeInTheDocument();
});
it("shows no stale expiry when there is no recoverable cookie", async () => {
  const actor = userEvent.setup(); vi.mocked(api.createCliInstallLink).mockResolvedValue(link());
  render(<CliInstallButton release={release} />);
  const button = await screen.findByRole("button", { name: "生成并复制安装提示词" });
  expect(screen.queryByText(/私人链接有效期|上次提示词|刷新页面后无法恢复/)).not.toBeInTheDocument();
  await actor.click(button); await screen.findByText("已复制安装提示词");
  expect(screen.getByText(/私人链接有效期至/)).toBeInTheDocument();
});
it("restores and copies the translated server prompt when the page language changes", async () => {
  const actor = userEvent.setup(), value = { ...link(), prompt: "请安装 OneCrew" };
  const translated = { ...value, prompt: "Install OneCrew" };
  const write = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue(undefined);
  vi.mocked(api.restoreCliInstallLink).mockResolvedValueOnce({ link: value }).mockResolvedValueOnce({ link: translated });
  render(<CliInstallButton release={release} />);
  await screen.findByText(value.prompt, { selector: "code" });
  expect(screen.getByText("安装提示词", { selector: "strong" })).toBeInTheDocument();
  await act(async () => { await i18n.changeLanguage("en-US"); });
  await screen.findByText(translated.prompt, { selector: "code" });
  expect(screen.queryByText(value.prompt, { selector: "code" })).not.toBeInTheDocument();
  expect(screen.getByText("Installation prompt", { selector: "strong" })).toBeInTheDocument();
  await actor.click(screen.getByRole("button", { name: "Copy installation prompt again" }));
  expect(write).toHaveBeenLastCalledWith(translated.prompt);
  expect(api.restoreCliInstallLink).toHaveBeenCalledTimes(2);
  expect(api.createCliInstallLink).not.toHaveBeenCalled();
});
it("keeps the generate action disabled until restoration settles", async () => {
  let resolve!: (value: { link: api.CliInstallLink | null }) => void;
  vi.mocked(api.restoreCliInstallLink).mockReturnValue(new Promise(done => { resolve = done; }));
  render(<CliInstallButton release={release} />);
  expect(screen.getByRole("button", { name: "恢复中…" })).toBeDisabled();
  await act(async () => resolve({ link: null }));
  expect(screen.getByRole("button", { name: "生成并复制安装提示词" })).toBeEnabled();
  expect(api.createCliInstallLink).not.toHaveBeenCalled();
});
it("ignores a restoration response after the account component is unmounted", async () => {
  let resolve!: (value: { link: api.CliInstallLink | null }) => void;
  vi.mocked(api.restoreCliInstallLink).mockReturnValueOnce(new Promise(done => { resolve = done; }));
  const first = render(<CliInstallButton release={release} />); first.unmount();
  render(<CliInstallButton release={release} />);
  await screen.findByRole("button", { name: "生成并复制安装提示词" });
  await act(async () => resolve({ link: link() }));
  expect(screen.queryByText("Current server installation instructions")).not.toBeInTheDocument();
});
it("allows recovery from a restore error without silently creating a link", async () => {
  vi.mocked(api.restoreCliInstallLink).mockRejectedValue(new Error("network"));
  render(<CliInstallButton release={release} />);
  expect(await screen.findByRole("alert")).toHaveTextContent("暂时无法恢复");
  expect(screen.getByRole("button", { name: "生成并复制安装提示词" })).toBeEnabled();
  expect(api.createCliInstallLink).not.toHaveBeenCalled();
});
it("honors retry-after during restoration without issuing a replacement", async () => {
  vi.mocked(api.restoreCliInstallLink).mockRejectedValue(new api.DistributionApiError(429, "rate_limited", 30));
  render(<CliInstallButton release={release} />);
  expect(await screen.findByRole("alert")).toHaveTextContent("请求过于频繁");
  expect(screen.getByRole("button")).toBeDisabled();
  expect(screen.getByText(/秒后重试/)).toBeInTheDocument();
  expect(api.createCliInstallLink).not.toHaveBeenCalled();
});
it("expires an in-memory link and generates a replacement only after a click", async () => {
  userEvent.setup();
  vi.useFakeTimers();
  const value = { ...link(), expiresAt: Date.now() + 5000 };
  vi.mocked(api.restoreCliInstallLink).mockResolvedValue({ link: value });
  vi.mocked(api.createCliInstallLink).mockResolvedValue({ ...link(), prompt: "Replacement instructions" });
  await act(async () => { render(<CliInstallButton release={release} />); });
  await act(() => vi.advanceTimersByTimeAsync(6000));
  expect(screen.queryByText(value.prompt, { selector: "code" })).not.toBeInTheDocument();
  expect(api.createCliInstallLink).not.toHaveBeenCalled();
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "生成并复制安装提示词" })); });
  expect(api.createCliInstallLink).toHaveBeenCalledTimes(1);
});
it("does not restore or generate for an unpublished release", () => {
  render(<CliInstallButton release={{ status: "unavailable", version: "0.5.22", targets: [] }} />);
  expect(screen.getByRole("button")).toBeDisabled();
  expect(api.restoreCliInstallLink).not.toHaveBeenCalled(); expect(api.createCliInstallLink).not.toHaveBeenCalled();
});
it("honors a server rate limit without deriving cooldown from old database tickets", async () => {
  const actor = userEvent.setup(); vi.mocked(api.createCliInstallLink).mockRejectedValue(new api.DistributionApiError(429, "rate_limited", 60));
  render(<CliInstallButton release={release} />);
  await actor.click(await screen.findByRole("button", { name: "生成并复制安装提示词" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("生成过于频繁");
  expect(screen.getByRole("button")).toBeDisabled(); expect(screen.getByText(/秒后重试/)).toBeInTheDocument();
});
it("reports a release becoming unavailable without copying a link", async () => {
  const actor = userEvent.setup(), write = vi.spyOn(navigator.clipboard, "writeText");
  vi.mocked(api.createCliInstallLink).mockRejectedValue(new api.DistributionApiError(409, "release_unavailable"));
  render(<CliInstallButton release={release} />);
  await actor.click(await screen.findByRole("button", { name: "生成并复制安装提示词" }));
  await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("CLI 版本暂时不可用"));
  expect(write).not.toHaveBeenCalled();
});
