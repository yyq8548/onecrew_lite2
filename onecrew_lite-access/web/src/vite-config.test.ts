// @vitest-environment node

import { describe, expect, it } from "vitest";

import { accessConfig } from "../vite.access.config";
import siteConfig from "../vite.site.config";
import reactRouterConfig from "../react-router.config";
import marketingPages from "../../site/marketing-pages.json";

describe("Vite development boundary", () => {
  it("proxies same-origin /api calls to the local Worker", () => {
    expect(accessConfig.server?.proxy?.["/api"]).toMatchObject({
      changeOrigin: true,
      target: "http://localhost:8787",
    });
    expect(siteConfig.server?.proxy).toBeUndefined();
  });

  it("writes isolated Access and Site artifacts", () => {
    expect(String(accessConfig.build?.outDir)).toMatch(/web\/dist\/access$/);
    expect(reactRouterConfig.buildDirectory).toBe("../site/build");
    expect(reactRouterConfig.ssr).toBe(false);
    expect(typeof reactRouterConfig.prerender).toBe("object");
    const prerenderPaths = (reactRouterConfig.prerender as { paths: string[] }).paths;
    expect(prerenderPaths).toHaveLength(marketingPages.length * 2);
    expect(prerenderPaths).toContain("/zh/resources/user-guide");
    expect(prerenderPaths).toContain("/resources/user-guide");
    expect(prerenderPaths).toContain("/zh");
    expect(prerenderPaths).not.toContain("/en");
    expect(prerenderPaths).toContain("/");
    expect(prerenderPaths).toContain("/product");
    expect(accessConfig.publicDir).toBe(false);
    expect(String(siteConfig.publicDir)).toMatch(/web\/public$/);
    expect(accessConfig.plugins?.flat().map((plugin) => plugin && "name" in plugin ? plugin.name : null)).toContain("onecrew-access-module-boundary");
    expect(siteConfig.plugins?.flat().map((plugin) => plugin && "name" in plugin ? plugin.name : null)).toContain("onecrew-site-module-boundary");
  });
});
