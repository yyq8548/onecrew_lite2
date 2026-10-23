import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { I18nextProvider } from "react-i18next";
import { createInstance } from "i18next";
import { DiscoveryOverview } from "./discovery-overview";
import type { DiscoveryCampaign } from "@/lib/opencli-discovery";

const campaign = { brief: "水杯", phase: "active", target_count: 100, max_calls: 500, used_calls: 21,
  scopes: [{ platform: "youtube" }], platform_runs: [] } as unknown as DiscoveryCampaign;

describe("New plan entry", () => {
  it("explains exhausted scope without offering continue or pause", async () => {
    const i18n = createInstance();
    await i18n.init({ lng: "en-US", resources: { "en-US": { shell: { opencliPilot: {
      evidenceStopHelp: "Approved sources processed; review contact actions", automaticStart: "Continue",
    } } } } });
    const html = renderToStaticMarkup(<I18nextProvider i18n={i18n}><DiscoveryOverview
      campaign={campaign} counts={undefined} stage="needs_evidence" running={false} busy={false}
      profileChanged={false} syncing={false} library={null} syncFailed={false} canExpand={false}
      onSync={() => {}} onStart={() => {}} onPause={() => {}} onAnother={() => {}} onExpanded={() => {}} onIssues={() => {}}
    /></I18nextProvider>);
    expect(html).toContain("Approved sources processed; review contact actions");
    expect(html).not.toContain("Continue</button>");
    expect(html).not.toContain("opencliPilot.automaticPause");
    expect(html).not.toContain("opencliPilot.pauseEffect");
  });
  it("shows scoring failure separately from provider and sync failures", async () => {
    const i18n = createInstance();
    await i18n.init({ lng: "zh-CN", resources: { "zh-CN": { shell: { opencliPilot: {
      automaticError: "读取结果未确认", failure: { score: "评分中断", read: "读取中断" },
      syncPending: "结果待同步", newResultsSyncFailed: "同步失败",
    } } } } });
    const html = renderToStaticMarkup(<I18nextProvider i18n={i18n}><DiscoveryOverview
      campaign={campaign} counts={undefined} stage="error" running={false} busy={false}
      profileChanged={false} syncing={false} library={null} syncFailed={false} canExpand={false}
      failurePhase="score" failureCode="review_submit_outcome_unknown"
      onSync={() => {}} onStart={() => {}} onPause={() => {}} onAnother={() => {}} onExpanded={() => {}} onIssues={() => {}}
    /></I18nextProvider>);
    expect(html).toContain("评分中断");
    expect(html).not.toContain("读取结果未确认");
    expect(html).toContain("review_submit_outcome_unknown");
  });
  it.each(["zh-CN", "en-US"])("is visible without opening the menu in %s", async locale => {
    const i18n = createInstance();
    await i18n.init({ lng: locale, resources: { [locale]: { shell: { opencliPilot: { another: "New plan", title: "Discovery" } } } } });
    const render = (running: boolean) => renderToStaticMarkup(<I18nextProvider i18n={i18n}><DiscoveryOverview
      campaign={campaign} counts={undefined} stage="ready" running={running} busy={false}
      profileChanged={false} syncing={false} library={null} syncFailed={false} canExpand={true}
      onSync={() => {}} onStart={() => {}} onPause={() => {}} onAnother={() => {}} onExpanded={() => {}} onIssues={() => {}}
    /></I18nextProvider>);
    expect(render(false)).toMatch(/<button[^>]*>[^<]*New plan<\/button>/);
    expect(render(true)).toMatch(/<button[^>]*disabled=""[^>]*>[^<]*New plan<\/button>/);
    expect(render(true)).toContain("opencliPilot.automaticPause");
    expect(render(true).indexOf("opencliPilot.automaticPause")).toBeLessThan(render(true).indexOf("<dl"));
    expect(render(false)).not.toContain("opencliPilot.automaticPause");
  });
});
