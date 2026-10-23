import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import test from "node:test";
import { hostMatchesSuffix } from "../src/native-capabilities/chrome-cookies.ts";
import { createReadSession } from "../src/native-capabilities/network.ts";

test("cookie hosts must match a complete domain label", () => {
  assert.equal(hostMatchesSuffix(".api.x.com", "x.com"), true);
  assert.equal(hostMatchesSuffix("fakeinstagram.com", "instagram.com"), false);
  assert.equal(hostMatchesSuffix("x.com.other.example", "x.com"), false);
  assert.equal(hostMatchesSuffix("x.com", ""), false);
});

test("Windows native DPAPI protects and restores bytes under the current user", {
  skip: process.platform !== "win32" ? "requires Windows DPAPI" : false,
}, async () => {
  const { Dpapi, isPlatformSupported } = await import("@primno/dpapi");
  assert.equal(isPlatformSupported, true);
  const plaintext = randomBytes(32);
  const protectedData = Dpapi.protectData(plaintext, null, "CurrentUser");
  const restored = Dpapi.unprotectData(protectedData, null, "CurrentUser");
  try {
    assert.deepEqual(Buffer.from(restored), plaintext);
  } finally {
    plaintext.fill(0);
    restored.fill(0);
  }
});

test("native read sessions reject an unapproved origin and a cancelled read", async () => {
  const session = await createReadSession(["https://x.com"]);
  try {
    await assert.rejects(session.getJson(new URL("https://other.example"), new AbortController().signal), /origin is not allowed/);
    await assert.rejects(session.getJson(new URL("https://x.com"), AbortSignal.abort()), { name: "AbortError" });
  } finally {
    await session.close();
  }
});
