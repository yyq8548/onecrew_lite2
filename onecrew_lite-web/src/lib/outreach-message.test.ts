import { createInstance, type TFunction } from "i18next";
import { beforeAll, describe, expect, it } from "vitest";

import enUSInbox from "@/i18n/locales/en-US/inbox";
import zhCNInbox from "@/i18n/locales/zh-CN/inbox";
import type {
  DirectMessageCapability,
  DirectMessagePreparation,
  RelationshipValue,
} from "@/lib/creator";
import {
  directMessageCapabilityFact,
  followUiState,
  preparationAfterFollow,
} from "@/lib/outreach-message";

let zhT: TFunction<"inbox">;
let enT: TFunction<"inbox">;

beforeAll(async () => {
  const i18n = createInstance();
  await i18n.init({
    fallbackLng: "zh-CN",
    interpolation: { escapeValue: false },
    lng: "zh-CN",
    resources: {
      "zh-CN": { inbox: zhCNInbox },
      "en-US": { inbox: enUSInbox },
    },
  });
  zhT = i18n.getFixedT("zh-CN", "inbox");
  enT = i18n.getFixedT("en-US", "inbox");
});

function preparation(following: RelationshipValue): DirectMessagePreparation {
  return {
    creator_id: "creator-1",
    operator_account: {
      id: "explicit",
      platform: "x",
      kind: "profile",
      label: "Explicit",
      active: true,
      availability: "available",
      created_at: 1,
      updated_at: 1,
    },
    target: {
      id: "target-1",
      account_ref_id: "explicit",
      platform: "x",
      target_kind: "user",
      external_target_id: "target-external",
      metadata: {},
      created_at: 1,
      updated_at: 1,
    },
    relationship: {
      following: {
        id: "following-1",
        actor_account_ref_id: "explicit",
        target_platform_account_id: "target-account",
        kind: "following",
        value: following,
        checked_at: 1,
        updated_at: 1,
      },
      can_message: {
        id: "message-1",
        actor_account_ref_id: "explicit",
        target_platform_account_id: "target-account",
        kind: "can_message",
        value: "yes",
        checked_at: 1,
        updated_at: 1,
      },
    },
    capability: { state: "allowed", scope: "existing_conversation", reason_code: null, detail: null },
    warnings: [],
    confirmation_required: false,
    unfollowed_confirmation_required: false,
    confirmation_reason: "",
  };
}

describe("follow and direct-message capability presentation", () => {
  it("updates only relationship-derived DM preparation fields after follow", () => {
    const current = preparation("no");
    current.warnings = ["provider prose the Web never renders"];
    const next = preparationAfterFollow(current, {
      relationship: { ...current.relationship.following, value: "yes" },
      can_message: current.relationship.can_message,
    });

    expect(next?.relationship.following.value).toBe("yes");
    expect(next?.unfollowed_confirmation_required).toBe(false);
    // Provider prose is passed through untouched: no surface renders it, the
    // next preflight replaces it, and the previous version edited it by
    // comparing against the runtime's exact English sentence.
    expect(next?.warnings).toBe(current.warnings);
    expect(next?.operator_account).toBe(current.operator_account);
    expect(next?.target).toBe(current.target);
    expect(next?.capability).toBe(current.capability);
    expect(preparationAfterFollow(undefined, {
      relationship: current.relationship.following,
      can_message: current.relationship.can_message,
    })).toBeUndefined();
  });

  it("exposes every follow state and treats a refresh as checking", () => {
    expect(followUiState({ preparation: preparation("yes"), preparationFetching: false, preparationError: null, mutationPending: false, mutationError: null })).toBe("yes");
    expect(followUiState({ preparation: preparation("no"), preparationFetching: false, preparationError: null, mutationPending: false, mutationError: null })).toBe("no");
    expect(followUiState({ preparation: preparation("unknown"), preparationFetching: false, preparationError: null, mutationPending: false, mutationError: null })).toBe("unknown");
    expect(followUiState({ preparation: preparation("yes"), preparationFetching: true, preparationError: null, mutationPending: false, mutationError: null })).toBe("checking");
    expect(followUiState({ preparation: preparation("yes"), preparationFetching: false, preparationError: null, mutationPending: true, mutationError: null })).toBe("updating");
    expect(followUiState({ preparation: undefined, preparationFetching: false, preparationError: new Error("probe failed"), mutationPending: false, mutationError: null })).toBe("error");
  });

  it.each<[DirectMessageCapability, string]>([
    [{ state: "unavailable", scope: "new_message_request", reason_code: "target_dm_blocking", detail: "raw" }, "X 返回：该目标当前阻止直接消息。"],
    [{ state: "unavailable", scope: "new_message_request", reason_code: "platform_dm_not_allowed", detail: null }, "X 当前不允许所选身份向该目标发起新的私信请求。"],
    [{ state: "unavailable", scope: "new_message_request", reason_code: "operator_premium_required", detail: null }, "所选 X 身份不满足发起新私聊所需的 Premium 条件。"],
    [{ state: "unavailable", scope: "unknown", reason_code: "operator_chat_passcode_missing", detail: "raw" }, "所选 X 身份尚未配置 X Chat passcode，请到账号页打开 X Chat 设置完成配置。"],
  ])("maps structured capability reason %#", (capability, expected) => {
    expect(directMessageCapabilityFact(capability, "X", zhT)).toBe(expected);
  });

  it("keeps unknown capability sendable while presenting the revalidation fact", () => {
    expect(directMessageCapabilityFact({ state: "unknown", scope: "unknown", reason_code: null, detail: null }, "X", zhT))
      .toBe("平台未能确认权限，发送时仍会重新校验。");
  });

  it("localizes fixed capability guidance while preserving provider detail", () => {
    expect(directMessageCapabilityFact({
      state: "unknown",
      scope: "unknown",
      reason_code: null,
      detail: "RAW_PROVIDER_DETAIL",
    }, "X", enT)).toBe(
      "The platform could not confirm permission and will check again when you send. Platform detail: RAW_PROVIDER_DETAIL",
    );
  });
});
