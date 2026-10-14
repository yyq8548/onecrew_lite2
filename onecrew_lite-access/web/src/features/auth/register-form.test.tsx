import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { signUpWithEmail, startGoogleSignIn, verifyRegistrationEmailOTP, sendRegistrationEmailOTP } from "@/lib/auth-actions";

import { RegisterForm } from "./register-form";

vi.mock("@/lib/auth-actions", () => ({
  signUpWithEmail: vi.fn(),
  startGoogleSignIn: vi.fn(),
  verifyRegistrationEmailOTP: vi.fn(),
  sendRegistrationEmailOTP: vi.fn(),
}));

describe("RegisterForm", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(signUpWithEmail).mockResolvedValue({ ok: true });
    vi.mocked(startGoogleSignIn).mockResolvedValue({ ok: true });
    vi.mocked(verifyRegistrationEmailOTP).mockResolvedValue({ ok: true });
  });

  it("keeps registration behind email verification and the initial resend cooldown", async () => {
    vi.mocked(signUpWithEmail).mockResolvedValue({ ok: true, verificationRequired: true, email: "alex@example.com" });
    const user = userEvent.setup();
    const onAuthenticated = vi.fn();
    render(<MemoryRouter><RegisterForm emailOtpEnabled googleSignInEnabled={false} onAuthenticated={onAuthenticated} /></MemoryRouter>);
    await user.type(screen.getByLabelText("姓名"), "Alex Chen");
    await user.type(screen.getByLabelText("邮箱"), "alex@example.com");
    await user.type(screen.getByLabelText("密码"), "password123");
    await user.type(screen.getByLabelText("确认密码"), "password123");
    await user.click(screen.getByRole("button", { name: "创建账号" }));
    expect(onAuthenticated).not.toHaveBeenCalled();
    expect(await screen.findByRole("button", { name: "重新发送验证码（60s）" })).toBeDisabled();
    expect(sendRegistrationEmailOTP).not.toHaveBeenCalled();
    await user.type(screen.getByLabelText("邮箱验证码"), "123456");
    await user.click(screen.getByRole("button", { name: "验证并继续" }));
    expect(verifyRegistrationEmailOTP).toHaveBeenCalledWith({ email: "alex@example.com", otp: "123456" }, "zh");
    expect(onAuthenticated).toHaveBeenCalledOnce();
  });

  it("rejects mismatched passwords before calling Better Auth", async () => {
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <RegisterForm googleSignInEnabled onAuthenticated={vi.fn()} />
      </MemoryRouter>,
    );

    await user.type(screen.getByLabelText("姓名"), "Alex Chen");
    await user.type(screen.getByLabelText("邮箱"), "alex@example.com");
    await user.type(screen.getByLabelText("密码"), "password123");
    await user.type(screen.getByLabelText("确认密码"), "password456");
    await user.click(screen.getByRole("button", { name: "创建账号" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("两次输入的密码不一致。");
    expect(signUpWithEmail).not.toHaveBeenCalled();
  });

  it("submits a valid registration", async () => {
    const user = userEvent.setup();
    const onAuthenticated = vi.fn();
    render(
      <MemoryRouter>
        <RegisterForm googleSignInEnabled onAuthenticated={onAuthenticated} />
      </MemoryRouter>,
    );

    await user.type(screen.getByLabelText("姓名"), "Alex Chen");
    await user.type(screen.getByLabelText("邮箱"), "alex@example.com");
    await user.type(screen.getByLabelText("密码"), "password123");
    await user.type(screen.getByLabelText("确认密码"), "password123");
    await user.click(screen.getByRole("button", { name: "创建账号" }));

    expect(signUpWithEmail).toHaveBeenCalledWith({
      name: "Alex Chen",
      email: "alex@example.com",
      password: "password123",
    }, "zh");
    expect(onAuthenticated).toHaveBeenCalledOnce();
  });

  it("shows a disabled Google placeholder when Google is unavailable", () => {
    render(
      <MemoryRouter>
        <RegisterForm googleSignInEnabled={false} onAuthenticated={vi.fn()} />
      </MemoryRouter>,
    );

    expect(screen.getByRole("button", { name: "Google 登录（暂未接入）" })).toBeDisabled();
    expect(screen.getByText("或")).toBeInTheDocument();
  });
});
