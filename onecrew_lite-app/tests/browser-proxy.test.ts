import test from "node:test";
import assert from "node:assert/strict";
import { createServer, request } from "node:http";
import { once } from "node:events";
import { randomBytes } from "node:crypto";
import { Server } from "proxy-chain";
import { createBrowserProxy, proxyUrl } from "../src/platforms/browser/proxy.ts";

test("real authenticated proxy forwarding uses a loopback-only browser endpoint without exposing credentials", async () => {
  const nonce = randomBytes(20).toString("hex"), password = randomBytes(20).toString("base64url"); let authenticated = 0;
  const target = createServer((_, response) => response.end(nonce)); target.listen(0, "127.0.0.1"); await once(target, "listening");
  const address = target.address(); assert.ok(address && typeof address !== "string");
  const upstream = new Server({ host: "127.0.0.1", port: 0, prepareRequestFunction: ({ username, password: actual }) => { if (username === "private" && actual === password) { authenticated++; return {}; } return { requestAuthentication: true }; } });
  await upstream.listen(); const upstreamAddress = upstream.server.address(); assert.ok(upstreamAddress && typeof upstreamAddress !== "string");
  const bridge = await createBrowserProxy({ kind: "http", host: "127.0.0.1", port: upstreamAddress.port, username: "private", password });
  try {
    assert.equal(bridge.host, "127.0.0.1"); assert.throws(() => JSON.stringify(bridge), /not_serializable/u);
    const body = await new Promise<string>((resolve, reject) => {
      const outgoing = request({ host: bridge.host, port: bridge.port, path: `http://127.0.0.1:${address.port}/`, method: "GET" }, response => {
        assert.equal(response.statusCode, 200); const chunks: Buffer[] = []; response.on("data", chunk => chunks.push(Buffer.from(chunk))); response.on("end", () => resolve(Buffer.concat(chunks).toString("utf8"))); response.on("error", reject);
      }); outgoing.on("error", reject); outgoing.end();
    });
    assert.equal(body, nonce); assert.equal(authenticated, 1);
    assert.equal(new URL(proxyUrl({ kind: "socks5", host: "::1", port: 1080, username: "user@name", password: "p:a/ss" })).hostname, "[::1]");
  } finally { await Promise.all([bridge.close(), bridge.close()]); await upstream.close(true); target.close(); await once(target, "close"); }
});
