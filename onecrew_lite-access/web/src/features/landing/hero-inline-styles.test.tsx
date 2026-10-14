import { act, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { HeroHeadline } from "./hero-headline";
import { HeroPromptDemo } from "./hero-prompt-demo";

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("landing hero CSP-safe animation state", () => {
  it("keeps alternating both headline brands across multiple cycles", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: false })));
    const { container, unmount } = render(<HeroHeadline locale="zh" />);
    for (const phase of [1, 0, 1, 0]) {
      await act(() => vi.advanceTimersByTimeAsync(2600));
      expect(container.querySelector('.hero-ai-roller')).toHaveAttribute('data-phase', String(phase));
    }
    unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("automatically replays typing and creator reveals through a full layout cycle", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: false })));
    const { container, unmount } = render(<HeroPromptDemo />);
    const firstBurst = container.querySelector('.hero-kol-burst');
    for (let round = 0; round <= 6; round++) {
      await act(() => vi.advanceTimersByTimeAsync(5000));
      expect(container.querySelector('.hero-kol-burst')).toHaveClass('is-showing');
      expect(container.querySelector(`.hero-kol-layout-${round % 3}-one`)).toBeInTheDocument();
      expect(container.querySelectorAll('.hero-kol-card')).toHaveLength(5);
      expect(container.querySelector('[style]')).toBeNull();
      if (round === 6) expect(container.querySelector('.hero-kol-burst')).not.toBe(firstBurst);
      await act(() => vi.advanceTimersByTimeAsync(6600));
      expect(container.querySelector('.hero-prompt-text')).toHaveClass('is-typing');
    }
    unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("animates through finite classes without adding style attributes", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: false })));

    const { container } = render(<><HeroHeadline locale="en" /><HeroPromptDemo /></>);
    expect(container.querySelector("[style]")).toBeNull();
    expect(container.querySelector(".hero-ai-roller-phase-0")).toHaveAttribute("data-phase", "0");

    await act(async () => vi.advanceTimersByTimeAsync(5_000));

    expect(container.querySelector(".hero-ai-roller-phase-1")).toHaveAttribute("data-phase", "1");
    expect(container.querySelectorAll(".hero-kol-card")).toHaveLength(5);
    expect(container.querySelector(".hero-kol-layout-0-one.hero-kol-delay-0")).toBeInTheDocument();
    expect(container.querySelector("[style]")).toBeNull();

    fireEvent.click(container.querySelector(".hero-prompt-run") as HTMLButtonElement);
    await act(async () => vi.advanceTimersByTimeAsync(5_000));
    expect(container.querySelector(".hero-kol-layout-1-one.hero-kol-delay-0")).toBeInTheDocument();
    expect(container.querySelector("[style]")).toBeNull();
  });

  it("keeps Codex and resolves the prompt immediately for reduced motion", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: true })));

    const { container } = render(<><HeroHeadline locale="zh" /><HeroPromptDemo /></>);
    await act(async () => undefined);

    expect(container.querySelector(".hero-ai-roller-phase-0")).toHaveAttribute("data-phase", "0");
    expect(container.querySelectorAll(".hero-kol-card")).toHaveLength(5);
    expect(container.querySelector("[style]")).toBeNull();
    await act(() => vi.advanceTimersByTimeAsync(30000));
    expect(container.querySelector('.hero-kol-layout-0-one')).toBeInTheDocument();
    expect(container.querySelector('.hero-kol-burst')).toHaveClass('is-showing');
    expect(vi.getTimerCount()).toBe(0);
  });

  it("stops loops immediately when the motion preference changes", async () => {
    vi.useFakeTimers();
    const listeners = new Set<() => void>();
    const media = { matches: false, addEventListener: (_: string, callback: () => void) => listeners.add(callback), removeEventListener: (_: string, callback: () => void) => listeners.delete(callback) };
    vi.stubGlobal('matchMedia', vi.fn(() => media));
    const { container, unmount } = render(<><HeroHeadline locale="zh" /><HeroPromptDemo /></>);
    await act(() => vi.advanceTimersByTimeAsync(3000));
    await act(async () => { media.matches = true; listeners.forEach(listener => listener()); });
    expect(container.querySelector('.hero-ai-roller')).toHaveAttribute('data-phase', '0');
    expect(container.querySelector('.hero-prompt-text')).not.toHaveClass('is-typing');
    expect(vi.getTimerCount()).toBe(0);
    await act(async () => { media.matches = false; listeners.forEach(listener => listener()); });
    expect(container.querySelector('.hero-prompt-text')).toHaveClass('is-typing');
    expect(vi.getTimerCount()).toBeGreaterThan(0);
    unmount();
    expect(listeners.size).toBe(0);
    expect(vi.getTimerCount()).toBe(0);
  });
});
