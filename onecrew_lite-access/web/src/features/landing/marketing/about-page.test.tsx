import { act, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import i18n from "@/lib/i18n";
import { AboutPage } from "./about-page";

beforeEach(async () => {
  await i18n.changeLanguage("zh-CN");
});

afterEach(() => {
  vi.unstubAllGlobals();
});

function expectRevealedContent(container: HTMLElement) {
  const items = container.querySelectorAll(
    "#brand-facts div[data-reveal], main article[data-reveal], #teams a[data-reveal]",
  );
  expect(items).toHaveLength(10);
  for (const item of items) expect(item).toHaveAttribute("data-visible", "true");
}

describe("AboutPage language switching", () => {
  it.each([false, true])("reveals translated content after switching both ways (initially revealed: %s)", async (initiallyRevealed) => {
    const observers: TestObserver[] = [];
    class TestObserver {
      private readonly targets = new Set<Element>();
      constructor(private readonly callback: IntersectionObserverCallback) {
        observers.push(this);
      }
      observe(target: Element) { this.targets.add(target); }
      unobserve(target: Element) { this.targets.delete(target); }
      disconnect() { this.targets.clear(); }
      reveal() {
        const entries = [...this.targets].map((target) => ({ target, isIntersecting: true }) as IntersectionObserverEntry);
        this.callback(entries, this as unknown as IntersectionObserver);
      }
    }
    vi.stubGlobal("IntersectionObserver", TestObserver);
    const { container } = render(<MemoryRouter><AboutPage /></MemoryRouter>);
    if (initiallyRevealed) {
      observers.forEach((observer) => observer.reveal());
      expectRevealedContent(container);
    }

    for (const language of ["en-US", "zh-CN"] as const) {
      await act(async () => { await i18n.changeLanguage(language); });
      expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
        language === "en-US" ? "Find the right creators." : "找到对的创作者，",
      );
      observers.forEach((observer) => observer.reveal());
      expectRevealedContent(container);
    }
  });

  it("keeps translated content visible when IntersectionObserver is unavailable", async () => {
    // JSDOM has no native observer; the global stub lets teardown restore that state.
    vi.stubGlobal("IntersectionObserver", undefined);
    Reflect.deleteProperty(window, "IntersectionObserver");
    const { container } = render(<MemoryRouter><AboutPage /></MemoryRouter>);
    expectRevealedContent(container);

    for (const language of ["en-US", "zh-CN"] as const) {
      await act(async () => { await i18n.changeLanguage(language); });
      expectRevealedContent(container);
    }
  });
});
