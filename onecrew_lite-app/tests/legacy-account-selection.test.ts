import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import test from "node:test";
import { decideLegacyAccounts, type LegacyAccountSnapshot } from "../src/identity/legacy-account-selection.ts";

test("local migration decisions never activate an account or silently select between different owners", () => {
  const main: LegacyAccountSnapshot = { source: "main", state: "present", userId: "main-owner", fingerprint: randomBytes(32).toString("hex") };
  const library: LegacyAccountSnapshot = { source: "library", state: "present", userId: "library-owner", fingerprint: randomBytes(32).toString("hex") };
  assert.equal(decideLegacyAccounts(main, library).state, "choose_account");
  const chosen = decideLegacyAccounts(main, library, { source: "library", fingerprint: library.fingerprint! });
  assert.equal(chosen.state, "verify_existing");
  if (chosen.state !== "verify_existing") throw new Error("Expected verification requirement");
  assert.equal(chosen.userId, "library-owner");
  assert.throws(() => decideLegacyAccounts(main, { ...library, fingerprint: randomBytes(32).toString("hex") }, { source: "library", fingerprint: library.fingerprint! }), { code: "access.account_changed" });
  assert.equal(decideLegacyAccounts(main, { ...library, userId: main.userId }).state, "verify_existing");
  assert.equal(decideLegacyAccounts({ source: "main", state: "missing" }, library).state, "verify_existing");
  assert.equal(decideLegacyAccounts(main, { source: "library", state: "missing" }).state, "verify_existing");
  assert.equal(decideLegacyAccounts({ ...main, state: "inactive" }, { ...library, state: "inactive" }).state, "choose_account");
  assert.equal(decideLegacyAccounts({ ...main, state: "inactive" }, { ...library, state: "inactive", userId: main.userId }).state, "reauthorize");
  assert.equal(decideLegacyAccounts(main, { source: "library", state: "authorizing" }).state, "authorization_in_progress");
  assert.equal(decideLegacyAccounts(main, { source: "library", state: "unavailable" }).state, "unavailable");
  assert.equal(decideLegacyAccounts({ source: "main", state: "missing" }, { source: "library", state: "missing" }).state, "signed_out");
  assert.throws(() => decideLegacyAccounts({ source: "main", state: "present" }, library));
});
