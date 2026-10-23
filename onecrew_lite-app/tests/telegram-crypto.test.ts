import test from "node:test";
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { NodeCryptoProvider } from "@mtcute/node/utils.js";
import { assertNativeTelegramCryptography } from "../src/native-capabilities/telegram-crypto.ts";

test("actual Telegram Wasm cryptography loads locally and encrypts without opening a provider connection", async () => {
  await assertNativeTelegramCryptography();
  const key = randomBytes(32), iv = randomBytes(32), clear = randomBytes(64), crypto = new NodeCryptoProvider(), encrypted = crypto.createAesIge(key, iv).encrypt(clear);
  try { assert.notDeepEqual(encrypted, clear); const decrypted = crypto.createAesIge(key, iv).decrypt(encrypted); try { assert.deepEqual(Buffer.from(decrypted), clear); } finally { decrypted.fill(0); } }
  finally { key.fill(0); iv.fill(0); clear.fill(0); encrypted.fill(0); }
});
