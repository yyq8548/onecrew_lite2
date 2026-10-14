import { describe, expect, it } from "vitest";
import i18next from "i18next";

import { resources } from "@/i18n/resources";

import {
  canContinueFromProfileStep,
  canSubmitXChatPasscodeSetting,
  effectiveXChatConnectionPreference,
  xChatConnectionPayload,
  xChatPasscodeStatusView,
} from "./x-chat-passcode-state";

const testI18n = i18next.createInstance();
void testI18n.init({ initAsync: false, lng: "zh-CN", resources });
const t = testI18n.getFixedT("zh-CN", "accounts");

describe("X Chat connection preference", () => {
  it("requires an explicit X choice before continuing", () => {
    expect(canContinueFromProfileStep({
      platform: "x",
      profileName: "x-primary",
      preference: null,
      passcode: "",
    })).toBe(false);
  });

  it("requires a non-empty passcode when X Chat is enabled", () => {
    expect(canContinueFromProfileStep({
      platform: "x",
      profileName: "x-primary",
      preference: "enabled",
      passcode: "   ",
    })).toBe(false);
    expect(canContinueFromProfileStep({
      platform: "x",
      profileName: "x-primary",
      preference: "enabled",
      passcode: " chat-secret ",
    })).toBe(true);
  });

  it("allows an explicit disabled choice and sends no passcode", () => {
    expect(canContinueFromProfileStep({
      platform: "x",
      profileName: "x-primary",
      preference: "disabled",
      passcode: "ignored",
    })).toBe(true);
    expect(xChatConnectionPayload({
      platform: "x",
      preference: "disabled",
      passcode: "ignored",
    }, t)).toEqual({ xChatPreference: "disabled", xChatPasscode: null });
  });

  it("preserves the enabled passcode exactly for the Tauri handoff", () => {
    expect(xChatConnectionPayload({
      platform: "x",
      preference: "enabled",
      passcode: " chat-secret ",
    }, t)).toEqual({ xChatPreference: "enabled", xChatPasscode: " chat-secret " });
  });

  it("keeps Instagram fixed to disabled", () => {
    expect(effectiveXChatConnectionPreference("instagram", null)).toBe("disabled");
    expect(xChatConnectionPayload({
      platform: "instagram",
      preference: "enabled",
      passcode: "must-not-be-forwarded",
    }, t)).toEqual({ xChatPreference: "disabled", xChatPasscode: null });
  });

  it("requires an explicit setting action and a passcode for set or replace", () => {
    expect(canSubmitXChatPasscodeSetting(null, "secret")).toBe(false);
    expect(canSubmitXChatPasscodeSetting("enabled", " ")).toBe(false);
    expect(canSubmitXChatPasscodeSetting("enabled", "secret")).toBe(true);
    expect(canSubmitXChatPasscodeSetting("disabled", "")).toBe(true);
  });
});

describe("X Chat passcode status view", () => {
  it.each([
    ["configured", "X Chat 已配置", "success"],
    ["not_configured", "X Chat 未配置", "warning"],
    ["unavailable", "X Chat 状态不可用", "destructive"],
    ["not_applicable", "X Chat 不适用", "muted"],
  ] as const)("maps %s to its safe account presentation", (status, label, tone) => {
    expect(xChatPasscodeStatusView(status, t)).toMatchObject({ label, tone });
  });
});
