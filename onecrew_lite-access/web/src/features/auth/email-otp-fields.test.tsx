import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { EmailOtpFields } from "./email-otp-fields";

const baseProps = {
  email: "alex@example.com",
  otp: "",
  sent: true,
  pending: null,
  onEmailChange: vi.fn(),
  onOtpChange: vi.fn(),
  onSend: vi.fn(),
  onSubmit: vi.fn(),
};

describe("EmailOtpFields", () => {
  it("keeps resend disabled during the 60 second cooldown", () => {
    render(<EmailOtpFields {...baseProps} cooldownSeconds={60} />);

    const resend = screen.getByRole("button", { name: "重新发送验证码（60s）" });
    expect(resend).toBeDisabled();
  });

  it("enables resend after the cooldown expires", () => {
    render(<EmailOtpFields {...baseProps} cooldownSeconds={0} />);

    expect(screen.getByRole("button", { name: "重新发送验证码" })).toBeEnabled();
  });
});
