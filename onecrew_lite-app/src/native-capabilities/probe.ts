import { arch, platform } from "node:os";
import { parseArgs } from "node:util";
import { z } from "zod";
import { readChromeCookies } from "./chrome-cookies.ts";
import { createReadSession, X_BROWSER_PROFILE } from "./network.ts";

const { values } = parseArgs({
  options: {
    kind: { type: "string" },
    "profile-dir": { type: "string" },
    proxy: { type: "string" },
  },
  strict: true,
  allowPositionals: false,
});
if (process.versions.node !== "26.7.0") throw new Error("Native probes require Node.js 26.7.0");
const environment = { platform: platform(), arch: arch(), node: process.versions.node };
if (values.kind === "chrome") {
  if (!values["profile-dir"]) throw new Error("Chrome probe requires an app-owned --profile-dir with Chrome closed");
  const cookies = await readChromeCookies(values["profile-dir"], ["x.com", "instagram.com"]);
  console.log(JSON.stringify({ ...environment, capability: "chrome-cookie-decryption", decryptedCookies: cookies.length, identityVerified: false }));
} else if (values.kind === "network") {
  const session = await createReadSession(["https://tls.peet.ws"], values.proxy);
  try {
    const raw = await session.getJson(new URL("https://tls.peet.ws/api/all"), AbortSignal.timeout(30_000));
    const observed = z.object({
      http_version: z.string(),
      tls: z.object({ ja4: z.string() }),
      http2: z.object({ akamai_fingerprint: z.string() }),
    }).parse(raw);
    console.log(JSON.stringify({ ...environment, capability: "browser-network", browser: X_BROWSER_PROFILE, observed, browserComparisonVerified: false }));
  } finally {
    await session.close();
  }
} else {
  throw new Error("Native probe kind must be chrome or network");
}
