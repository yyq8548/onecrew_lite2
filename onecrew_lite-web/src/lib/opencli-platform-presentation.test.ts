import { describe, expect, it } from "vitest";
import { openCliPlatformIsGreen, openCliPlatformNeedsSignIn, orderOpenCliPlatforms } from "./opencli-platform-presentation";
import type { OpenCliAuthObservation } from "./opencli-auth";

const browser = { platform: "instagram", command: "instagram/search", browser_required: true, registered: true };
const publicSource = { ...browser, platform: "substack", browser_required: false };
const loggedIn: OpenCliAuthObservation = { platform: "instagram", profile_ref: "selected", status: "logged_in", checked_at: Math.floor(Date.now() / 1000), method: "full", issue: null };
describe("platform connection presentation", () => {
  it("does not ask a customer to sign in again when a check fails or session exists", () => {
    for (const status of ["logged_in", "session_detected", "error", "unknown"] as const) {
      expect(openCliPlatformNeedsSignIn(browser, "selected", { ...loggedIn, status }, false)).toBe(false);
      expect(openCliPlatformNeedsSignIn(browser, "other", { ...loggedIn, status }, false)).toBe(true);
    }
    expect(openCliPlatformNeedsSignIn(browser, "selected", { ...loggedIn, status: "not_logged_in" }, false)).toBe(true);
    expect(openCliPlatformNeedsSignIn(browser, "selected", undefined, true)).toBe(false);
    expect(openCliPlatformNeedsSignIn(publicSource, null, undefined, false)).toBe(false);
  });
  it("puts public sources last, preserving order and the input inventory", () => {
    const items = [publicSource, browser, { ...browser, platform: "youtube" }, { ...publicSource, platform: "bluesky" }];
    expect(orderOpenCliPlatforms(items).map(item => item.platform)).toEqual(["instagram", "youtube", "substack", "bluesky"]);
    expect(items[0]).toBe(publicSource);
  });
  it("marks public paths green without suggesting site login was checked", () => {
    expect(openCliPlatformIsGreen(publicSource, null, undefined, false)).toBe(true);
  });
  it("shows YouTube account login until a full matching login observation is available", () => {
    const youtube = { ...browser, platform: "youtube" };
    expect(openCliPlatformNeedsSignIn(youtube, "selected", undefined, false)).toBe(true);
    expect(openCliPlatformIsGreen(youtube, "selected", undefined, false)).toBe(false);
    expect(openCliPlatformIsGreen(youtube, "selected", { ...loggedIn, platform: "youtube" }, false)).toBe(true);
    expect(openCliPlatformIsGreen(youtube, "selected", { ...loggedIn, platform: "youtube", method: "quick" }, false)).toBe(false);
  });
  it("marks only matching successful login observations green", () => {
    expect(openCliPlatformIsGreen(browser, "selected", loggedIn, false)).toBe(true);
    expect(openCliPlatformIsGreen(browser, "other", loggedIn, false)).toBe(false);
    expect(openCliPlatformIsGreen(browser, null, loggedIn, false)).toBe(false);
    expect(openCliPlatformIsGreen(browser, "selected", { ...loggedIn, platform: "youtube" }, false)).toBe(false);
    expect(openCliPlatformIsGreen(browser, "selected", loggedIn, true)).toBe(false);
    expect(openCliPlatformIsGreen(browser, "selected", { ...loggedIn, method: "quick" }, false)).toBe(false);
    for (const status of ["session_detected", "not_logged_in", "unknown", "error", "public_read"] as const)
      expect(openCliPlatformIsGreen(browser, "selected", { ...loggedIn, status }, false)).toBe(false);
  });
});
