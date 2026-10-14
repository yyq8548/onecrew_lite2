import { fileURLToPath, URL } from "node:url";

import { reactRouter } from "@react-router/dev/vite";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig, type Plugin } from "vite";

const root = fileURLToPath(new URL(".", import.meta.url));

function siteModuleBoundary(): Plugin {
  const forbidden = [
    "/src/access-app.tsx",
    "/features/account/",
    "/features/auth/",
    "/features/pairing/",
    "/lib/access-api.ts",
    "/lib/account-actions.ts",
    "/lib/access-i18n.ts",
    "/lib/auth-actions.ts",
    "/lib/auth-client.ts",
    "/lib/i18n.ts",
    "/lib/use-access-state.ts",
  ];
  return {
    name: "onecrew-site-module-boundary",
    generateBundle(_options, bundle) {
      for (const output of Object.values(bundle)) {
        if (output.type !== "chunk") continue;
        for (const id of Object.keys(output.modules)) {
          if (forbidden.some((segment) => id.includes(segment))) {
            this.error(`Site build imported Access module: ${id}`);
          }
        }
      }
    },
  };
}

export default defineConfig({
  root,
  publicDir: fileURLToPath(new URL("./public", import.meta.url)),
  plugins: [reactRouter(), tailwindcss(), siteModuleBoundary()],
  resolve: {
    alias: [
      {
        find: "@/lib/access-i18n",
        replacement: fileURLToPath(new URL("./src/lib/site-access-i18n.ts", import.meta.url)),
      },
      {
        find: "@",
        replacement: fileURLToPath(new URL("./src", import.meta.url)),
      },
    ],
  },
  server: { host: "127.0.0.1" },
  preview: { host: "127.0.0.1" },
  build: { emptyOutDir: true },
});
