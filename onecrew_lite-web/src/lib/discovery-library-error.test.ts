import { describe, expect, it } from "vitest";
import { discoveryLibraryFailureKey } from "./discovery-library-error";

describe("Library sync recovery", () => {
  it("distinguishes connection, account, and version failures", () => {
    expect(discoveryLibraryFailureKey("library_runtime_unavailable")).toBe("libraryUnavailable");
    expect(discoveryLibraryFailureKey("library_login_required")).toBe("libraryLoginRequired");
    expect(discoveryLibraryFailureKey("library_authorization_pending")).toBe("libraryAuthorizationPending");
    expect(discoveryLibraryFailureKey("library_auth_unavailable")).toBe("libraryAuthUnavailable");
    expect(discoveryLibraryFailureKey(new Error("library_account_mismatch"))).toBe("libraryAccountRequired");
    expect(discoveryLibraryFailureKey("web_session_required")).toBe("libraryWebSessionRequired");
    expect(discoveryLibraryFailureKey("library_sync_upgrade_required")).toBe("libraryUpgradeRequired");
  });

  it("does not treat uncertain writes or conflicting data as a safe connection retry", () => {
    for (const code of ["library_write_failed", "library_readback_incomplete", "library_sync_interrupted"]) {
      expect(discoveryLibraryFailureKey(code)).toBe("libraryOutcomeUncertain");
    }
    for (const code of ["library_projection_conflict", "library_row_identity_changed", "library_table_missing"]) {
      expect(discoveryLibraryFailureKey(code)).toBe("libraryProjectionConflict");
    }
    expect(discoveryLibraryFailureKey("library_source_stale")).toBe("librarySourceStale");
  });

  it("never renders or diagnoses unstructured private errors", () => {
    for (const value of [null, {}, { code: "library_account_mismatch" },
      "library_account_mismatch: secret-token", new Error("private server response"),
      new TypeError("Failed to fetch")]) {
      expect(discoveryLibraryFailureKey(value)).toBe("librarySyncFailed");
    }
  });
});
