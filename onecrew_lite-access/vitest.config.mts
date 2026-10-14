import {
  cloudflareTest,
  readD1Migrations,
} from "@cloudflare/vitest-plugin";
import { defineConfig } from "vitest/config";
import { readFile } from "node:fs/promises";

const migrations = await readD1Migrations("./migrations");

export default defineConfig({
  plugins: [
    {
      // Match Wrangler's Text module rule instead of Vite's asset-URL import.
      name: "cli-installer-text-modules",
      enforce: "pre",
      async load(id) {
        if (/\/src\/cli-distribution\/install\.(?:sh|ps1)\.txt$/.test(id)) {
          return `export default ${JSON.stringify(await readFile(id, "utf8"))};`;
        }
      },
    },
    cloudflareTest({
      additionalExports: {
        DesktopPairingAdmission: "WorkerEntrypoint",
        DesktopPairingRateLimitAdmin: "WorkerEntrypoint",
        DesktopDeviceLifecycle: "WorkerEntrypoint",
      },
      main: "./src/index.ts",
      miniflare: {
        compatibilityDate: "2026-08-26",
        compatibilityFlags: ["enable_request_signal"],
        bindings: {
          ACCESS_ORIGIN: "http://localhost:8787",
          BETTER_AUTH_SECRET: "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
          BETTER_AUTH_URL: "http://localhost:8787",
          PAIRING_HMAC_SECRET: "ICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8",
          TEST_MIGRATIONS: migrations,
          WEB_ORIGIN: "http://localhost:8787",
        },
        d1Databases: ["DB"],
        serviceBindings: {
          CREATOR_SYNC_DEVICE_REVOCATION: {
            name: "creator-sync-device-revocation-mock",
            entrypoint: "CreatorSyncDeviceRevocation",
          },
          CREATOR_SYNC_DEVICE_REVOCATION_CONTROL: {
            name: "creator-sync-device-revocation-mock",
            entrypoint: "CreatorSyncDeviceRevocationControl",
          },
        },
        workers: [
          {
            name: "creator-sync-device-revocation-mock",
            rootPath: ".",
            scriptPath: "./test/mocks/creator-sync-device-revocation-mock.js",
            modules: true,
          },
        ],
        ratelimits: {
          EDGE_RATE_LIMITER: {
            namespace_id: "test-reacher-access-edge",
            simple: { limit: 120, period: 60 },
          },
        },
      },
    }),
  ],
  test: {
    include: ["test/**/*.test.ts"],
  },
});
