import { describe, expect, it } from "vitest";
import { presentDmFailure, presentProviderError, readDmFailure, type DmFailure } from "./provider-error";

const failure: DmFailure = { stage: "message_readback", diagnostic: {
  platform: "telegram", protocol: "telegram.mtproto", origin: "provider", code: "FLOOD_WAIT",
  http_status: null, rpc_code: 420, message: "Telegram message readback could not be completed.",
} };
function envelope(value: DmFailure = failure) {
  return { code: "direct_message_failed", message: value.diagnostic.message, failure: value };
}

describe("owner-provided diagnostic presentation", () => {
  it("does not invent recovery guidance from human error text without a native reason", () => {
    const unknown = presentProviderError("No structured diagnostic was returned.");
    for (const text of ["login", "HTTP 429", "timeout", "passcode", "sent successfully", "ProxyConnect"]) {
      expect(presentProviderError(text)).toEqual(unknown);
      expect(presentProviderError(new Error(text))).toEqual(unknown);
    }
    expect(unknown.details).toEqual([]);
  });

  it("keeps stage and protocol evidence without assigning a shared cause or retry policy", () => {
    const presentation = presentProviderError(envelope());
    expect(presentation.summary).toBe(failure.diagnostic.message);
    expect(presentation.details).toEqual([
      { label: "用例错误码", value: "direct_message_failed" },
      { label: "发生阶段", value: "message_readback" },
      { label: "平台", value: "telegram" },
      { label: "实际协议", value: "telegram.mtproto" },
      { label: "来源", value: "平台调用" },
      { label: "原生错误码", value: "FLOOD_WAIT" },
      { label: "RPC 错误码", value: "420" },
    ]);
    expect(presentation).not.toHaveProperty("kind");
    expect(presentation).not.toHaveProperty("retry_safe");
    expect(presentation).not.toHaveProperty("effect");
  });

  it("does not lose different protocols or stages when their numeric codes are equal", () => {
    const other: DmFailure = { stage: "thread_sync", diagnostic: {
      ...failure.diagnostic, platform: "another-platform", protocol: "another-protocol",
      code: "UNRECOGNIZED_CODE", rpc_code: null, http_status: 420,
    } };
    const first = presentDmFailure(failure).details;
    const second = presentDmFailure(other).details;
    expect(first).toContainEqual({ label: "RPC 错误码", value: "420" });
    expect(second).toContainEqual({ label: "HTTP 状态", value: "420" });
    expect(second).toContainEqual({ label: "实际协议", value: "another-protocol" });
    expect(second).toContainEqual({ label: "发生阶段", value: "thread_sync" });
    expect(second).toContainEqual({ label: "原生错误码", value: "UNRECOGNIZED_CODE" });
  });

  it("preserves missing native codes instead of filling an inferred value", () => {
    const value: DmFailure = { ...failure, diagnostic: { ...failure.diagnostic, code: null, rpc_code: null } };
    expect(readDmFailure(value)).toEqual(value);
    const details = presentDmFailure(value).details;
    expect(details).toContainEqual({ label: "原生错误码", value: "未提供" });
    expect(details.some(({ label }) => label === "RPC 错误码" || label === "HTTP 状态")).toBe(false);
  });

  it("distinguishes local condition codes from provider codes", () => {
    const value: DmFailure = { ...failure, diagnostic: {
      ...failure.diagnostic, origin: "local", code: "peer_reference_unavailable", rpc_code: null,
    } };
    expect(presentDmFailure(value).details).toContainEqual({ label: "本地条件码", value: "peer_reference_unavailable" });
    expect(presentDmFailure(value).details).not.toContainEqual({ label: "原生错误码", value: "peer_reference_unavailable" });
  });

  it("changes only explanatory copy when the owner changes its safe human message", () => {
    const first = presentDmFailure(failure);
    for (const message of ["login", "429", "sent successfully", "timeout"]) {
      const next = presentDmFailure({ ...failure, diagnostic: { ...failure.diagnostic, message } });
      expect(next.summary).toBe(message);
      expect(next.details).toEqual(first.details);
    }
  });

  it("rejects malformed evidence without inferring missing protocol or status fields", () => {
    for (const diagnostic of [
      { ...failure.diagnostic, protocol: undefined },
      { ...failure.diagnostic, platform: "" },
      { ...failure.diagnostic, code: undefined },
      { ...failure.diagnostic, origin: "guessed" },
      { ...failure.diagnostic, http_status: "429" },
      { ...failure.diagnostic, http_status: 65_536 },
      { ...failure.diagnostic, rpc_code: 0.5 },
    ]) {
      expect(readDmFailure({ ...failure, diagnostic })).toBeNull();
      expect(presentProviderError(envelope({ ...failure, diagnostic } as DmFailure)).details).toEqual([]);
    }
  });

  it("displays a typed local owner failure without pretending a provider was called", () => {
    expect(presentProviderError({ code: "context_unavailable", message: "The local context is unavailable." })).toEqual({
      summary: "The local context is unavailable.",
      details: [{ label: "用例错误码", value: "context_unavailable" }],
    });
  });

  it("does not expose unstructured causes or undeclared diagnostic properties", () => {
    const secret = "SENTINEL_PRIVATE_CREDENTIAL";
    const raw = "login https://provider.invalid/query?token=" + secret;
    expect(JSON.stringify(presentProviderError(raw))).not.toContain(secret);
    expect(JSON.stringify(presentProviderError(new Error(raw)))).not.toContain(secret);
    const value = { ...failure, diagnostic: { ...failure.diagnostic, raw_body: raw } };
    expect(JSON.stringify(readDmFailure(value))).not.toContain(secret);
    expect(JSON.stringify(presentDmFailure(value))).not.toContain(secret);
  });
});
