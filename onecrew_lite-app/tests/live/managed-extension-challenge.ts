import assert from "node:assert/strict";
import test from "node:test";
import { setTimeout as delay } from "node:timers/promises";
import { prepareManagedExtensionChallenge } from "../../src/platforms/browser/extension-challenge.ts";
import { launchOwnedChrome } from "../../src/platforms/browser/chrome-host.ts";
import { resolveOpenCliInstallation } from "../../src/platforms/opencli/installation.ts";

test("installed official extension rejects another challenge and verifies its actual dedicated browser", { timeout: 90_000 }, async () => {
  const profileDirectory = process.env.ONECREW_LIVE_CHROME_PROFILE;
  const configurationDirectory = process.env.ONECREW_LIVE_OPENCLI_CONFIG;
  const contextId = process.env.ONECREW_LIVE_OPENCLI_CONTEXT;
  const origin = process.env.ONECREW_LIVE_ORIGIN;
  assert.ok(profileDirectory && configurationDirectory && contextId && origin, "Explicit installed dedicated browser and live service are required");
  const signal = AbortSignal.timeout(80_000);
  const challenge = await prepareManagedExtensionChallenge(origin, signal);
  const other = await prepareManagedExtensionChallenge(origin, signal);
  const browser = await launchOwnedChrome({ profileDirectory, url: challenge.url, signal });
  try {
    await delay(5000);
    const input = { process: browser, installation: await resolveOpenCliInstallation(), configurationDirectory, contextId, signal };
    await assert.rejects(other.verify(input), /managed_challenge_tab_required/u);
    browser.assertRunning();
    const proof = await challenge.verify(input);
    proof.assertCurrent();
    assert.equal(proof.contextId, contextId);
    proof.consume();
    assert.throws(() => proof.assertCurrent(), /extension_proof_consumed/u);
    await assert.rejects(challenge.verify(input), /challenge_already_consumed/u);
  } finally { await browser.close(); }
});
