import type { Platform, XChatPasscodeStatus } from "@/types";
import type { TFunction } from "i18next";

export type XChatConnectionPreference = "enabled" | "disabled";
export type XChatConnectionChoice = XChatConnectionPreference | null;

export type XChatPasscodeStatusView = {
  label: string;
  description: string;
  tone: "success" | "warning" | "destructive" | "muted";
};

export function xChatPasscodeStatusView(
  status: XChatPasscodeStatus,
  t: TFunction<"accounts">,
): XChatPasscodeStatusView {
  switch (status) {
    case "configured":
      return {
        label: t("xChat.status.configured.label"),
        description: t("xChat.status.configured.description"),
        tone: "success",
      };
    case "not_configured":
      return {
        label: t("xChat.status.notConfigured.label"),
        description: t("xChat.status.notConfigured.description"),
        tone: "warning",
      };
    case "unavailable":
      return {
        label: t("xChat.status.unavailable.label"),
        description: t("xChat.status.unavailable.description"),
        tone: "destructive",
      };
    case "not_applicable":
      return {
        label: t("xChat.status.notApplicable.label"),
        description: t("xChat.status.notApplicable.description"),
        tone: "muted",
      };
  }
}

export function effectiveXChatConnectionPreference(
  platform: Platform,
  choice: XChatConnectionChoice,
): XChatConnectionChoice {
  return platform === "x" ? choice : "disabled";
}

export function canContinueFromProfileStep({
  platform,
  profileName,
  preference,
  passcode,
}: {
  platform: Platform;
  profileName: string;
  preference: XChatConnectionChoice;
  passcode: string;
}): boolean {
  if (!profileName.trim()) return false;
  const effective = effectiveXChatConnectionPreference(platform, preference);
  return effective === "disabled" || (effective === "enabled" && Boolean(passcode.trim()));
}

export function xChatConnectionPayload({
  platform,
  preference,
  passcode,
}: {
  platform: Platform;
  preference: XChatConnectionChoice;
  passcode: string;
}, t: TFunction<"accounts">): {
  xChatPreference: XChatConnectionPreference;
  xChatPasscode: string | null;
} {
  const effective = effectiveXChatConnectionPreference(platform, preference);
  if (!effective) throw new Error(t("wizard.validation.selectXChat"));
  if (effective === "disabled") {
    return { xChatPreference: "disabled", xChatPasscode: null };
  }
  if (!passcode.trim()) throw new Error(t("wizard.validation.passcodeRequired"));
  return { xChatPreference: "enabled", xChatPasscode: passcode };
}

export function canSubmitXChatPasscodeSetting(
  preference: XChatConnectionChoice,
  passcode: string,
): boolean {
  return preference === "disabled" || (preference === "enabled" && Boolean(passcode.trim()));
}
