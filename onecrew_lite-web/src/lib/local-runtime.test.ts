import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

describe("local browser transport", () => {
  beforeEach(() => vi.resetModules());
  afterEach(() => vi.unstubAllGlobals());

  it("requires a real session handshake and sends same-origin cookies plus CSRF", async () => {
    const fetch = vi.fn()
      .mockResolvedValueOnce(Response.json({ ok: true, result: { csrf: "synthetic-csrf", version: "test", transport: "local_http" } }))
      .mockResolvedValueOnce(Response.json({ ok: true, result: { id: "local-record" } }));
    vi.stubGlobal("fetch", fetch);
    const runtime = await import("./local-runtime");
    expect(runtime.isRuntimeAvailable()).toBe(false);
    expect(await runtime.invoke("onecrew_creator_get", { input: { creator_id: "test" } })).toEqual({ id: "local-record" });
    expect(runtime.isRuntimeAvailable()).toBe(true);
    expect(fetch).toHaveBeenNthCalledWith(1, "/api/session", expect.objectContaining({ credentials: "same-origin" }));
    expect(fetch).toHaveBeenNthCalledWith(2, "/api/commands/onecrew_creator_get", expect.objectContaining({
      credentials: "same-origin", method: "POST",
      headers: { "Content-Type": "application/json", "X-OneCrew-CSRF": "synthetic-csrf" },
    }));
  });

  it("rejects unauthenticated sessions before any business request", async () => {
    const fetch = vi.fn().mockResolvedValue(Response.json({ ok: false, error: "session_required" }, { status: 401 }));
    vi.stubGlobal("fetch", fetch);
    const runtime = await import("./local-runtime");
    await expect(runtime.invoke("onecrew_creator_manual_create", { input: {} })).rejects.toThrow("web_session_required");
    expect(fetch).toHaveBeenCalledOnce();
    expect(runtime.isRuntimeAvailable()).toBe(false);
  });

  it("preserves typed owner errors and invalidates an expired browser session", async () => {
    const typed = { kind: "busy", storage_upgraded: false, support_code: "LOCAL_DATA_BUSY" };
    const fetch = vi.fn()
      .mockResolvedValueOnce(Response.json({ ok: true, result: { csrf: "synthetic", version: "test", transport: "local_http" } }))
      .mockResolvedValueOnce(Response.json({ ok: false, error: typed }))
      .mockResolvedValueOnce(Response.json({ ok: false }, { status: 401 }));
    vi.stubGlobal("fetch", fetch);
    const runtime = await import("./local-runtime");
    await expect(runtime.invoke("local_data_upgrade")).rejects.toEqual(typed);
    expect(runtime.isRuntimeAvailable()).toBe(true);
    await expect(runtime.invoke("service_status")).rejects.toThrow("web_session_required");
    expect(runtime.isRuntimeAvailable()).toBe(false);
  });

  it("passes native diagnostics through the authenticated command boundary without stringifying", async () => {
    const error = { code: "direct_message_failed", message: "The provider read failed.", failure: {
      stage: "thread_sync", diagnostic: { platform: "telegram", protocol: "telegram.mtproto",
        origin: "provider", code: "UNRECOGNIZED_RPC", http_status: null, rpc_code: 420,
        message: "The provider read failed." },
    } };
    vi.stubGlobal("fetch", vi.fn()
      .mockResolvedValueOnce(Response.json({ ok: true, result: { csrf: "synthetic", version: "test", transport: "local_http" } }))
      .mockResolvedValueOnce(Response.json({ ok: false, error })));
    const runtime = await import("./local-runtime");
    await expect(runtime.invoke("onecrew_conversation_sync", { input: { conversation_id: "conversation" } }))
      .rejects.toEqual(error);
    expect(runtime.isRuntimeAvailable()).toBe(true);
  });
});
