import { env } from "cloudflare:workers";
import { applyD1Migrations, type D1Migration } from "cloudflare:test";
import { beforeEach, afterEach, describe, it, expect, vi } from "vitest";
import app from "../src/index";
import * as sessionModule from "../src/auth/session";
import { issueInstallLink, authorizeLegacyTicket, cliReleaseProjection, cleanExpiredLegacyTickets } from "../src/cli-distribution/service";
import { openCredential, ENTRY_LIFETIME_MS, DOWNLOAD_LIFETIME_MS, INSTALL_COOKIE } from "../src/cli-distribution/credentials";
import { manifestSchema, targets, type CliManifest } from "../src/cli-distribution/contracts";
import { renderInstaller } from "../src/cli-distribution/routes";
import { hash } from "../src/distribution/crypto";
import macTemplate from "../src/cli-distribution/install.sh.txt";
import windowsTemplate from "../src/cli-distribution/install.ps1.txt";

const base = env as unknown as CloudflareBindings & { TEST_MIGRATIONS: D1Migration[] };
const version = "0.5.22", user = "cli-test-user";
const manifestFor = (selected = version): CliManifest => ({ schema_version: 1, product: "onecrew-cli", version: selected,
  assets: targets.map(target => ({ target, key: `releases/${selected}/onecrew-${selected}-${target}${target.endsWith("windows-msvc") ? ".exe" : ""}`, size: 3, sha256: "a".repeat(64) })) });
const manifest = manifestFor();
const releases = new Map<string, CliManifest>();
const defaultGet = async (key: string) => {
  if (key.endsWith("cli-manifest.json")) {
    const value = releases.get(key.split("/")[1]!);
    return value ? { size: 1000, json: async () => structuredClone(value) } : null;
  }
  return { size: 3, customMetadata: { sha256: "a".repeat(64) }, body: new Blob([new Uint8Array([1, 2, 3])]).stream() };
};
const get = vi.fn(defaultGet);
const head = vi.fn(async () => ({ size: 3, customMetadata: { sha256: "a".repeat(64) } }));
const bindings = { ...base, ACCESS_ORIGIN: "https://access.example", BETTER_AUTH_URL: "https://access.example", WEB_ORIGIN: "https://access.example",
  RELEASE_LINK_SECRET: btoa(String.fromCharCode(...Array.from({ length: 32 }, (_, i) => i + 64))).replace(/=+$/, ""),
  CLI_RELEASE_VERSION: version, RELEASES: { get, head } as unknown as R2Bucket };
type Link = Awaited<ReturnType<typeof issueInstallLink>>;
const request = (path: string, init?: RequestInit, selected = bindings) => app.request(`https://access.example${path}`, init, selected);
const tokenOf = (link: { url: string }) => link.url.split("/").at(-1)!;
const cookieOf = (link: Link) => `${INSTALL_COOKIE}=${tokenOf(link)}`;
const post = (cookie?: string): RequestInit => ({ method: "POST", headers: { Origin: "https://access.example", "Content-Type": "application/json", ...(cookie ? { Cookie: cookie } : {}) }, body: "{}" });
async function downloadBase(link: Link, selected = bindings) {
  const result = await request(link.url, undefined, selected);
  expect(result.status).toBe(200);
  const text = await result.text();
  const scriptUrl = /^macScriptUrl: (.+)\/install\.sh$/m.exec(text)?.[1];
  expect(scriptUrl).toBeTruthy();
  return new URL(scriptUrl!).pathname;
}
async function legacyTicket(expiresAt = Date.now() + ENTRY_LIFETIME_MS) {
  const token = "b".repeat(43);
  await base.DB.prepare("INSERT INTO cli_install_ticket VALUES(?,?,?,?,?,?,?)")
    .bind(await hash(token), user, version, await hash(JSON.stringify(manifest)), 1, Date.now(), expiresAt).run();
  return { url: `/cli/install/${token}`, expiresAt };
}
beforeEach(async () => {
  vi.restoreAllMocks(); get.mockReset().mockImplementation(defaultGet); head.mockReset().mockResolvedValue({ size: 3, customMetadata: { sha256: "a".repeat(64) } });
  releases.clear(); releases.set(version, structuredClone(manifest));
  await applyD1Migrations(base.DB, base.TEST_MIGRATIONS);
  for (const id of [user, "other-cli-user"]) await base.DB.prepare('DELETE FROM "user" WHERE id=?').bind(id).run();
  await base.DB.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)')
    .bind(user, "Tester", "cli@example.com", "2026-01-01", "2026-01-01").run();
  await base.DB.prepare("INSERT INTO distribution_entitlement VALUES(?,'active',1,0)").bind(user).run();
  vi.spyOn(sessionModule, "loadSafeSession").mockResolvedValue({ headers: new Headers(), session: { expiresAt: new Date(Date.now() + 10000).toISOString(), sessionId: "test", user: { id: user, name: "Tester", email: "cli@example.com", image: null } } });
});
afterEach(() => vi.restoreAllMocks());

