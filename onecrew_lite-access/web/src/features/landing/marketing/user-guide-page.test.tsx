import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { beforeEach, expect, it, vi } from "vitest";
import i18n from "@/lib/i18n";
import { MarketingRoutes } from "./marketing-routes";

beforeEach(async () => {
  await i18n.changeLanguage("zh-CN");
  HTMLDialogElement.prototype.showModal = vi.fn(function(this: HTMLDialogElement) { this.open = true; });
  HTMLDialogElement.prototype.close = vi.fn(function(this: HTMLDialogElement) { this.open = false; });
});
function page() { return render(<MemoryRouter initialEntries={["/resources/user-guide"]}><MarketingRoutes /></MemoryRouter>); }
it.each(["zh-CN", "en-US"])("renders the complete %s guide, eight images and contact details", async language => {
  await i18n.changeLanguage(language); page();
  expect(screen.getByRole("heading", { level: 1, name: language === "en-US" ? "Get started with OneCrew" : "OneCrew 快速开始与使用指南" })).toBeInTheDocument();
  const triggers = screen.getAllByRole("button", { name: /查看大图：|View image:/ });
  expect(triggers).toHaveLength(8);
  expect(new Set(triggers.map(button => button.querySelector("img")?.getAttribute("src"))).size).toBe(8);
  expect(screen.getAllByRole("button", { name: /^复制：|^Copy:/ })).toHaveLength(23);
  expect(screen.getByRole("link", { name: /OneCrew.*X/ })).toHaveAttribute("href", "https://x.com/onecrewai");
  expect(screen.getAllByRole("link", { name: "info@onecrewai.io" })[0]).toHaveAttribute("href", "mailto:info@onecrewai.io");
  expect(document.querySelector('link[rel="canonical"]')).toHaveAttribute("href", `https://onecrewai.io${language === "en-US" ? "" : "/zh"}/resources/user-guide`);
});
it("opens and closes the large image by button, Escape and backdrop, restoring scroll", async () => {
  const user = userEvent.setup(); page();
  document.body.style.overflow = "auto";
  const trigger = screen.getAllByRole("button", { name: /查看大图：/ })[0];
  for (const close of ["button", "escape", "backdrop"]) {
    await user.click(trigger);
    const dialog = screen.getByRole("dialog");
    expect(dialog.querySelector(".largeImage, img")).toBeInTheDocument();
    expect(document.body.style.overflow).toBe("hidden");
    if (close === "button") await user.click(screen.getByRole("button", { name: "关闭" }));
    else if (close === "escape") fireEvent(dialog, new Event("cancel"));
    else fireEvent.click(dialog);
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(document.body.style.overflow).toBe("auto");
  }
  document.body.style.overflow = "";
});
it("reports successful and blocked clipboard access without executing prompts", async () => {
  const user = userEvent.setup(); page();
  const copy = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
  const button = screen.getAllByRole("button", { name: /^复制：/ })[0];
  await user.click(button);
  expect(await screen.findByRole("status")).toHaveTextContent("已复制");
  expect(copy).toHaveBeenCalledTimes(1);
  copy.mockRejectedValue(new Error("blocked"));
  await user.click(button);
  expect(await screen.findByRole("status")).toHaveTextContent("手动复制");
});
