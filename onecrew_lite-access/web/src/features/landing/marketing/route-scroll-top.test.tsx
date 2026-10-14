import { act, fireEvent, render, screen } from "@testing-library/react";
import { Link, MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";

import { RouteScrollTop } from "./route-scroll-top";

afterEach(() => {
  document.documentElement.classList.remove("route-scroll-instant");
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("RouteScrollTop", () => {
  it("keeps scroll on language changes but resets for a different page", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("requestAnimationFrame", undefined);
    vi.stubGlobal("cancelAnimationFrame", undefined);
    const scroll = vi.spyOn(window, "scrollTo").mockImplementation(() => undefined);
    render(<MemoryRouter initialEntries={["/en"]}>
      <RouteScrollTop />
      <Link to="/zh">Chinese</Link>
      <Link to="/zh/product">Product</Link>
      <Link to="/en/product">English product</Link>
    </MemoryRouter>);
    await act(async () => vi.runAllTimersAsync());
    scroll.mockClear();
    fireEvent.click(screen.getByText("Chinese"));
    await act(async () => vi.runAllTimersAsync());
    expect(scroll).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText("Product"));
    await act(async () => vi.runAllTimersAsync());
    expect(scroll).toHaveBeenCalledWith(0, 0);
    scroll.mockClear();
    fireEvent.click(screen.getByText("English product"));
    await act(async () => vi.runAllTimersAsync());
    expect(scroll).not.toHaveBeenCalled();
  });

  it("temporarily disables smooth scrolling with a class and never writes root style", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("requestAnimationFrame", undefined);
    vi.stubGlobal("cancelAnimationFrame", undefined);
    vi.spyOn(window, "scrollTo").mockImplementation(() => undefined);

    render(<MemoryRouter initialEntries={["/product"]}><RouteScrollTop /></MemoryRouter>);

    expect(document.documentElement).toHaveClass("route-scroll-instant");
    expect(document.documentElement).not.toHaveAttribute("style");

    await act(async () => vi.runAllTimersAsync());

    expect(document.documentElement).not.toHaveClass("route-scroll-instant");
    expect(document.documentElement).not.toHaveAttribute("style");
  });
});
