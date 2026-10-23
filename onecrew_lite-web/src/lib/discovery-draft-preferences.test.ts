import { describe, expect, it } from "vitest";
import { DISCOVERY_DEFAULTS, loadDiscoveryPreferences, saveDiscoveryPreferences, discoveryPlanningBrief, discoveryProductBrief, discoveryPlanningScopeOverrides, discoveryEditedScopes, discoveryEditIsComplete } from "./discovery-draft-preferences";

const emptyDraft = { format: 1 as const, brief: "", region: "", language: "", category: "", ...DISCOVERY_DEFAULTS,
  followersEdited: false, keywords: "", v2exNodes: "", tagInputs: {}, categoryInputs: {}, platforms: [], requireDemo: false, requireToolLink: false };
describe("Optional search draft settings", () => {
  it("passes the whole sentence to AI and distinguishes defaults from explicit platform edits", () => {
    const brief = "在tiktok上搜索美妆博主5人";
    expect(discoveryPlanningBrief({ ...emptyDraft, brief, defaultPlatforms: ["youtube"] }))
      .toBe(`${brief}\nDefault search entries when no platform is requested: youtube`);
    expect(discoveryPlanningBrief({ ...emptyDraft, brief, defaultPlatforms: ["youtube"], platformRestriction: ["douyin"] }))
      .toBe(`${brief}\nSearch only these entries: douyin`);
    expect(discoveryProductBrief(`${brief}\nDefault search entries when no platform is requested: youtube`)).toBe(brief);
  });
  it("saves complete edits directly but still asks AI to supply missing fields", () => {
    expect(discoveryEditIsComplete(true, "稳定币", [{ keywords: ["USDT"] }])).toBe(true);
    expect(discoveryEditIsComplete(false, "稳定币", [{ keywords: ["USDT"] }])).toBe(false);
    expect(discoveryEditIsComplete(true, "", [{ keywords: ["USDT"] }])).toBe(false);
    expect(discoveryEditIsComplete(true, "稳定币", [{ keywords: [] }])).toBe(false);
  });
  it("preserves per-site queries when editing another condition, while honoring explicit keywords and tags", () => {
    const previous = [{ platform: "youtube", keywords: ["stablecoin"] }, { platform: "twitter", keywords: ["USDT"] }, { platform: "devto-tag", keywords: ["finance"] }];
    const current = [{ platform: "youtube", keywords: ["stablecoin", "USDT", "finance"] }, { platform: "twitter", keywords: ["stablecoin", "USDT", "finance"] }, { platform: "devto-tag", keywords: ["crypto"] }];
    expect(discoveryEditedScopes(current, previous, "stablecoin, USDT, finance")).toEqual([previous[0], previous[1], current[2]]);
    expect(discoveryEditedScopes(current, previous, "new keyword")).toEqual(current);
    expect(discoveryEditedScopes(current, undefined, "stablecoin, USDT, finance")).toEqual(current);
  });
  it("lets AI expand ordinary query hints but preserves named site scopes", () => {
    const scopes = [
      { platform: "youtube", keywords: ["keyboard review"] },
      { platform: "bilibili", keywords: ["键盘测评"] },
      { platform: "devto-tag", keywords: ["hardware"] },
    ];
    expect(discoveryPlanningScopeOverrides(scopes)).toEqual(scopes.slice(2));
    expect(discoveryPlanningBrief({ ...emptyDraft, brief: "键盘", language: "中文", keywords: "keyboard review" }))
      .toBe("键盘\nContent language: 中文\nPreferred search keywords: keyboard review");
  });
  it("reopens the product without stale generated language or source restrictions", () => {
    expect(discoveryProductBrief("手机支架\nContent language: 英文\nSearch only these entries: youtube")).toBe("手机支架");
    expect(discoveryProductBrief("洗发水\n希望找到测评达人")).toBe("洗发水\n希望找到测评达人");
  });
  it("saves only a follower edit without requiring a product or search scopes", () => {
    let stored = "";
    const storage = { setItem: (_key: string, value: string) => { stored = value; }, getItem: () => stored };
    expect(saveDiscoveryPreferences(storage, { ...emptyDraft, minSubscribers: 10000, followersEdited: true })).toBe(true);
    expect(loadDiscoveryPreferences(storage)).toEqual({ ...emptyDraft, minSubscribers: 10000, followersEdited: true });
    expect(loadDiscoveryPreferences(storage)).not.toHaveProperty("maxCalls");
    stored = JSON.stringify({ ...emptyDraft, maxCalls: 200 });
    expect(loadDiscoveryPreferences(storage)).not.toHaveProperty("maxCalls");
  });
  it("accepts an entirely blank draft and rejects corrupt or future formats", () => {
    expect(saveDiscoveryPreferences({ setItem: () => {} }, emptyDraft)).toBe(true);
    for (const value of ["not json", JSON.stringify({ ...emptyDraft, format: 2 }), JSON.stringify({ ...emptyDraft, minSubscribers: -1 })]) {
      expect(loadDiscoveryPreferences({ getItem: () => value })).toBeNull();
    }
  });
  it("reports unavailable storage and invalid numeric settings", () => {
    expect(saveDiscoveryPreferences({ setItem: () => { throw new Error("quota"); } }, emptyDraft)).toBe(false);
    expect(saveDiscoveryPreferences({ setItem: () => {} }, { ...emptyDraft, targetCount: 0 })).toBe(false);
    expect(loadDiscoveryPreferences({ getItem: () => { throw new Error("denied"); } })).toBeNull();
  });
  it("leaves absent region and language unrestricted while forwarding explicit edits", () => {
    expect(discoveryPlanningBrief({ ...emptyDraft, brief: "鞋子" })).toBe("鞋子");
    expect(discoveryPlanningBrief({ ...emptyDraft, brief: "鞋子", region: "美国" })).toBe("鞋子");
  });
  it("forwards only explicit source restrictions and an explicitly expanded copy", () => {
    expect(discoveryPlanningBrief({ ...emptyDraft, brief: "鞋子", platformRestriction: ["douyin"] }))
      .toBe("鞋子\nSearch only these entries: douyin");
    expect(discoveryPlanningBrief({ ...emptyDraft, brief: "只搜 douyin", expandAllSources: true }))
      .toBe("只搜 douyin\nContent category: unrestricted\nSearch all supported entries; replace any previous platform restriction.");
  });
});

it("edited scopes with a target language must run normalization before a new draft", () => {
 expect(discoveryEditIsComplete(true,"电脑",[{keywords:["笔记本测评"]}],"英文")).toBe(false);
});
