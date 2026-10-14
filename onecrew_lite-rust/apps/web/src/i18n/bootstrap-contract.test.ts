import { describe, expect, it } from "vitest";

import MAIN_SOURCE from "@/main.tsx?raw";

describe("desktop i18n bootstrap", () => {
  it("initializes translations before mounting React", () => {
    const initialization = MAIN_SOURCE.indexOf("await initializeI18n()");
    const mount = MAIN_SOURCE.indexOf("createRoot(");

    expect(initialization).toBeGreaterThan(-1);
    expect(mount).toBeGreaterThan(initialization);
    expect(MAIN_SOURCE).toContain("<I18nextProvider i18n={appI18n}>");
  });
});
