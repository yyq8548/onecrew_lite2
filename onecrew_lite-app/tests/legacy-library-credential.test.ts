import assert from "node:assert/strict";
import { randomBytes, randomUUID } from "node:crypto";
import { inspect } from "node:util";
import test from "node:test";
import { AccessInstallationVerifier, VerifiedMigrationCredential } from "../src/identity/access-verification.ts";
import { parseLegacyLibraryCredential } from "../src/identity/legacy-library-credential.ts";

test("legacy Library format validates local identity and protects bearer without claiming online verification", async () => {
  const credentialId = randomUUID();
  const record = {
    bearer: `${credentialId}_${randomBytes(32).toString("base64url")}`,
    installation: {
      installation_id: randomUUID(), credential_id: credentialId, access_user_id: "local-user",
      access_device_id: `dev_${randomBytes(18).toString("base64url")}`, access_pairing_id: `pair_${randomBytes(18).toString("base64url")}`,
      source_instance_id: randomUUID(), client_platform: "windows", app_version: "0.1.4", status: "active",
      revision: 1, created_at: 1, updated_at: 2, last_seen_at: null, revoked_at: null,
    },
  };
  const credential = parseLegacyLibraryCredential(record, "local-user");
  assert.equal(credential.exposeBearer(), record.bearer);
  assert.equal(inspect(credential).includes(record.bearer), false);
  assert.throws(() => JSON.stringify(credential), /cannot be serialized/);
  assert.throws(() => parseLegacyLibraryCredential(record, "another-user"), /identity mismatch/);
  assert.throws(() => parseLegacyLibraryCredential({ ...record, bearer: `${randomUUID()}_${randomBytes(32).toString("base64url")}` }, "local-user"), /bearer is invalid/);
  assert.throws(() => parseLegacyLibraryCredential({ ...record, installation: { ...record.installation, credential_id: "invalid" } }, "local-user"));
  const signal = AbortSignal.abort(new Error("Verification cancelled before any remote call"));
  await assert.rejects(new AccessInstallationVerifier().verifyForMigration(credential, signal), error => error === signal.reason);
  assert.throws(() => Reflect.construct(VerifiedMigrationCredential, [Symbol("untrusted")]), /require online verification/);
  credential.dispose();
  assert.throws(() => credential.exposeBearer(), /disposed/);
});
