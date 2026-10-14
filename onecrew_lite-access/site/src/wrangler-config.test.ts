import { readFile } from "node:fs/promises";
import { describe, expect, it } from "vitest";

const config = await readFile(new URL("../wrangler.toml", import.meta.url), "utf8");

describe("public Site Wrangler boundary", () => {
  it("uses only the custom production host with no preview endpoint", () => {
    expect(config).toContain('name = "site"');
    expect(config).toContain("workers_dev = false");
    expect(config).toContain("preview_urls = false");
    expect(config).toContain('{ pattern = "onecrewai.io", custom_domain = true }');
    expect(config).toContain('{ pattern = "www.onecrewai.io", custom_domain = true }');
    expect(config).toContain('SITE_ORIGIN = "https://onecrewai.io"');
    expect(config).toContain('WWW_ORIGIN = "https://www.onecrewai.io"');
    expect(config).toContain('ACCESS_ORIGIN = "https://access.onecrewai.io"');
    expect(config).toContain('BLOG_ORIGIN = "https://blog.onecrewai.io"');
  });

  it("runs the strict Worker guard before the asset binding", () => {
    expect(config).toContain('directory = "./build/client"');
    expect(config).toContain('binding = "ASSETS"');
    expect(config).toContain('not_found_handling = "none"');
    expect(config).toContain('html_handling = "none"');
    expect(config).toContain("run_worker_first = true");
    expect(config).not.toContain("d1_databases");
    expect(config).not.toContain("services");
  });
});
