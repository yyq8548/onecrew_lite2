import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, useLocation } from "react-router";
import { describe, expect, it, vi } from "vitest";

import i18n from "@/lib/i18n";

import { LandingPage } from "./landing-page";
import { FooterAiLinks } from "./footer-ai-links";

function LocationProbe() {
  const location = useLocation();
  return <output aria-label="current URL">{`${location.pathname}${location.search}${location.hash}`}</output>;
}

describe("LandingPage", () => {
  it("links the user guide from the hero, resource dropdown, mobile menu and footer", async () => {
    const { container } = render(<MemoryRouter><LandingPage /></MemoryRouter>);
    const links = container.querySelectorAll('a[href$="/resources/user-guide"]');
    expect(links).toHaveLength(4);
    expect(container.querySelector('.hero-new-actions a[href$="/resources/user-guide"]')).not.toBeNull();
    expect(container.querySelector('.mobile-menu a[href$="/resources/user-guide"]')).not.toBeNull();
    expect(container.querySelector('.site-footer a[href$="/resources/user-guide"]')).not.toBeNull();
    await userEvent.click(screen.getByText("资源", { selector: "summary" }));
    expect(container.querySelector('.nav-resources-menu a[href$="/resources/user-guide"]')).toBeVisible();
    await userEvent.keyboard("{Escape}");
    expect(container.querySelector('.nav-resources-menu a[href$="/resources/user-guide"]')).not.toBeVisible();
  });

  it.each([
    ["zh-CN", "博客"],
    ["en-US", "Blog"],
  ] as const)("links the blog from the desktop nav, mobile menu and footer in %s", async (language, label) => {
    await i18n.changeLanguage(language);
    try {
      const { container } = render(<MemoryRouter><LandingPage /></MemoryRouter>);
      for (const region of [".desktop-nav", ".mobile-menu", ".site-footer"]) {
        const blogLinks = container.querySelectorAll(`${region} a[href="https://blog.onecrewai.io"]`);
        expect(blogLinks).toHaveLength(1);
        expect(blogLinks[0]).toHaveTextContent(label);
        expect(blogLinks[0]).not.toHaveAttribute("target");
      }
    } finally {
      await i18n.changeLanguage("zh-CN");
    }
  });

  it("publishes an Organization and price-free SoftwareApplication schema from real site facts", () => {
    render(<MemoryRouter><LandingPage /></MemoryRouter>);

    const schema = JSON.parse(document.querySelector('script[type="application/ld+json"]')?.textContent ?? "null");
    expect(schema["@graph"]).toContainEqual({
      "@type": "Organization",
      name: "OneCrew",
      email: "info@onecrewai.io",
      sameAs: ["https://x.com/onecrewai"],
      url: "https://onecrewai.io/",
      logo: "https://onecrewai.io/onecrew-ai-mark.png",
    });
    const software = schema["@graph"].find((item: { "@type": string }) => item["@type"] === "SoftwareApplication");
    expect(software).toMatchObject({ name: "OneCrew", operatingSystem: "Desktop" });
    expect(software).not.toHaveProperty("offers");
    expect(software).not.toHaveProperty("price");
  });

  it("keeps the OneCrew homepage and sends every access CTA to the actual application entry", () => {
    render(
      <MemoryRouter>
        <LandingPage />
      </MemoryRouter>,
    );

    expect(
      screen.getByRole("heading", { name: "让 Codex 或 Claude 帮你找到对的人" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "传统外联靠人拼接OneCrew 由 AI 推进" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "3步开始达人合作" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "OneCrew支持哪些渠道" })).toBeInTheDocument();
    const faq = screen.getByRole("region", { name: /关于 OneCrew/ });
    expect(within(faq).getAllByRole("button")).toHaveLength(9);
    const requestLinks = screen.getAllByRole("link", { name: /立即体验/ });
    expect(requestLinks.length).toBeGreaterThan(3);
    for (const link of requestLinks) {
      expect(link).toHaveAttribute("href", "https://access.onecrewai.io/account?apply=1&lng=zh-CN");
    }

    const footer = screen.getByRole("contentinfo");
    const expectedFooterLinks = [
      ["产品", "/zh/product"],
      ["适用团队", "/zh/use-cases"],
      ["关于我们", "/zh/about"],
      ["定价", "/zh/pricing"],
      ["OneCrew vs. Modash", "/zh/compare/modash"],
      ["服务条款", "/zh/terms"],
      ["隐私政策", "/zh/privacy"],
    ] as const;
    for (const [name, href] of expectedFooterLinks) {
      expect(within(footer).getByRole("link", { name })).toHaveAttribute("href", href);
    }
    expect(within(footer).queryByRole("link", { name: "KOL 外联指南" })).not.toBeInTheDocument();
    const contact = footer.querySelector(".footer-legal .footer-contact") as HTMLElement;
    expect(within(contact).getByRole("link", { name: "邮件联系 OneCrew" })).toHaveAttribute("href", "mailto:info@onecrewai.io");
    expect(within(contact).getByRole("link", { name: "OneCrew 官方 X" })).toHaveAttribute("href", "https://x.com/onecrewai");

    expect(screen.queryByText("Request access")).not.toBeInTheDocument();
  });

  it.each([
    ["zh-CN", "在 AI 中了解我们", "我正在评估用于出海达人营销的工具。"],
    ["en-US", "Learn about us with AI", "I’m evaluating tools for global influencer marketing."],
  ] as const)("uses the selected %s language in all five AI prompts", async (language, label, promptStart) => {
    await i18n.changeLanguage(language);
    try {
      render(<MemoryRouter><LandingPage /></MemoryRouter>);
      const footer = screen.getByRole("contentinfo");
      expect(within(footer).getByRole("heading", { name: label })).toBeInTheDocument();
      for (const provider of ["ChatGPT", "Claude", "Gemini", "Grok", "Perplexity"]) {
        const link = within(footer).getByRole("link", { name: new RegExp(provider) });
        const url = new URL(link.getAttribute("href")!);
        if (provider === "Gemini") {
          expect(url.href).toBe("https://gemini.google.com/");
        } else {
          const prompt = url.searchParams.get("prompt") ?? url.searchParams.get("q");
          expect(prompt).toMatch(new RegExp(`^${promptStart}`));
          expect(prompt).toContain("https://onecrewai.io/llms.txt");
          if (provider === "Grok") expect(url.searchParams.has("q")).toBe(true);
        }
        expect(link).toHaveAttribute("target", "_blank");
        expect(link).toHaveAttribute("rel", "noopener noreferrer");
      }
    } finally {
      await i18n.changeLanguage("zh-CN");
    }
  });

  it.each([
    ["zh-CN", "我正在评估用于出海达人营销的工具。", "问题已复制，请在 Gemini 中粘贴。"],
    ["en-US", "I’m evaluating tools for global influencer marketing.", "Prompt copied. Paste it in Gemini."],
  ] as const)("copies the %s question when opening Gemini", async (language, questionStart, feedback) => {
    const user = userEvent.setup();
    const writeText = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
    await i18n.changeLanguage(language);
    try {
      render(<MemoryRouter><FooterAiLinks /></MemoryRouter>);
      await user.click(screen.getByRole("link", { name: /Gemini/ }));
      expect(writeText).toHaveBeenCalledWith(expect.stringContaining(questionStart));
      expect(screen.getByRole("status")).toHaveTextContent(feedback);
    } finally {
      writeText.mockRestore();
      await i18n.changeLanguage("zh-CN");
    }
  });

  it("keeps the Gemini question available when clipboard access fails", async () => {
    const user = userEvent.setup();
    const writeText = vi.spyOn(navigator.clipboard, "writeText").mockRejectedValue(new Error("Clipboard denied"));
    try {
      render(<MemoryRouter><FooterAiLinks /></MemoryRouter>);
      await user.click(screen.getByRole("link", { name: /Gemini/ }));
      const question = screen.getByRole("textbox", { name: "用于 Gemini 的问题" }) as HTMLTextAreaElement;
      expect(question.value).toContain("我正在评估用于出海达人营销的工具。");
      expect(screen.getByRole("status")).not.toHaveTextContent("问题已复制");
    } finally {
      writeText.mockRestore();
    }
  });

  it("uses a crawlable locale link and preserves path query and hash", async () => {
    const user = userEvent.setup();
    render(
      <MemoryRouter initialEntries={["/zh?source=test#hero"]}>
        <LandingPage />
        <LocationProbe />
      </MemoryRouter>,
    );

    await user.click(screen.getByLabelText("切换语言"));
    const englishLink = screen.getByRole("link", { name: "EN" });
    expect(englishLink).toHaveAttribute("href", "/?source=test#hero");
    await user.click(englishLink);
    expect(screen.getByRole("status", { name: "current URL" })).toHaveTextContent("/?source=test#hero");
    expect(
      screen.getByRole("heading", { name: "Turn Codex or Claude into a creator outreach agent" }),
    ).toBeInTheDocument();
    for (const link of screen.getAllByRole("link", { name: /Try OneCrew/ })) {
      expect(link).toHaveAttribute("href", "https://access.onecrewai.io/account?apply=1&lng=en-US");
    }
  });
});
