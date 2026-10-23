import { fileURLToPath } from "node:url";
import { readFileSync } from "node:fs";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  root: fileURLToPath(new URL("./..", import.meta.url)),
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@mcp-ui": fileURLToPath(new URL("./../src", import.meta.url)) } },
  define: {
    __MCP_UI_VERSION__: JSON.stringify(JSON.parse(readFileSync(fileURLToPath(new URL("./../package.json", import.meta.url)), "utf8")).version),
  },
});
