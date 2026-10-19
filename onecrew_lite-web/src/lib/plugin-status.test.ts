import { describe, expect, it } from "vitest";

import type { LocalServiceStatus } from "./generated/creator-sync-contracts";
import { pluginRepairDiagnostic } from "./plugin-status";

const UNAVAILABLE: LocalServiceStatus["plugin"] = {
  diagnostic_code: null,
  enabled: null,
  installed_version: null,
  last_verified_at: 1,
  state: "unavailable",
  verification: "cached",
};

describe("plugin repair diagnostics", () => {
  it("names only the registration failures that plugin install repairs", () => {
    expect(pluginRepairDiagnostic({ ...UNAVAILABLE, diagnostic_code: "codex_marketplace_source_missing" }))
      .toBe("codex_marketplace_source_missing");
    expect(pluginRepairDiagnostic({ ...UNAVAILABLE, diagnostic_code: "codex_marketplace_source_relocated" }))
      .toBe("codex_marketplace_source_relocated");
    expect(pluginRepairDiagnostic({ ...UNAVAILABLE, diagnostic_code: "plugin_receipt_invalid" })).toBeNull();
    expect(pluginRepairDiagnostic(UNAVAILABLE)).toBeNull();
  });
});
