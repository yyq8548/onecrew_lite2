import { afterEach, describe, expect, it, vi } from "vitest";
import { AccessContractError } from "./access-api";
import { createDistributionLink, DistributionApiError, getDistributionApplication, getDistributionPreview, submitDistributionApplication, createCliInstallLink, restoreCliInstallLink } from "./distribution-api";
import { safeDistributionReturnTo } from "./distribution-return";
const token = "A".repeat(43);
afterEach(() => vi.unstubAllGlobals());
const response = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });
describe("distribution authentication and download boundary", () => {
  it("restores a private CLI entry with browser credentials and rejects foreign entry URLs", async () => {
    const link = {url:`/cli/install/v1.${"a".repeat(100)}`,expiresAt:Date.now()+10000,prompt:"Server instructions"};
    const fetcher = vi.fn().mockResolvedValue(response({link})); vi.stubGlobal("fetch",fetcher);
    expect(await restoreCliInstallLink()).toEqual({link});
    expect(fetcher.mock.calls[0]).toMatchObject(["/api/cli/install-links", {method:"GET",credentials:"include",cache:"no-store"}]);
    fetcher.mockResolvedValue(response({...link,url:"https://evil.example/cli/install/v1."+"a".repeat(100)}));
    await expect(createCliInstallLink()).rejects.toBeInstanceOf(AccessContractError);
  });
  it("only preserves closed internal return routes", () => {
    expect(safeDistributionReturnTo(`/d/${token}`)).toBe(`/d/${token}`);
    expect(safeDistributionReturnTo("/account?apply=1")).toBe("/account?apply=1");
    for (const path of ["https://evil.test", "//evil.test", `/d/${token}?next=https://evil.test`, `/d/${token}#x`, "/d/%2f%2fevil.test", `/d/${"A".repeat(42)}`, "/account?next=https://evil.test"]) expect(safeDistributionReturnTo(path)).toBeNull();
  });
  it("submits only minimum fields with authenticated same-origin and stable idempotency key", async () => {
    const fetcher = vi.fn().mockResolvedValue(response({ cliRelease: { status: "not_published" as const, version: null, targets: [] }, applicationsOpen: true, settingRevision: 1, latestRelease: null, application: null, entitlement: null, email: "alex@example.com" })); vi.stubGlobal("fetch", fetcher);
    await submitDistributionApplication({ displayName: "Alex", purpose: "Creator partnerships", company: "" }, "test-key");
    const [url, options] = fetcher.mock.calls[0];
    expect(url).toBe("/api/distribution/application"); expect(options).toMatchObject({ method: "POST", cache: "no-store", credentials: "include", headers: { "Idempotency-Key": "test-key" } });
    expect(JSON.parse(options.body)).toEqual({ displayName: "Alex", purpose: "Creator partnerships", company: "" });
  });
  it("rejects unrecognized success and foreign download link URLs", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response({ url: "https://evil.test/d/token", expiresAt: 123 })));
    await expect(createDistributionLink("test-key")).rejects.toBeInstanceOf(AccessContractError);
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response({ application: null })));
    await expect(getDistributionApplication()).rejects.toBeInstanceOf(AccessContractError);
  });
  it("previews without redeeming tokens and only accepts protected asset URLs", async () => {
    const asset = { id: "installer-arm", filename: "OneCrew.dmg", platform: "darwin", arch: "aarch64", kind: "installer", size: 42, downloadUrl: `/api/distribution/download-links/${token}/assets/installer-arm` };
    const body = { expiresAt: 123, release: { version: "1.0.0", notes: "", pubDate: "2026-09-06" }, assets: [asset] };
    const fetcher = vi.fn().mockResolvedValue(response(body)); vi.stubGlobal("fetch", fetcher);
    await expect(getDistributionPreview(token)).resolves.toEqual(body);
    expect(fetcher).toHaveBeenCalledExactlyOnceWith(`/api/distribution/download-links/${token}`, expect.objectContaining({ method: "GET", credentials: "include", cache: "no-store" }));
    fetcher.mockResolvedValue(response({ ...body, assets: [{ ...asset, downloadUrl: "https://public.test/installer.dmg" }] }));
    await expect(getDistributionPreview(token)).rejects.toBeInstanceOf(AccessContractError);
  });
  it.each([[401, "authentication_required"], [404, "link_not_found"], [410, "link_expired"], [403, "entitlement_revoked"], [409, "release_unavailable"]])("preserves verified server state %s %s", async (status, error) => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response({ error }, Number(status))));
    await expect(getDistributionPreview(token)).rejects.toMatchObject({ status, code: error });
  });
  it("does not expose unknown backend diagnostics", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response({ error: "private_secret", detail: "secret" }, 500)));
    await expect(getDistributionPreview(token)).rejects.toEqual(new DistributionApiError(500));
  });
});
