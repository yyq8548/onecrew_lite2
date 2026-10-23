import i18next from "i18next";
import { beforeAll, describe, expect, it } from "vitest";

import zhCNLeads from "@/i18n/locales/zh-CN/leads";
import type { CreatorImportImpact } from "@/lib/creator";
import type { LeadsT } from "./i18n-labels";
import {
  creatorImportConflictAccountLabel,
  creatorImportConflictLabel,
  creatorImportImpactRows,
} from "./creator-import-model";

let t: LeadsT;

beforeAll(async () => {
  const instance = i18next.createInstance();
  await instance.init({
    lng: "zh-CN",
    ns: ["leads"],
    defaultNS: "leads",
    interpolation: { escapeValue: false },
    resources: { "zh-CN": { leads: zhCNLeads } },
  });
  t = instance.getFixedT("zh-CN", "leads");
});

const emptyImpact: CreatorImportImpact = {
  creator_inputs: 1,
  creators_created: 0,
  creators_merged_by_public_identity: 0,
  creator_scores_filled: 0,
  follower_counts_filled: 0,
  platform_accounts_created: 0,
  claimed_links_created: 0,
  confirmed_links_created: 0,
  rejected_links_created: 0,
  links_promoted_to_confirmed: 0,
  profile_observations_created: 0,
  contact_routes_created: 0,
  creators_unchanged: 0,
  creator_tags_filled: 0,
};

describe("creator import presentation", () => {
  it("shows tag-only import changes and the explicit tag conflict", () => {
    expect(creatorImportImpactRows({ ...emptyImpact, creator_tags_filled: 2 }, t)).toEqual([
      { key: "creator_tags_filled", label: "补齐缺失标签", count: 2 },
    ]);
    expect(creatorImportConflictLabel({
      kind: "creator_tags_conflict", incoming_creator_key: "incoming", local_creator_ids: ["local"],
    }, t)).toBe("导入标签与本地标签不同，需要人工判断");
  });
  it("shows only user-relevant non-zero preview rows", () => {
    expect(
      creatorImportImpactRows({
        ...emptyImpact,
        creators_created: 2,
        profile_observations_created: 3,
      }, t),
    ).toEqual([
      { key: "creators_created", label: "新增 Creator", count: 2 },
      { key: "profile_observations_created", label: "补充账号资料", count: 3 },
    ]);
  });

  it("keeps identity conflicts explicit", () => {
    expect(
      creatorImportConflictLabel({
        kind: "possible_handle_match",
        incoming_creator_key: "incoming",
        account: null,
        local_creator_ids: ["local"],
      }, t),
    ).toBe("发现相同 handle，需要人工判断");
  });

  it("keeps the exact account key needed to resolve a conflict", () => {
    expect(
      creatorImportConflictAccountLabel({
        kind: "account_owned_by_another_creator",
        incoming_creator_key: "incoming",
        account: { platform: "instagram", target_kind: "creator", external_id: "ig-42" },
        local_creator_ids: ["local"],
      }),
    ).toBe("Instagram · creator · ig-42");
  });
});
