import { describe, expect, it, vi } from "vitest";

const signInSocial = vi.hoisted(() => vi.fn().mockResolvedValue({ error: null }));

vi.mock("better-auth/react", () => ({
  createAuthClient: vi.fn(() => ({
    signIn: { social: signInSocial },
  })),
}));

import * as publicClient from "./auth-client";

describe("Better Auth browser boundary", () => {
  it("exports only a fixed Google social-start operation", async () => {
    await publicClient.startGoogleOAuth();

    expect(Object.keys(publicClient)).toEqual(["startGoogleOAuth"]);
    expect(signInSocial).toHaveBeenCalledWith({
      provider: "google",
    });
  });
});

describe("Google safe distribution return", () => {
  it("passes only a validated internal returnTo to the auth route", async () => {
    const path = `/d/${"A".repeat(43)}`;
    await publicClient.startGoogleOAuth(path);
    expect(signInSocial).toHaveBeenLastCalledWith({ provider: "google", returnTo: path });
    await publicClient.startGoogleOAuth("https://evil.test");
    expect(signInSocial).toHaveBeenLastCalledWith({ provider: "google" });
  });
});