describe("stateless CLI installation", () => {
  it("requires origin, authenticated verified user and entitlement for issuance", async () => {
    expect((await request("/api/cli/install-links", { ...post(), headers: { Origin: "https://evil.example", "Content-Type": "application/json" } })).status).toBe(403);
    vi.mocked(sessionModule.loadSafeSession).mockResolvedValueOnce({ headers: new Headers(), session: null });
    expect((await request("/api/cli/install-links", post())).status).toBe(401);
    await base.DB.prepare('UPDATE "user" SET emailVerified=0 WHERE id=?').bind(user).run();
    expect((await request("/api/cli/install-links", post())).status).toBe(403);
    await base.DB.prepare('UPDATE "user" SET emailVerified=1 WHERE id=?').bind(user).run();
    await base.DB.prepare("UPDATE distribution_entitlement SET status='revoked' WHERE user_id=?").bind(user).run();
    expect((await request("/api/cli/install-links", post())).status).toBe(403);
  });
  it("issues no database tickets and restores the same link without extending its expiry", async () => {
    expect(await (await request("/api/cli/install-links")).json()).toEqual({ link: null });
    const issued = await request("/api/cli/install-links", post());
    expect(issued.status).toBe(200);
    const link = await issued.json() as Link;
    const cookie = issued.headers.get("Set-Cookie")!;
    for (const flag of [INSTALL_COOKIE, "Secure", "HttpOnly", "SameSite=Strict", "Path=/"]) expect(cookie).toContain(flag);
    expect(cookie).not.toContain("Domain=");
    expect(link.prompt).toContain(`https://access.example${link.url}`);
    expect(link.expiresAt).toBeGreaterThan(Date.now() + ENTRY_LIFETIME_MS - 1000);
    for (const init of [{ headers: { Cookie: cookieOf(link) } }, post(cookieOf(link))] as RequestInit[]) {
      const response = await request("/api/cli/install-links", init);
      expect(response.headers.get("Cache-Control")).toBe("no-store");
      expect(response.headers.get("Vary")).toContain("Cookie");
      const value = await response.json();
      expect(init.method ? value : (value as { link: Link }).link).toEqual(link);
    }
    expect((await base.DB.prepare("SELECT count(*) AS n FROM cli_install_ticket").first<{ n: number }>())?.n).toBe(0);
  });
  it("does not recover another account's cookie or issue a link without a browser session", async () => {
    const link = await issueInstallLink(bindings, user);
    await base.DB.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)')
      .bind("other-cli-user", "Other", "other@example.com", "2026-01-01", "2026-01-01").run();
    await base.DB.prepare("INSERT INTO distribution_entitlement VALUES(?,'active',1,0)").bind("other-cli-user").run();
    vi.mocked(sessionModule.loadSafeSession).mockResolvedValue({ headers: new Headers(), session: { expiresAt: new Date(Date.now() + 10000).toISOString(), sessionId: "other", user: { id: "other-cli-user", name: "Other", email: "other@example.com", image: null } } });
    const restored = await request("/api/cli/install-links", { headers: { Cookie: cookieOf(link) } });
    expect(await restored.json()).toEqual({ link: null });
    expect(restored.headers.get("Set-Cookie")).toContain("Max-Age=0");
    const other = await (await request("/api/cli/install-links", post(cookieOf(link)))).json() as Link;
    expect((await openCredential(bindings, tokenOf(other))).subject).toBe("other-cli-user");
    vi.mocked(sessionModule.loadSafeSession).mockResolvedValue({ headers: new Headers(), session: null });
    expect((await request("/api/cli/install-links", { headers: { Cookie: cookieOf(link) } })).status).toBe(401);
  });
  it("selects the latest release on entry access and pins scripts and files for that installation", async () => {
    const link = await issueInstallLink(bindings, user);
    releases.set("0.5.23", manifestFor("0.5.23"));
    const path = await downloadBase(link, { ...bindings, CLI_RELEASE_VERSION: "0.5.23" });
    const credential = await openCredential(bindings, path.split("/").at(-1)!);
    expect(credential).toMatchObject({ kind: "download", version: "0.5.23" });
    expect(credential.expiresAt - credential.issuedAt).toBeLessThanOrEqual(DOWNLOAD_LIFETIME_MS);
    releases.set("0.5.24", manifestFor("0.5.24"));
    const newer = { ...bindings, CLI_RELEASE_VERSION: "0.5.24" };
    for (const suffix of ["/install.sh", "/install.ps1"]) {
      const response = await request(path + suffix, undefined, newer);
      expect(response.status).toBe(200);
      const script = await response.text();
      if (suffix === "/install.sh") {
        expect(script).not.toContain("\r");
        expect(script).toMatch(/^#!\/bin\/sh\n/);
        expect(script).not.toMatch(/__ONECREW_[A-Z0-9_]+__/);
      }
      expect(script).toContain("0.5.23"); expect(script).not.toContain("0.5.24");
      expect(script).toContain(`${path}/files/`);
    }
    expect((await openCredential(bindings, (await downloadBase(link, newer)).split("/").at(-1)!))).toMatchObject({ version: "0.5.24" });
  });
  it("keeps downloads retryable, private, cookie-free and limited to their credential purpose", async () => {
    const link = await issueInstallLink(bindings, user), path = await downloadBase(link);
    vi.mocked(sessionModule.loadSafeSession).mockClear();
    for (const target of [...targets, targets[0]]) {
      const response = await request(`${path}/files/${target}`);
      expect(response.status).toBe(200); expect(response.headers.get("Cache-Control")).toBe("no-store");
      expect(response.headers.get("Referrer-Policy")).toBe("no-referrer");
      expect(new Uint8Array(await response.arrayBuffer())).toEqual(new Uint8Array([1, 2, 3]));
    }
    const headResponse = await request(`${path}/files/${targets[0]}`, { method: "HEAD" });
    expect(headResponse.status).toBe(200); expect(await headResponse.text()).toBe("");
    expect(headResponse.headers.get("Content-Length")).toBe("3");
    expect(sessionModule.loadSafeSession).not.toHaveBeenCalled();
    expect((await request(`/cli/download/${tokenOf(link)}/install.sh`)).status).toBe(404);
    expect((await request(`/cli/install/${path.split("/").at(-1)}`)).status).toBe(404);
    expect((await request(link.url + "/install.sh")).status).toBe(404);
    expect((await request(`${path}/files/unsupported`)).status).toBe(404);
    expect((await request(`${path}/install.sh`, { method: "POST" })).status).toBe(405);
  });
  it("rejects tampering, oversized tokens, foreign origins and an incompatible key", async () => {
    const link = await issueInstallLink(bindings, user), token = tokenOf(link);
    const tampered = token.slice(0, 30) + (token[30] === "a" ? "b" : "a") + token.slice(31);
    expect((await request(`/cli/install/${tampered}`)).status).toBe(404);
    expect((await request(`/cli/install/v1.${"a".repeat(1600)}`)).status).toBe(404);
    await expect(openCredential({ ...bindings, ACCESS_ORIGIN: "https://other.example", BETTER_AUTH_URL: "https://other.example", WEB_ORIGIN: "https://other.example" }, token)).rejects.toThrow("link_not_found");
    const rotatedKey = btoa(String.fromCharCode(...Array.from({ length: 32 }, (_, i) => i + 96))).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
    await expect(openCredential({ ...bindings, RELEASE_LINK_SECRET: rotatedKey }, token)).rejects.toThrow("link_not_found");
    const missingKey = { ...bindings }; Reflect.deleteProperty(missingKey, "RELEASE_LINK_SECRET");
    await expect(openCredential(missingKey, token)).rejects.toThrow("configuration_required");
  });
  it("expires credentials, restores no expired cookie, and signs a new entry on request", async () => {
    const link = await issueInstallLink(bindings, user);
    vi.spyOn(Date, "now").mockReturnValue(link.expiresAt - 1000);
    const path = await downloadBase(link);
    expect((await openCredential(bindings, path.split("/").at(-1)!)).expiresAt).toBe(link.expiresAt);
    vi.mocked(Date.now).mockReturnValue(link.expiresAt + 1);
    expect((await request(link.url)).status).toBe(410);
    expect((await request(path + "/install.sh")).status).toBe(410);
    expect(await (await request("/api/cli/install-links", { headers: { Cookie: cookieOf(link) } })).json()).toEqual({ link: null });
    const replacement = await (await request("/api/cli/install-links", post(cookieOf(link)))).json() as Link;
    expect(replacement.url).not.toBe(link.url); expect(replacement.expiresAt).toBeGreaterThan(link.expiresAt);
  });
  it.each(["revoked", "revision", "email"])("enforces %s changes on both entry and download", async change => {
    const link = await issueInstallLink(bindings, user), path = await downloadBase(link);
    if (change === "email") await base.DB.prepare('UPDATE "user" SET emailVerified=0 WHERE id=?').bind(user).run();
    else if (change === "revoked") await base.DB.prepare("UPDATE distribution_entitlement SET status='revoked' WHERE user_id=?").bind(user).run();
    else await base.DB.prepare("UPDATE distribution_entitlement SET revision=2 WHERE user_id=?").bind(user).run();
    expect((await request(link.url)).status).toBe(403);
    expect((await request(path + "/install.sh")).status).toBe(403);
  });
  it("rejects mutated releases and mismatched binary metadata", async () => {
    const link = await issueInstallLink(bindings, user), path = await downloadBase(link);
    releases.set(version, { ...manifest, assets: manifest.assets.map(a => ({ ...a, sha256: "b".repeat(64) })) });
    head.mockResolvedValue({ size: 3, customMetadata: { sha256: "b".repeat(64) } });
    expect((await request(path + "/install.sh")).status).toBe(409);
    releases.set(version, structuredClone(manifest)); head.mockResolvedValue({ size: 3, customMetadata: { sha256: "a".repeat(64) } });
    get.mockImplementation(async key => key.endsWith("cli-manifest.json") ? defaultGet(key) : { size: 4, customMetadata: { sha256: "a".repeat(64) }, body: new Blob([]).stream() });
    expect((await request(`${path}/files/${targets[0]}`)).status).toBe(409);
  });
  it("does not issue a new entry when the selected release is unavailable", async () => {
    await expect(issueInstallLink({ ...bindings, CLI_RELEASE_VERSION: undefined } as unknown as CloudflareBindings, user)).rejects.toThrow("release_unavailable");
    head.mockResolvedValueOnce({ size: 4, customMetadata: { sha256: "a".repeat(64) } });
    expect((await request("/api/cli/install-links", post())).status).toBe(409);
  });
  it("disables existing downloads when the release gateway is unpinned", async () => {
    const link = await issueInstallLink(bindings, user), path = await downloadBase(link);
    const unpinned = { ...bindings }; Reflect.deleteProperty(unpinned, "CLI_RELEASE_VERSION");
    expect((await request(link.url, undefined, unpinned)).status).toBe(409);
    expect((await request(path + "/install.sh", undefined, unpinned)).status).toBe(409);
  });
  it("keeps old links working until expiry and only cleans expired legacy rows", async () => {
    const link = await legacyTicket();
    expect((await request(link.url)).status).toBe(200);
    expect((await request(link.url + "/install.sh")).status).toBe(200);
    await cleanExpiredLegacyTickets(bindings);
    expect((await base.DB.prepare("SELECT count(*) AS n FROM cli_install_ticket").first<{ n: number }>())?.n).toBe(1);
    vi.spyOn(Date, "now").mockReturnValue(link.expiresAt + 1);
    await expect(authorizeLegacyTicket(bindings, tokenOf(link))).rejects.toThrow("link_expired");
    await cleanExpiredLegacyTickets(bindings);
    expect((await base.DB.prepare("SELECT count(*) AS n FROM cli_install_ticket").first<{ n: number }>())?.n).toBe(0);
  });
  it.each(["zh-CN", "en-US"])("carries %s from account issuance through agent access and language switching", async locale => {
    const init = post();
    const issued = await request("/api/cli/install-links", { ...init, headers: { ...init.headers, "Accept-Language": locale } });
    expect(issued.status).toBe(200);
    const link = await issued.json() as Link;
    expect(link.prompt).toContain(locale === "zh-CN" ? "请通过" : "Install or update OneCrew");
    expect(link.prompt).toContain(locale === "zh-CN" ? "先识别你当前所在的 AI 客户端" : "Identify your current AI client");
    for (const phrase of locale === "zh-CN"
      ? ["通过执行工具主动申请最小必要权限", "用户拒绝批准时停止"]
      : ["request the minimum required permission through your execution tool", "If the user denies approval, stop"]) expect(link.prompt).toContain(phrase);
    expect(link.prompt).toContain(locale === "zh-CN" ? "云端沙箱里的 uname 或 PowerShell 结果只代表沙箱，不代表用户电脑" : "uname or PowerShell results from a cloud sandbox describe only that sandbox, not the user's computer");
    expect(link.prompt).toContain(locale === "zh-CN" ? "其他支持 MCP 的客户端使用 setup" : "use setup to configure MCP in other supported clients");
    const promptUrl = new URL(link.prompt.match(/https:\/\/\S+/)![0]);
    expect(promptUrl.pathname).toBe(link.url);
    expect(promptUrl.searchParams.get("lng")).toBe(locale);
    expect(tokenOf(link)).not.toContain("?");
    const agent = await request(promptUrl.pathname + promptUrl.search);
    expect(agent.status).toBe(200);
    expect(agent.headers.get("Content-Language")).toBe(locale);
    expect(await agent.text()).toContain(locale === "zh-CN" ? "请将 OneCrew CLI" : "Install or update OneCrew CLI");

    const other = locale === "zh-CN" ? "en-US" : "zh-CN";
    for (const method of ["GET", "POST"]) {
      const options = method === "POST" ? post(cookieOf(link)) : { headers: { Cookie: cookieOf(link) } };
      const response = await request("/api/cli/install-links", { ...options, headers: { ...options.headers, "Accept-Language": other } });
      expect(response.headers.get("Vary")).toContain("Accept-Language");
      const body = await response.json();
      const restored = (method === "POST" ? body : (body as { link: Link }).link) as Link;
      expect(restored).toMatchObject({ url: link.url, expiresAt: link.expiresAt });
      expect(restored.prompt).toContain(`?lng=${other}`);
      expect(restored.prompt).toContain(other === "zh-CN" ? "请通过" : "Install or update OneCrew");
    }
  });
  it.each([
    ["?lng=zh-CN", "en-US", "zh-CN"],
    ["?lng=en-US", "zh-CN", "en-US"],
    ["?lng=unknown", "en-GB,en;q=0.9", "en-US"],
    ["", "zh-CN", "zh-CN"],
    ["", "en-US", "en-US"],
    ["", "", "zh-CN"],
  ])("resolves installation language for query %s and header %s", async (query, header, locale) => {
    const link = await issueInstallLink(bindings, user);
    for (const method of ["GET", "HEAD"]) {
      const response = await request(link.url + query, { method, headers: { "Accept-Language": header } });
      expect(response.status).toBe(200);
      expect(response.headers.get("Content-Language")).toBe(locale);
      expect(response.headers.get("Content-Type")).toBe("text/markdown; charset=utf-8");
      expect(response.headers.get("Vary")).toContain("Accept-Language");
      const text = await response.text();
      if (method === "HEAD") expect(text).toBe("");
      else expect(text).toContain(locale === "zh-CN" ? "请将 OneCrew CLI" : "Install or update OneCrew CLI");
    }
  });
  it.each(["zh-CN", "en-US"])("returns the full install workflow in %s", async locale => {
    const link = await issueInstallLink(bindings, user);
    const text = await (await request(`${link.url}?lng=${locale}`)).text();
    const required = locale === "en-US"
      ? ["preserves login; do not run login again unless explicitly required", "After the CLI succeeds and any required first-install login completes", "Always run plugin install", "remove only the old OneCrew plugin identities", "fully quit and restart Codex", "Do not classify every network error or nonzero exit as a sandbox denial", "Do not blindly rerun a command whose effects are unknown", "Approval does not connect a cloud sandbox to the target computer", "exact command, reason, and minimum permission scope"]
      : ["保留登录状态；除非明确要求，否则不要重新登录", "CLI 安装成功且所需的首次登录完成后", "必须使用 installed_path", "仅移除返回说明中列出的旧版 OneCrew 插件标识", "完全退出并重新启动 Codex", "不要把所有网络错误或非零退出都归因于沙盒", "效果未知的命令不得盲目重跑", "批准不会把云端沙箱连接到目标电脑", "具体命令、原因和最小权限范围"];
    for (const phrase of [...required, "next_actions", "plugin_identity_change_required", "installed", "enabled", "true", "verification", "fresh", "/bin/sh", "PowerShell"]) expect(text).toContain(phrase);
    const headings = locale === "en-US"
      ? ["Codex only:", "Other MCP clients:", "Unknown client:"]
      : ["仅限 Codex：", "其他 MCP 客户端：", "无法确定客户端："];
    const [common, branches] = text.split(headings[0]!);
    const [codex, rest] = branches!.split(headings[1]!);
    const [mcp, unknown] = rest!.split(headings[2]!);
    expect(common).toContain("connection_options");
    expect(common).not.toContain("plugin install");
    const environmentRules = locale === "en-US"
      ? ["A cloud Ubuntu/Linux sandbox does not establish that the user's computer is Linux", "only after the tool's connection to the target is established", "A product name or desktop window alone does not establish local command access or MCP support", "not that the user's OS is unsupported"]
      : ["云端 Ubuntu/Linux 沙箱不代表用户电脑是 Linux", "先确认工具连接到目标电脑后", "仅凭产品名称或桌面窗口，不能认定它能执行本机命令或支持 MCP", "而不是说用户的操作系统不受支持"];
    for (const rule of environmentRules) expect(common).toContain(rule);
    expect(text).not.toContain("Detect the current operating system.");
    expect(text).not.toContain("识别当前操作系统。");
    expect(codex).toContain("plugin_identity_change_required");
    for (const phrase of ["installed_path", "setup", "mcp --stdio", "serve"]) expect(mcp).toContain(phrase);
    expect(mcp).not.toContain("plugin_identity_change_required");
    expect(mcp).toContain(locale === "en-US" ? "it is not the MCP launch command" : "不是 MCP 启动命令");
    expect(mcp).toContain(locale === "en-US" ? "instead of inventing an HTTP endpoint" : "不要编造 HTTP 接入地址");
    expect(unknown).toContain(locale === "en-US" ? "Ask which AI client to connect before making client-specific changes" : "在修改客户端配置之前，询问用户要接入哪个 AI 客户端");
    expect(unknown).toContain(locale === "en-US" ? "does not by itself prove a live connection" : "不代表实际连接已成功");
    expect(text).not.toContain("Rust");
    expect(text).toMatch(/^# OneCrew /);
    expect(text).toContain(locale === "en-US" ? "## Client configuration" : "## 客户端配置");
    expect(text).toContain(locale === "en-US" ? "reopen the original installation link" : "重新打开原始安装链接");
    for (const code of ["download_refresh_required", "download_access_denied", "download_retryable"]) expect(text).toContain(code);
    expect(text).toContain("& $powerShellPath -NoProfile -NonInteractive -File $installerPath");
    expect(text).toContain("System32\\WindowsPowerShell\\v1.0\\powershell.exe");
    expect(text).toContain("$installerExitCode = $LASTEXITCODE");
    expect(text).toContain("$env:LOCALAPPDATA\\io.onecrewai.onecrew");
    expect(text).not.toContain("-ExecutionPolicy Bypass");
    expect(text).not.toContain("no separate confirmation is needed");
    expect(text).not.toContain("无需另行确认");
    for (const field of ["native_exit_code", "cli_installed", "command_timeout"]) expect(text).toContain(field);
    const mac = new URL(/^macScriptUrl: (.+)$/m.exec(text)![1]!);
    const windows = new URL(/^windowsScriptUrl: (.+)$/m.exec(text)![1]!);
    expect(mac.origin).toBe("https://access.example");
    expect(mac.pathname).toMatch(/^\/cli\/download\/v1\.[A-Za-z0-9_-]+\/install\.sh$/);
    expect(windows.href).toBe(mac.href.replace(/install\.sh$/, "install.ps1"));
  });
  it.each(["zh-CN", "en-US"])("localizes legacy entries and pre-bootstrap release guidance in %s", async locale => {
    releases.set("0.5.21", manifestFor("0.5.21"));
    const selected = { ...bindings, CLI_RELEASE_VERSION: "0.5.21" };
    const link = await issueInstallLink(selected, user);
    const text = await (await request(`${link.url}?lng=${locale}`, undefined, selected)).text();
    expect(text).toContain(locale === "zh-CN" ? "此旧版本使用旧版安装器" : "This older release uses the legacy installer");
    expect(text).toContain("mcp --stdio");
    expect(text).not.toContain("Rust");
    releases.set(version, structuredClone(manifest));
    const legacy = await legacyTicket();
    const response = await request(`${legacy.url}?lng=${locale}`);
    expect(response.headers.get("Content-Language")).toBe(locale);
    expect(await response.text()).toContain(`macScriptUrl: https://access.example${legacy.url}/install.sh`);
  });
});

describe("CLI release contracts", () => {
  it("rejects incomplete or inconsistent release assets", () => {
    expect(manifestSchema.safeParse(manifest).success).toBe(true);
    for (const assets of [manifest.assets.slice(1), [manifest.assets[0], manifest.assets[0], manifest.assets[2]], manifest.assets.map(a => ({ ...a, key: a.key.replace(version, "0.5.20") })), manifest.assets.map(a => ({ ...a, size: 0 }))]) expect(manifestSchema.safeParse({ ...manifest, assets }).success).toBe(false);
  });
  it("validates script substitutions and preserves pre-bootstrap release support", () => {
    for (const [selectedVersion, capability] of [["0.5.21", "0"], ["0.5.22", "1"], ["1.0.0", "1"]]) {
      for (const template of [macTemplate, windowsTemplate]) {
        const rendered = renderInstaller(template, manifestFor(selectedVersion!), `https://access.example/cli/download/v1.${"a".repeat(100)}`);
        expect(rendered).not.toMatch(/__ONECREW_/);
        expect(rendered).toContain(template === macTemplate ? `bootstrap_update='${capability}'` : `$bootstrapUpdate = '${capability}'`);
        expect(rendered).toContain("__bootstrap-update --install-path");
      }
    }
    for (const url of ["http://access.example/cli/download/v1." + "a".repeat(100), "https://access.example/cli/download/v1." + "a".repeat(100) + "'", "https://user@access.example/cli/download/v1." + "a".repeat(100)]) expect(() => renderInstaller(macTemplate, manifest, url)).toThrow("release_unavailable");
  });
  it("separates release availability and removes old ticket metadata from account responses", async () => {
    const unpinned: CloudflareBindings = { ...bindings }; Reflect.deleteProperty(unpinned, "CLI_RELEASE_VERSION");
    expect(await cliReleaseProjection(unpinned)).toEqual({ status: "not_published", version: null, targets: [] });
    const value = await (await request("/api/distribution/application")).json();
    expect(value).toMatchObject({ latestRelease: null, cliRelease: { status: "published", version, targets: [...targets] } });
    expect(value).not.toHaveProperty("cliTicket");
  });
});
