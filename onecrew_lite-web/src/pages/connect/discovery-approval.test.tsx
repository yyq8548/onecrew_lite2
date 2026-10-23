import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import i18next from "i18next";
import { I18nextProvider } from "react-i18next";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en-US/shell";
import zh from "@/i18n/locales/zh-CN/shell";
import { DiscoveryPilot } from "./discovery-pilot";

const harness = vi.hoisted(() => ({
  buttons: new Map<string, () => void>(),
  approve: vi.fn(), preflight: vi.fn(), start: vi.fn(),
}));
vi.mock("@/components/ui/button", () => ({ Button: ({ children, onClick }: any) => {
  const label = renderToStaticMarkup(<>{children}</>);
  if (onClick) harness.buttons.set(label, onClick);
  return <button>{children}</button>;
} }));
vi.mock("@/lib/opencli-discovery", async importOriginal => ({
  ...await importOriginal<object>(), approveDiscoveryCampaign: harness.approve,
}));
vi.mock("@/lib/opencli-auth", async importOriginal => ({
  ...await importOriginal<object>(), preflightOpenCliSearch: harness.preflight,
}));
vi.mock("@/lib/discovery-execution", async importOriginal => ({
  ...await importOriginal<object>(), startDiscoveryExecution: harness.start,
}));
afterEach(() => { vi.unstubAllGlobals(); vi.clearAllMocks(); harness.buttons.clear(); });

it.each([false, true])("approval starts provider work only when selected: %s", async startAfterApproval => {
  const campaign = { id: "smoke", phase: "draft", revision: 1, brief: "Mouse",
    opencli_version: "1.8.8", criteria: {}, target_count: 3, used_calls: 0,
    scopes: [{ platform: "youtube", profile_ref: "test-profile", keywords: ["鼠标测评"] }], platform_runs: [] };
  vi.stubGlobal("window", { localStorage: { getItem: () => "smoke", setItem: () => {} } });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
  client.setQueryData(["opencli-campaign", "smoke"], campaign);
  harness.approve.mockResolvedValue({ ...campaign, phase: "active" });
  harness.preflight.mockResolvedValue([]);
  harness.start.mockResolvedValue({ campaign_id: "smoke", revision: 1, stage: "searching" });
  const i18n = i18next.createInstance();
  await i18n.init({ lng: "en-US", resources: { "en-US": { shell: en } }, defaultNS: "shell" });
  const html = renderToStaticMarkup(<I18nextProvider i18n={i18n}><QueryClientProvider client={client}>
    <DiscoveryPilot profileRef="test-profile" version="1.8.8" capabilities={[
      { platform: "youtube", command: "youtube/search", browser_required: true, registered: true },
    ]} />
  </QueryClientProvider></I18nextProvider>);
  expect(html).not.toContain("Maximum platform reads");
  const chinese = i18next.createInstance();
  await chinese.init({ lng: "zh-CN", resources: { "zh-CN": { shell: zh } }, defaultNS: "shell" });
  expect(chinese.t("opencliPilot.frozenPlan", { target: 3, version: "1.8.8" })).toContain("目标 3 人");
  const action = harness.buttons.get(startAfterApproval ? "Confirm plan and start searching" : "Approve plan only");
  expect(action).toBeDefined();
  action!();
  await vi.waitFor(() => expect(client.isMutating()).toBe(0));
  expect(harness.approve).toHaveBeenCalledWith("smoke", 1);
  expect(client.getQueryData(["opencli-campaign", "smoke"])).toMatchObject({ phase: "active", used_calls: 0 });
  expect(harness.preflight).toHaveBeenCalledTimes(startAfterApproval ? 1 : 0);
  expect(harness.start).toHaveBeenCalledTimes(startAfterApproval ? 1 : 0);
  client.clear();
});
