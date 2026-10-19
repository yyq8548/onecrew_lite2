import { test } from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { once } from "node:events";
import { ManagedExtensionChallenge, prepareManagedExtensionChallenge } from "../src/platforms/browser/extension-challenge.ts";

test("managed browser challenge requires the actual local asset and retains an immutable one-use nonce", async () => {
  const asset = await readFile(new URL("../../onecrew_lite-web/public/browser-binding.html", import.meta.url));
  let changed = false;
  const server = createServer((request, response) => {
    assert.equal(request.url, "/browser-binding.html");
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" }); response.end(changed ? Buffer.concat([asset, Buffer.from(" ")]) : asset);
  });
  server.listen(0, "127.0.0.1"); await once(server, "listening");
  const address = server.address(); assert.ok(address && typeof address !== "string");
  const origin = `http://127.0.0.1:${address.port}`;
  try {
    const controller = new AbortController(), challenge = await prepareManagedExtensionChallenge(origin, controller.signal);
    assert.match(challenge.url.hash, /^#onecrew_browser_bind=[A-Za-z0-9_-]{43}$/u);
    const original = challenge.url.href; challenge.url.hash = "changed"; assert.equal(challenge.url.href, original);
    assert.throws(() => JSON.stringify(challenge), /not_serializable/u);
    assert.throws(() => Reflect.construct(ManagedExtensionChallenge, [Symbol(), new URL(origin), "untrusted"]), /managed_challenge_required/u);
    changed = true; await assert.rejects(prepareManagedExtensionChallenge(origin, controller.signal), /asset_changed/u);
    await assert.rejects(prepareManagedExtensionChallenge("https://example.com", controller.signal), /local_origin_required/u);
    controller.abort(); await assert.rejects(prepareManagedExtensionChallenge(origin, controller.signal), { name: "AbortError" });
  } finally { server.close(); await once(server, "close"); }
});
