import { describe, expect, it, vi } from "vitest";
import backendInventory from "../../../../crates/opencli/src/addons.rs?raw";
import { canInstallOpenCliAddons, installOpenCliAddons, openCliSetupReportSchema, openOpenCliSetupPage, recoverOpenCliBridge, repairOpenCliInstallation, selectedProfileIsConnected, type OpenCliSetupReport } from "./opencli-setup";
import { invoke } from "./local-runtime";

vi.mock("./local-runtime", () => ({ invoke: vi.fn(), isRuntimeAvailable: () => false }));

const report: OpenCliSetupReport = {
  installed: true, version: "1.8.8", command_count: 100,
  required_version: "1.8.8", command_path: "/test/opencli",
  daemon_running: true, extension_connected: true,
  profiles: [{ context_id: "work", alias: "campaign" }], issue: null,
  search_capabilities: [{ platform: "youtube", command: "youtube/search", browser_required: true, registered: false }],
  addons: [{ id: "onecrew-site-auth", state: "ready" }, { id: "onecrew-youtube", state: "ready" }, { id: "onecrew-tiktok", state: "missing" }, { id: "onecrew-bilibili", state: "ready" }, { id: "onecrew-tiktok-profile", state: "ready" }, { id: "onecrew-bilibili-profile", state: "ready" }, { id: "onecrew-bilibili-video", state: "ready" }, { id: "onecrew-twitter-identity", state: "ready" }, { id: "onecrew-twitter-post-identity", state: "ready" }, { id: "onecrew-twitter-post-content", state: "ready" }, { id: "onecrew-substack", state: "ready" }, { id: "onecrew-source-search", state: "ready" }, { id: "onecrew-browser-search", state: "ready" }],
};
report.addons.push({ id: "onecrew-public-search", state: "ready" }, { id: "onecrew-bluesky", state: "ready" });

