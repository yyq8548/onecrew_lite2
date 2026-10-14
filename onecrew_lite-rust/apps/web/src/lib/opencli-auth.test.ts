import { afterEach, describe, expect, it, vi } from "vitest";
import { preflightOpenCliSearch, signedOutPlatforms, openOpenCliLoginPage, checkOpenCliSiteAuth, checkOpenCliSitesAuth, clearOpenCliAuthObservations, loadOpenCliAuthObservations, saveOpenCliAuthObservation, type OpenCliAuthObservation } from "./opencli-auth";
import { invoke } from "./local-runtime";
import { displayOpenCliAuthObservations } from "./opencli-auth";

vi.mock("./local-runtime", () => ({ invoke: vi.fn() }));

function memoryStorage() {
  const values = new Map<string, string>();
  return {
    get length() { return values.size; },
    key(index: number) { return [...values.keys()][index] ?? null; },
    getItem(key: string) { return values.get(key) ?? null; },
    setItem(key: string, value: string) { values.set(key, value); },
    removeItem(key: string) { values.delete(key); },
  };
}

const observation: OpenCliAuthObservation = {
  platform: "instagram", profile_ref: "profile-1", status: "logged_in", checked_at: 1_790_000_000,
  method: "quick", issue: null,
};

afterEach(() => { vi.unstubAllGlobals(); vi.clearAllMocks(); });

describe("OpenCLI one-site login observation", () => {
  it("restores display history after restart without admitting a search from cached success", async () => {
    vi.stubGlobal("window", { localStorage: memoryStorage(), dispatchEvent: vi.fn() });
    const success = { ...observation, method: "full" as const, checked_at: Math.floor(Date.now() / 1000) };
    saveOpenCliAuthObservation("credential-1", success);
    expect(displayOpenCliAuthObservations("credential-1", "profile-1", {})).toEqual({instagram: success});
    expect(displayOpenCliAuthObservations("credential-2", "profile-1", {})).toEqual({});
    expect(displayOpenCliAuthObservations("credential-1", "profile-2", {})).toEqual({});
    vi.mocked(invoke).mockResolvedValueOnce([]).mockResolvedValueOnce({...success, status: "not_logged_in"});
    expect(await preflightOpenCliSearch("credential-1", "profile-1", [{platform: "instagram", browser_required: true}])).toEqual(["instagram"]);
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(displayOpenCliAuthObservations("credential-1", "profile-1", {}).instagram.status).toBe("not_logged_in");
  });
  it("excludes only confirmed signed-out entries from the exact browser", () => {
    const observations = { instagram: { ...observation, status: "not_logged_in" as const },
      twitter: { ...observation, platform: "twitter", status: "unknown" as const },
      facebook: { ...observation, platform: "facebook", profile_ref: "another", status: "not_logged_in" as const } };
    expect(signedOutPlatforms(observations, "profile-1")).toEqual(["instagram"]);
    expect(signedOutPlatforms({ instagram: observation }, "profile-1")).toEqual([]);
  });
  it("keeps bounded failure reasons distinct from a signed-out receipt", async () => {
    for (const issue of ["auth_probe_rate_limited", "auth_probe_navigation_failed", "auth_probe_timeout"] as const) {
      const failed = { ...observation, status: "error" as const, method: "full" as const, issue };
      vi.mocked(invoke).mockResolvedValueOnce(failed);
      expect(await checkOpenCliSiteAuth("profile-1", "instagram")).toEqual(failed);
      vi.mocked(invoke).mockResolvedValueOnce({ ...failed, error: "cookie=secret" });
      await expect(checkOpenCliSiteAuth("profile-1", "instagram")).rejects.toThrow();
    }
  });
  it("accepts a full identity-check receipt without exposing or storing identity", async () => {
    const full = { ...observation, platform: "jike", method: "full" as const };
    vi.mocked(invoke).mockResolvedValueOnce(full);
    expect(await checkOpenCliSiteAuth("profile-1", "jike")).toEqual(full);
    vi.stubGlobal("window", { localStorage: memoryStorage() });
    saveOpenCliAuthObservation("credential-1", full);
    expect(loadOpenCliAuthObservations("credential-1", "profile-1").jike).toEqual(full);
    vi.mocked(invoke).mockResolvedValueOnce({ ...full, identity: "private account" });
    await expect(checkOpenCliSiteAuth("profile-1", "jike")).rejects.toThrow();
  });
  it("opens only the requested profile and rejects mismatched navigation receipts", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ profile: "selected", platform: "instagram", opened: true });
    await openOpenCliLoginPage("selected", "instagram");
    expect(invoke).toHaveBeenCalledWith("onecrew_opencli_login_page_open", { profile: "selected", platform: "instagram" });
    vi.mocked(invoke).mockResolvedValueOnce({ profile: "other", platform: "instagram", opened: true });
    await expect(openOpenCliLoginPage("selected", "instagram")).rejects.toThrow("opencli_login_target_mismatch");
  });

  it("batch and individual TikTok checks use identical target and proof", async () => {
    const tk = {...observation, platform: "tiktok"};
    vi.mocked(invoke).mockResolvedValue(tk);
    const onResult = vi.fn();
    await checkOpenCliSitesAuth("profile-1", ["tiktok"], new AbortController().signal, onResult);
    expect(onResult).toHaveBeenCalledWith("tiktok", await checkOpenCliSiteAuth("profile-1", "tiktok"), 1);
    expect(invoke).toHaveBeenNthCalledWith(1, "onecrew_opencli_site_auth_check", {profile: "profile-1", platform: "tiktok"});
  });
  it("checks serially, deduplicates entries and continues past individual failures", async () => {
    const events: string[] = [];
    vi.mocked(invoke).mockImplementation(async (_command, args) => {
      const platform = (args as { platform: string }).platform;
      events.push(`read:${platform}`);
      if (platform === "twitter") throw new Error("failed");
      return { ...observation, platform };
    });
    await checkOpenCliSitesAuth("profile-1", ["instagram", "twitter", "instagram", "youtube"], new AbortController().signal,
      (platform, result, completed) => events.push(`${completed}:${platform}:${result?.status ?? "unknown"}`));
    expect(events).toEqual(["read:instagram", "1:instagram:logged_in", "read:twitter", "2:twitter:error", "read:youtube", "3:youtube:logged_in"]);
  });
  it("does not publish an in-flight result or start another read after cancellation", async () => {
    const controller = new AbortController();
    const callback = vi.fn();
    vi.mocked(invoke).mockImplementationOnce(async () => { controller.abort(); return observation; });
    await checkOpenCliSitesAuth("profile-1", ["instagram", "youtube"], controller.signal, callback);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(callback).not.toHaveBeenCalled();
  });
  it("requires the exact requested profile and excludes provider identity fields", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(observation);
    expect(await checkOpenCliSiteAuth("profile-1", "instagram")).toEqual(observation);
    expect(invoke).toHaveBeenCalledWith("onecrew_opencli_site_auth_check", { profile: "profile-1", platform: "instagram" });
    vi.mocked(invoke).mockResolvedValueOnce({ ...observation, identity: "private account" });
    await expect(checkOpenCliSiteAuth("profile-1", "instagram")).rejects.toThrow();
    vi.mocked(invoke).mockResolvedValueOnce({ ...observation, profile_ref: "another-profile" });
    await expect(checkOpenCliSiteAuth("profile-1", "instagram")).rejects.toThrow("opencli_auth_target_mismatch");
  });

  it("stores only account and Chrome-profile scoped historical results and clears them on logout", () => {
    const storage = memoryStorage();
    vi.stubGlobal("window", { localStorage: storage });
    const oldKey = "onecrew:opencli-auth-observations:v1:credential-1:profile-1";
    storage.setItem(oldKey, JSON.stringify([observation]));
    expect(loadOpenCliAuthObservations("credential-1", "profile-1")).toEqual({});
    saveOpenCliAuthObservation("credential-1", observation);
    expect(loadOpenCliAuthObservations("credential-1", "profile-1").instagram).toEqual(observation);
    expect(loadOpenCliAuthObservations("credential-2", "profile-1")).toEqual({});
    expect(loadOpenCliAuthObservations("credential-1", "profile-2")).toEqual({});
    expect(storage.getItem(storage.key(0) ?? "")).not.toContain("identity");
    clearOpenCliAuthObservations("credential-1");
    expect(storage.getItem(oldKey)).toBeNull();
    expect(loadOpenCliAuthObservations("credential-1", "profile-1")).toEqual({});
  });
});

