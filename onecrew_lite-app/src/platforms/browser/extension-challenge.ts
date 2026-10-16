import { randomBytes, timingSafeEqual, createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { z } from "zod";
import { parseApplicationJson } from "../../contracts/application-json.ts";
import { runtimeAssetPath } from "../../packaging/runtime-context.ts";
import { runTrustedOpenCliProcess, type TrustedOpenCliProcess } from "../opencli/process.ts";
import { parseOpenCliVersion, validateOpenCliProfile } from "../opencli/registry.ts";
import { VERIFIED_OPENCLI_VERSION } from "../opencli/installation.ts";
import { requireExactOpenCliContextRoute } from "../opencli/context-route.ts";
import { OwnedChromeProcess } from "./chrome-host.ts";

const tabsSchema = z.array(z.object({ page: z.string().min(1), url: z.string(), title: z.string(), index: z.number().int().nonnegative(), active: z.boolean() })).max(1000);
const issuer = Symbol("managed-extension-challenge");
const verifiedIssuer = Symbol("verified-managed-extension-context");
const expression = "({href:location.href,nonce:location.hash.slice('#onecrew_browser_bind='.length),topLevel:window===window.top})";
export class VerifiedManagedExtensionContext {
  readonly contextId: string;
  readonly profileDirectory: string;
  readonly #process: OwnedChromeProcess;
  readonly #signal: AbortSignal;
  readonly #issuedAt: number;
  #consumed = false;
  constructor(token: symbol, contextId: string, process: OwnedChromeProcess, signal: AbortSignal) {
    if (token !== verifiedIssuer) throw new Error("browser.extension_challenge_required");
    this.contextId = contextId; this.profileDirectory = process.profileDirectory; this.#process = process; this.#signal = signal; this.#issuedAt = performance.now(); Object.freeze(this);
  }
  assertCurrent(): void {
    if (this.#consumed) throw new Error("browser.extension_proof_consumed");
    if (performance.now() - this.#issuedAt > 30_000) throw new Error("browser.extension_proof_expired");
    this.#signal.throwIfAborted(); OwnedChromeProcess.prototype.assertRunning.call(this.#process);
  }
  consume(): void { this.assertCurrent(); this.#consumed = true; }
  toJSON(): never { throw new Error("browser.extension_proof_not_serializable"); }
}
export class ManagedExtensionChallenge {
  readonly #url: string;
  readonly #nonce: string;
  readonly #session: string;
  #consumed = false;
  constructor(token: symbol, url: URL, nonce: string) {
    if (token !== issuer) throw new Error("browser.managed_challenge_required");
    this.#url = url.href; this.#nonce = nonce; this.#session = `onecrew-${randomBytes(16).toString("hex")}`; Object.freeze(this);
  }
  get url(): URL { return new URL(this.#url); }
  async verify(input: { process: OwnedChromeProcess; installation: TrustedOpenCliProcess; configurationDirectory: string; contextId: string; signal: AbortSignal }): Promise<VerifiedManagedExtensionContext> {
    if (this.#consumed) throw new Error("browser.challenge_already_consumed");
    input.signal.throwIfAborted(); OwnedChromeProcess.prototype.assertRunning.call(input.process); validateOpenCliProfile(input.contextId);
    await requireExactOpenCliContextRoute(input.configurationDirectory, input.contextId);
    const options = { deadlineMs: 15_000, signal: input.signal, configurationDirectory: input.configurationDirectory };
    if (parseOpenCliVersion((await runTrustedOpenCliProcess(input.installation, ["--version"], options)).stdout) !== VERIFIED_OPENCLI_VERSION) throw new Error("browser.opencli_version_changed");
    const prefix = ["--profile", input.contextId, "browser", this.#session];
    await runTrustedOpenCliProcess(input.installation, [...prefix, "bind"], options);
    try {
      const tabs = tabsSchema.parse(parseApplicationJson((await runTrustedOpenCliProcess(input.installation, [...prefix, "tab", "list"], options)).stdout));
      const matches = tabs.filter(tab => tab.url === this.#url);
      if (matches.length !== 1) throw new Error("browser.managed_challenge_tab_required");
      const value = z.strictObject({ href: z.string(), nonce: z.string(), topLevel: z.literal(true) }).parse(parseApplicationJson((await runTrustedOpenCliProcess(input.installation, [...prefix, "eval", expression, "--tab", matches[0]!.page], options)).stdout));
      await requireExactOpenCliContextRoute(input.configurationDirectory, input.contextId);
      const actual = Buffer.from(value.nonce), expected = Buffer.from(this.#nonce);
      if (value.href !== this.#url || actual.length !== expected.length || !timingSafeEqual(actual, expected)) throw new Error("browser.managed_challenge_mismatch");
    } finally {
      await runTrustedOpenCliProcess(input.installation, [...prefix, "unbind"], options);
    }
    OwnedChromeProcess.prototype.assertRunning.call(input.process); input.signal.throwIfAborted(); this.#consumed = true;
    return new VerifiedManagedExtensionContext(verifiedIssuer, input.contextId, input.process, input.signal);
  }
  toJSON(): never { throw new Error("browser.challenge_not_serializable"); }
}

export async function prepareManagedExtensionChallenge(origin: string, signal: AbortSignal): Promise<ManagedExtensionChallenge> {
  const base = new URL(origin);
  if (base.protocol !== "http:" || base.hostname !== "127.0.0.1" || !base.port || base.pathname !== "/" || base.username || base.password || base.search || base.hash) throw new Error("browser.local_origin_required");
  signal.throwIfAborted();
  const expected = await readFile(runtimeAssetPath("browser/browser-binding.html", new URL("../../../../onecrew_lite-rust/apps/web/public/browser-binding.html", import.meta.url)));
  const response = await fetch(new URL("browser-binding.html", base), { redirect: "error", signal: AbortSignal.any([signal, AbortSignal.timeout(5000)]) });
  if (!response.ok || !response.headers.get("content-type")?.startsWith("text/html")) throw new Error("browser.challenge_asset_unavailable");
  if (!response.body || expected.length > 64 * 1024) throw new Error("browser.challenge_asset_invalid");
  const reader = response.body.getReader(), chunks: Uint8Array[] = []; let size = 0;
  for (;;) {
    const item = await reader.read(); if (item.done) break;
    size += item.value.length;
    if (size > expected.length) { await reader.cancel(); throw new Error("browser.challenge_asset_changed"); }
    chunks.push(item.value);
  }
  const bytes = Buffer.concat(chunks);
  if (bytes.length !== expected.length || createHash("sha256").update(bytes).digest("hex") !== createHash("sha256").update(expected).digest("hex")) throw new Error("browser.challenge_asset_changed");
  const nonce = randomBytes(32).toString("base64url"), url = new URL("browser-binding.html", base); url.hash = `onecrew_browser_bind=${nonce}`;
  return new ManagedExtensionChallenge(issuer, url, nonce);
}