describe("OpenCLI setup selection", () => {
  it("accepts every adapter in the backend installation inventory", () => {
    const ids = [...new Set([...backendInventory.matchAll(/id: "(onecrew-[a-z-]+)"/g)].map(match => match[1]))];
    expect(ids).toContain("onecrew-public-search");
    const addons = ids.map(id => ({ id, state: "ready" }));
    expect(openCliSetupReportSchema.parse({ ...report, addons }).addons).toEqual(addons);
  });
  it("accepts the full installed search adapter inventory", () => {
    expect(openCliSetupReportSchema.parse(report).addons).toHaveLength(15);
  });
  it("repairs only after explicit approval and keeps disconnected readback honest", async () => {
    const disconnected = { ...report, daemon_running: true, extension_connected: false, profiles: [], issue: "extension_unavailable" };
    vi.mocked(invoke).mockResolvedValueOnce(disconnected);
    expect(await repairOpenCliInstallation()).toEqual(disconnected);
    expect(invoke).toHaveBeenLastCalledWith("onecrew_opencli_installation_repair", { approved: true });
    expect(selectedProfileIsConnected(disconnected, "work")).toBe(false);
    vi.mocked(invoke).mockResolvedValueOnce({ installed: true });
    await expect(repairOpenCliInstallation()).rejects.toThrow();
    vi.mocked(invoke).mockClear();
  });
  it("uses readback after local recovery and preserves disconnected evidence", async () => {
    const disconnected = { ...report, daemon_running: true, extension_connected: false, profiles: [], issue: "extension_unavailable" };
    vi.mocked(invoke).mockResolvedValueOnce(disconnected);
    expect(await recoverOpenCliBridge()).toEqual(disconnected);
    expect(invoke).toHaveBeenLastCalledWith("onecrew_opencli_bridge_recover");
    expect(selectedProfileIsConnected(disconnected, "work")).toBe(false);
    vi.mocked(invoke).mockResolvedValueOnce({ started: true });
    await expect(recoverOpenCliBridge()).rejects.toThrow();
    vi.mocked(invoke).mockClear();
  });
  it("requires a matching native Chrome receipt and does not retry or fall back to a web link", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ page: "extension", browser: "chrome", launch_requested: true });
    await openOpenCliSetupPage("extension");
    expect(invoke).toHaveBeenLastCalledWith("onecrew_opencli_setup_page_open", { page: "extension" });
    vi.mocked(invoke).mockResolvedValueOnce({ page: "download", browser: "chrome", launch_requested: true });
    await expect(openOpenCliSetupPage("extension")).rejects.toThrow();
    vi.mocked(invoke).mockRejectedValueOnce(new Error("chrome_not_installed"));
    await expect(openOpenCliSetupPage("download")).rejects.toThrow("chrome_not_installed");
    vi.mocked(invoke).mockClear();
  });
  it("sends explicit local setup approval and rejects incomplete installation receipts", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(report.addons);
    expect(await installOpenCliAddons()).toEqual(report.addons);
    expect(invoke).toHaveBeenLastCalledWith("onecrew_opencli_addons_install", { approved: true });
    vi.mocked(invoke).mockResolvedValueOnce([]);
    await expect(installOpenCliAddons()).rejects.toThrow();
    vi.mocked(invoke).mockRejectedValueOnce(new Error("local failure"));
    await expect(installOpenCliAddons()).rejects.toThrow("local failure");
    expect(invoke).toHaveBeenCalledTimes(3); // No automatic retry in the client.
  });
  it("allows only missing compatible bundled files, independently of browser login", () => {
    expect(canInstallOpenCliAddons(report)).toBe(true);
    expect(canInstallOpenCliAddons({ ...report, version: "1.8.9" })).toBe(false);
    expect(canInstallOpenCliAddons({ ...report, addons: [] })).toBe(false);
    for (const state of ["conflict", "unavailable"] as const) {
      expect(canInstallOpenCliAddons({ ...report, addons: report.addons.map(addon => addon.id === "onecrew-tiktok" ? { ...addon, state } : addon) })).toBe(false);
    }
    expect(canInstallOpenCliAddons({ ...report, addons: report.addons.map(addon => ({ ...addon, state: "ready" })) })).toBe(false);
    expect(openCliSetupReportSchema.safeParse({ ...report, addons: undefined }).success).toBe(false);
    expect(openCliSetupReportSchema.safeParse({ ...report, addons: [report.addons[0], report.addons[0]] }).success).toBe(false);
    expect(openCliSetupReportSchema.safeParse({ ...report, addons: [{ id: "arbitrary-addon", state: "ready" }] }).success).toBe(false);
    expect(canInstallOpenCliAddons({ ...report, addons: report.addons.slice(0, 2) })).toBe(false);
    for (const missing of ["onecrew-bilibili", "onecrew-tiktok-profile", "onecrew-bilibili-profile", "onecrew-bilibili-video", "onecrew-twitter-identity", "onecrew-twitter-post-identity", "onecrew-twitter-post-content", "onecrew-substack", "onecrew-source-search", "onecrew-browser-search", "onecrew-public-search", "onecrew-bluesky"]) {
      expect(canInstallOpenCliAddons({ ...report, addons: report.addons.map(addon => ({ ...addon, state: addon.id === missing ? "missing" : "ready" })) })).toBe(true);
    }
    expect(canInstallOpenCliAddons({ ...report, addons: report.addons.slice(0, 9) })).toBe(false);
    expect(canInstallOpenCliAddons({ ...report, addons: report.addons.slice(0, 5) })).toBe(false);
    expect(canInstallOpenCliAddons({ ...report, addons: report.addons.slice(0, 7) })).toBe(false);
    expect(canInstallOpenCliAddons({ ...report, addons: report.addons.slice(0, 4) })).toBe(false);
    expect(canInstallOpenCliAddons({ ...report, addons: report.addons.slice(0, 3) })).toBe(false);
  });
  it("requires the exact connected context id and a healthy check", () => {
    expect(selectedProfileIsConnected(report, "work")).toBe(true);
    expect(selectedProfileIsConnected(report, "campaign")).toBe(false);
    expect(selectedProfileIsConnected(report, "other")).toBe(false);
    expect(selectedProfileIsConnected({ ...report, issue: "extension_unavailable" }, "work")).toBe(false);
    expect(selectedProfileIsConnected({ ...report, daemon_running: false }, "work")).toBe(false);
    expect(selectedProfileIsConnected({ ...report, extension_connected: false }, "work")).toBe(false);
    expect(selectedProfileIsConnected({ ...report, version: "1.8.6" }, "work")).toBe(false);
    expect(openCliSetupReportSchema.safeParse({ ...report, required_version: undefined }).success).toBe(false);
  });

  it("keeps command availability separate from the connected browser identity", () => {
    const parsed = openCliSetupReportSchema.parse(report);
    expect(selectedProfileIsConnected(parsed, "work")).toBe(true);
    expect(parsed.search_capabilities[0].registered).toBe(false);
    expect(openCliSetupReportSchema.safeParse({ ...report, search_capabilities: undefined }).success).toBe(false);
    expect(openCliSetupReportSchema.safeParse({ ...report, search_capabilities: [{ platform: "youtube", command: "search", browser_required: true, registered: "true" }] }).success).toBe(false);
  });
});
