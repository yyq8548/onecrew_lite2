import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { signInWithEmail, startGoogleSignIn } from "@/lib/auth-actions";

import { LoginForm } from "./login-form";

vi.mock("@/lib/auth-actions", () => ({
  signInWithEmail: vi.fn(),
  startGoogleSignIn: vi.fn(),
}));

describe("LoginForm", () => {
  beforeEach(() => {
    vi.mocked(signInWithEmail).mockResolvedValue({ ok: true });
    vi.mocked(startGoogleSignIn).mockResolvedValue({ ok: true });
  });

  it("submits the labeled email and password fields", async () => {
    const user = userEvent.setup();
    const onAuthenticated = vi.fn();
    render(
      <MemoryRouter>
        <LoginForm googleSignInEnabled onAuthenticated={onAuthenticated} />
      </MemoryRouter>,
    );

    await user.type(screen.getByLabelText("邮箱"), "user@example.com");
    await user.type(screen.getByLabelText("密码"), "password123");
    await user.click(screen.getByRole("button", { name: "登录" }));

    expect(signInWithEmail).toHaveBeenCalledWith({
      email: "user@example.com",
      password: "password123",
    }, "zh");
    expect(onAuthenticated).toHaveBeenCalledOnce();
  });

  it("renders the safe action error and allows another attempt", async () => {
    vi.mocked(signInWithEmail).mockResolvedValue({ ok: false, message: "邮箱或密码不正确。" });
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <LoginForm googleSignInEnabled onAuthenticated={vi.fn()} />
      </MemoryRouter>,
    );

    await user.type(screen.getByLabelText("邮箱"), "user@example.com");
    await user.type(screen.getByLabelText("密码"), "wrong-password");
    await user.click(screen.getByRole("button", { name: "登录" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("邮箱或密码不正确。");
    expect(screen.getByRole("button", { name: "登录" })).toBeEnabled();
  });

  it("shows a disabled Google placeholder when the backend capability is unavailable", () => {
    render(
      <MemoryRouter>
        <LoginForm googleSignInEnabled={false} onAuthenticated={vi.fn()} />
      </MemoryRouter>,
    );

    expect(screen.getByRole("button", { name: "Google 登录（暂未接入）" })).toBeDisabled();
    expect(screen.getByText("或")).toBeInTheDocument();
  });

  it("disables Google without showing its spinner during email login", async () => {
    let resolveLogin: ((value: { ok: true }) => void) | undefined;
    vi.mocked(signInWithEmail).mockReturnValue(
      new Promise((resolve) => {
        resolveLogin = resolve;
      }),
    );
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <LoginForm googleSignInEnabled onAuthenticated={vi.fn()} />
      </MemoryRouter>,
    );

    await user.type(screen.getByLabelText("邮箱"), "user@example.com");
    await user.type(screen.getByLabelText("密码"), "password123");
    await user.click(screen.getByRole("button", { name: "登录" }));

    const googleButton = screen.getByRole("button", { name: "使用 Google 继续" });
    expect(googleButton).toBeDisabled();
    expect(googleButton).toHaveAttribute("aria-busy", "false");
    expect(googleButton.querySelector(".lucide-loader-circle")).toBeNull();

    resolveLogin?.({ ok: true });
  });

  it("shows the Google spinner only for a Google request", async () => {
    vi.mocked(startGoogleSignIn).mockReturnValue(new Promise(() => undefined));
    const user = userEvent.setup();
    render(
      <MemoryRouter>
        <LoginForm googleSignInEnabled onAuthenticated={vi.fn()} />
      </MemoryRouter>,
    );

    const googleButton = screen.getByRole("button", { name: "使用 Google 继续" });
    await user.click(googleButton);

    expect(googleButton).toHaveAttribute("aria-busy", "true");
    expect(googleButton.querySelector(".lucide-loader-circle")).not.toBeNull();
  });
});
