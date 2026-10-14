import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { MemoryRouter } from "react-router";

import accessI18n from "@/lib/i18n";

import { AuthLayout } from "./auth-layout";
import { PUBLIC_SITE_URL } from "@/lib/public-site";

afterEach(async () => {
  await accessI18n.changeLanguage("zh-CN");
});

describe("auth story localization", () => {
  it("localizes the stories and website links, and supports keyboard navigation", async () => {
    await accessI18n.changeLanguage("en-US");

    render(
      <MemoryRouter>
        <AuthLayout>
          <p>Form content</p>
        </AuthLayout>
      </MemoryRouter>,
    );

    const carousel = screen.getByRole("region", { name: "OneCrew customer perspectives" });
    expect(carousel).toBeInTheDocument();
    expect(carousel.querySelector('[data-active="true"]')).toHaveTextContent("Creator Marketing Lead");
    fireEvent.keyDown(carousel, { key: "ArrowRight" });
    expect(carousel.querySelector('[data-active="true"]')).toHaveTextContent("Growth Lead");
    expect(screen.getByRole("link", { name: "Back to website" })).toHaveAttribute("href", `${PUBLIC_SITE_URL.replace(/\/$/, "")}/en`);
    expect(carousel.querySelectorAll('img')).toHaveLength(5);
  });
});
