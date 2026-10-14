import { cp, mkdir, readFile, writeFile } from "node:fs/promises";
import { renderGa4Init } from "../development/ga4-template.mjs";
import { fileURLToPath, URL } from "node:url";

import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

const root = fileURLToPath(new URL(".", import.meta.url));
const publicDirectory = fileURLToPath(new URL("./public", import.meta.url));
const outputDirectory = fileURLToPath(new URL("./dist/access", import.meta.url));
const accessAssetPaths = [
  "ga4-init.js",
  "images/auth-story-connected.jpg",
  "images/auth-story-follow-up.jpg",
  "images/auth-story-history.jpg",
  "images/auth-story-prioritize.jpg",
  "images/auth-story-projects.jpg",
  "onecrew-logo-color.svg",
  "onecrew-logo-pairing.png",
  "images/creator-avatar-sprite.jpg",
  "images/ecommerce-creator-stories.jpg",
  "onecrew-ai-mark.png",
  "onecrew-favicon.svg",
  "onecrew-logo-dark.svg",
  "onecrew-watermark-mark.svg",
] as const;
const emptyMarketingResource = fileURLToPath(new URL("./src/lib/empty-marketing-resource.ts", import.meta.url));

function accessEntry(): Plugin {
  return {
    name: "onecrew-access-entry",
    transformIndexHtml: {
      order: "pre",
      handler(html) {
        return {
          html: html
            .replace("/src/main.tsx", "/src/access-main.tsx")
            .replace(
              "OneCrew AI 达人发现与外联工作台。搜索、筛选、外联和持续跟进，都在一个可掌控的工作流中完成。",
              "登录或创建 OneCrew 账号，并管理账户安全与已连接设备。",
            )
            .replace("OneCrew｜AI 达人发现与外联工作台", "OneCrew｜账号与设备访问"),
          tags: [
            { tag: "script", attrs: { src: "/ga4-init.js", "data-surface": "access" }, injectTo: "head" },
            { tag: "script", attrs: { async: true, src: "https://www.googletagmanager.com/gtag/js?id=G-E8PCYC26GN" }, injectTo: "head" },
            {
              tag: "meta",
              attrs: { name: "robots", content: "noindex, nofollow" },
              injectTo: "head",
            },
          ],
        };
      },
    },
  };
}

function accessResourceBoundary(): Plugin {
  return {
    name: "onecrew-access-resource-boundary",
    enforce: "pre",
    resolveId(source, importer) {
      if (
        importer?.includes("/src/locales/") &&
        (source === "./competitors" || source === "./marketing")
      ) {
        return emptyMarketingResource;
      }
      return null;
    },
  };
}

function accessModuleBoundary(): Plugin {
  return {
    name: "onecrew-access-module-boundary",
    generateBundle(_options, bundle) {
      const allowedMarketingModules = new Set([
        "/features/landing/marketing/route-scroll-top.tsx",
      ]);
      for (const output of Object.values(bundle)) {
        if (output.type !== "chunk") continue;
        for (const id of Object.keys(output.modules)) {
          if (
            id.includes("/features/landing/marketing/") &&
            ![...allowedMarketingModules].some((suffix) => id.endsWith(suffix))
          ) {
            this.error(`Access build imported marketing module: ${id}`);
          }
          if (id.endsWith("/src/site-app.tsx") || id.endsWith("/features/landing/landing-page.tsx")) {
            this.error(`Access build imported Site module: ${id}`);
          }
        }
      }
    },
  };
}

function copyAccessAssets(): Plugin {
  return {
    name: "onecrew-access-public-assets",
    async closeBundle() {
      await mkdir(outputDirectory, { recursive: true });
      await mkdir(new URL("./dist/access/images/", import.meta.url), { recursive: true });
      for (const path of accessAssetPaths) {
        if (path === "ga4-init.js") {
          await writeFile(new URL(`./dist/access/${path}`, import.meta.url), renderGa4Init(await readFile(new URL(`./public/${path}`, import.meta.url), "utf8")), "utf8");
          continue;
        }
        await cp(
          new URL(`./public/${path}`, import.meta.url),
          new URL(`./dist/access/${path}`, import.meta.url),
          { recursive: false },
        );
      }
    },
  };
}

export const accessConfig = {
  root,
  publicDir: false,
  plugins: [accessEntry(), accessResourceBoundary(), react(), tailwindcss(), accessModuleBoundary(), copyAccessAssets()],
  resolve: {
    alias: [
      {
        find: "@",
        replacement: fileURLToPath(new URL("./src", import.meta.url)),
      },
    ],
  },
  server: {
    proxy: {
      "/api": {
        changeOrigin: true,
        target: "http://localhost:8787",
      },
    },
  },
  build: {
    outDir: outputDirectory,
    emptyOutDir: true,
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [{ name: "vendor", test: /node_modules/ }],
        },
      },
    },
  },
};

export default defineConfig(({ command }) => ({
  ...accessConfig,
  publicDir: command === "serve" ? publicDirectory : false,
}));