it("start preflight refreshes stale Bilibili login and isolates failed platforms", async () => {
 const storage = memoryStorage();
 vi.stubGlobal("window", {localStorage: storage, dispatchEvent: vi.fn()});
 saveOpenCliAuthObservation("credential-1", {...observation, platform: "bilibili", method: "full", checked_at: 1});
 vi.mocked(invoke).mockResolvedValueOnce([]).mockResolvedValueOnce({...observation, platform: "bilibili", status: "logged_in", method: "full", checked_at: Math.floor(Date.now()/1000)})
   .mockRejectedValueOnce(new Error("private-provider-detail"));
 const excluded = await preflightOpenCliSearch("credential-1", "profile-1", [{platform:"bilibili",browser_required:true},{platform:"instagram",browser_required:true},{platform:"youtube",browser_required:true}]);
 expect(excluded).toEqual(["instagram"]);
 expect(vi.mocked(invoke).mock.calls.map(call=>call[1])).toEqual([{profile:"profile-1"},{profile:"profile-1",platform:"bilibili"},{profile:"profile-1",platform:"instagram"}]);
 expect(loadOpenCliAuthObservations("credential-1", "profile-1").instagram.status).toBe("error");
 expect(JSON.stringify(loadOpenCliAuthObservations("credential-1", "profile-1"))).not.toContain("private-provider-detail");
 vi.mocked(invoke).mockClear();
 vi.mocked(invoke).mockResolvedValueOnce([loadOpenCliAuthObservations("credential-1", "profile-1").bilibili]);
 await preflightOpenCliSearch("credential-1", "profile-1", [{platform:"bilibili",browser_required:true}]);
 expect(invoke).toHaveBeenCalledExactlyOnceWith("onecrew_opencli_auth_observations", {profile:"profile-1"});
});
