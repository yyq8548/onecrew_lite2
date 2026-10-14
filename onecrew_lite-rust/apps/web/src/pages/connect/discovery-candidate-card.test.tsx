import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import i18next from "i18next";
import { I18nextProvider } from "react-i18next";
import { renderToStaticMarkup } from "react-dom/server";
import { expect, it } from "vitest";
import zh from "@/i18n/locales/zh-CN/shell";
import en from "@/i18n/locales/en-US/shell";
import type { DiscoveryCampaign, DiscoveryCandidate } from "@/lib/opencli-discovery";
import { DiscoveryCandidateCard } from "./discovery-candidate-card";

it.each(["zh-CN", "en-US"])("keeps only the creator link and concise review state in %s", async locale => {
  const i18n = i18next.createInstance();
  await i18n.init({lng: locale, resources: {"zh-CN": {shell: zh}, "en-US": {shell: en}}});
  const client = new QueryClient();
  const item: DiscoveryCandidate = {id: "candidate", platform: "youtube", platform_account_id: "author",
    display_name: "Creator", profile_url: "https://www.youtube.com/channel/author", verdict: "pending",
    evidence: {source_urls: ["https://example.com/evidence"], public_links: ["https://example.com/tool"],
      assessment: {reasons: ["language_unverified"], subscribers_normalized: 12000}}};
  const campaign = {id: "campaign", revision: 1, criteria: {min_followers: 8000, language: "中文"}} as DiscoveryCampaign;
  const render = () => renderToStaticMarkup(<I18nextProvider i18n={i18n}><QueryClientProvider client={client}>
    <DiscoveryCandidateCard item={item} campaign={campaign} reasonLabels={{language_unverified: i18n.t("shell:opencliPilot.reason.language")}}
      sync={async () => { throw new Error("render must not sync"); }} />
  </QueryClientProvider></I18nextProvider>);
  let markup = render();
  expect(markup).toContain(item.profile_url);
  expect(markup).not.toContain("https://example.com");
  expect(markup).not.toContain(i18n.t("shell:opencliPilot.manualApprove"));
  expect(markup).not.toContain("<button");
  expect(markup).toContain(i18n.t("shell:opencliPilot.followersMet"));
  campaign.criteria.category = "稳定币";
  markup = render();
  expect(markup).not.toContain(i18n.t("shell:opencliPilot.manualApprove"));
  delete campaign.criteria.category;
  item.evidence.manual_approval = {approved_at: 1};
  markup = render();
  expect(markup).not.toContain(i18n.t("shell:opencliPilot.manualApproved"));
  expect(markup).toContain(i18n.t("shell:opencliPilot.reason.language"));
  expect(markup).not.toContain(i18n.t("shell:opencliPilot.manualAdded"));
  item.verdict = "qualified";
  item.evidence.assessment!.reasons = [];
  item.evidence.assessment!.match_rating = {level: "good", source_url: item.evidence.source_urls[0],
    reason: "粉丝评论语言符合：西班牙语；博主本人语言未核实。其他要求符合。"};
  campaign.criteria.language = "西班牙语";
  markup = render();
  expect(markup).toContain(item.evidence.assessment!.match_rating.reason);
  expect(markup).not.toContain(`>${i18n.t("shell:opencliPilot.reason.language")}<`);
  client.clear();
});
