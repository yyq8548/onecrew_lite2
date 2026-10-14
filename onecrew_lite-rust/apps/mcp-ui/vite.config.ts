/// <reference types="vitest/config" />

import { readFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vite";

const packageJson = JSON.parse(readFileSync(fileURLToPath(new URL("./package.json", import.meta.url)), "utf8"));

export default defineConfig({
  resolve: {
    alias: {
      "@mcp-ui": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  define: {
    __MCP_UI_VERSION__: JSON.stringify(packageJson.version),
  },
  test: {
    include: ["src/**/*.test.ts", "src/**/*.test.tsx", "preview/**/*.test.ts", "preview/**/*.test.tsx"],
  },
});
