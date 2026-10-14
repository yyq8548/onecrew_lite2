import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, useLocation } from "react-router";
import { describe, expect, it } from "vitest";

import { PairingRateLimitedPage } from "./pairing-rate-limited-page";

function LocationProbe() {
  return <span data-testid="location">{useLocation().pathname}</span>;
}

describe("Pairing rate-limited page", () => {
  it("shows a retryable explanation without exposing request material", async () => {
    const user = userEvent.setup();
    render(
      <MemoryRouter initialEntries={["/pairing/rate-limited"]}>
        <PairingRateLimitedPage />
        <LocationProbe />
      </MemoryRouter>,
    );
    expect(screen.getByRole("heading", { name: "操作过于频繁" })).toBeInTheDocument();
    expect(screen.getByText("请稍后再试")).toBeInTheDocument();
    expect(screen.getByText(/OneCrew Desktop/)).toBeInTheDocument();
    expect(document.body).not.toHaveTextContent("Agent Platform");
    expect(document.body).not.toHaveTextContent("Reacher");
    expect(document.body).not.toHaveTextContent("approvalToken");
    await user.click(screen.getByRole("button", { name: "返回账号中心" }));
    expect(screen.getByTestId("location")).toHaveTextContent("/account");
  });
});
