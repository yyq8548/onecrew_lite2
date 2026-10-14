import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
const WEB_ROOT = process.cwd();

describe("OneCrew browser brand assets", () => {
  it("bundles the verified Geist Sans release with its complete OFL", () => {
    const fontPath = join(WEB_ROOT, "public", "fonts", "geist", "Geist-Variable.woff2");
    const licensePath = join(WEB_ROOT, "public", "licenses", "Geist-OFL-1.1.txt");
    const provenancePath = join(
      WEB_ROOT,
      "public",
      "licenses",
      "Geist-v1.7.2-SHA256.txt",
    );
    const font = readFileSync(fontPath);
    const license = readFileSync(licensePath);
    const provenance = readFileSync(provenancePath, "utf8");
    const theme = readFileSync(join(WEB_ROOT, "src", "styles", "theme.css"), "utf8");
    const globalStyles = readFileSync(join(WEB_ROOT, "src", "styles", "index.css"), "utf8");

    expect(createHash("sha256").update(font).digest("hex")).toBe(
      "a369fcf5628ea2aa4e1b9e2ec6a5b3624e365bda588e1f0f2f12b564f728fbb8",
    );
    expect(createHash("sha256").update(license).digest("hex")).toBe(
      "c683bfbcc7e087f5d37a54ef628f10387c451a83ddc459b151403a164ac46c90",
    );
    expect(provenance).toContain("Release: v1.7.2");
    expect(provenance).toContain(
      "Release asset SHA-256: 7fc800d2ac6b92844895196e5041aca55d814c15db70c44f79b3b83ab82b04e2",
    );
    expect(theme).toContain('src: url("/fonts/geist/Geist-Variable.woff2") format("woff2")');
    expect(theme).toContain('font-family: "Geist"');
    expect(globalStyles).toContain('font-family: "Geist", "PingFang SC"');
  });

});
