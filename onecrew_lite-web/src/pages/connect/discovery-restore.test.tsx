import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import i18next from "i18next";
import { I18nextProvider } from "react-i18next";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en-US/shell";
import { DiscoveryPilot } from "./discovery-pilot";

const harness = vi.hoisted(() => ({
  buttons: new Map<string, { disabled?: boolean; click?: () => void }>(),
  get: vi.fn(), approve: vi.fn(), start: vi.fn(),
}));
vi.mock("@/components/ui/button", () => ({ Button: ({ children, onClick, disabled }: any) => {
  harness.buttons.set(renderToStaticMarkup(<>{children}</>), { disabled, click: onClick });
  return <button disabled={disabled}>{children}</button>;
} }));
// Expand the existing primitive to exercise the feature's restore action in SSR.
vi.mock("@/components/ui/collapsible", () => {
  const Content = ({ children }: any) => <div>{children}</div>;
  return { Collapsible: Content, CollapsibleContent: Content, CollapsibleTrigger: Content };
});
vi.mock("@/lib/opencli-discovery", async importOriginal => ({
  ...await importOriginal<object>(), getDiscoveryCampaign: harness.get, approveDiscoveryCampaign: harness.approve,
}));
vi.mock("@/lib/discovery-execution", async importOriginal => ({
  ...await importOriginal<object>(), startDiscoveryExecution: harness.start,
}));
afterEach(() => { vi.unstubAllGlobals(); vi.clearAllMocks(); harness.buttons.clear(); });

it("restores an MCP draft while an old campaign is selected, without approval or search", async () => {
  const old = { id: "old", phase: "active", revision: 1, brief: "Previous plan",
    opencli_version: "1.8.8", criteria: {}, target_count: 1, max_calls: 2, used_calls: 2,
    scopes: [{ platform: "instagram", profile_ref: "test-profile", keywords: ["鼠标测评"] }], platform_runs: [] };
  const draft = { ...old, id: "instagram-mouse-public-acceptance-20261011", brief: "Public mouse review", phase: "draft", used_calls: 0 };
  const setItem = vi.fn();
  vi.stubGlobal("window", { localStorage: { getItem: (key: string) => key === "onecrew-opencli-pilot-campaign" ? "old" : null, setItem } });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
  client.setQueryData(["opencli-campaign", "old"], old);
  client.setQueryData(["opencli-recent-campaigns"], [{ ...draft, updated_at: 1791714201 }]);
  harness.get.mockResolvedValue(draft);
  const i18n = i18next.createInstance();
  await i18n.init({ lng: "en-US", resources: { "en-US": { shell: en } }, defaultNS: "shell" });
  renderToStaticMarkup(<I18nextProvider i18n={i18n}><QueryClientProvider client={client}>
    <DiscoveryPilot profileRef="test-profile" version="1.8.8" capabilities={[
      { platform: "instagram", command: "onecrew-search-instagram/search", browser_required: true, registered: true },
    ]} />
  </QueryClientProvider></I18nextProvider>);
  try {
    const action = [...harness.buttons.entries()].find(([label]) => label.includes(draft.brief))?.[1];
    expect(action).toBeDefined();
    expect(action?.disabled).toBeFalsy();
    action!.click!();
    await vi.waitFor(() => expect(client.isMutating()).toBe(0));
    expect(harness.get).toHaveBeenCalledExactlyOnceWith(draft.id);
    expect(setItem).toHaveBeenCalledWith("onecrew-opencli-pilot-campaign", draft.id);
    expect(client.getQueryData(["opencli-campaign", draft.id])).toEqual(draft);
    expect(client.getQueryData(["opencli-campaign", old.id])).toEqual(old);
    expect(harness.approve).not.toHaveBeenCalled();
    expect(harness.start).not.toHaveBeenCalled();
  } finally { client.clear(); }
});
