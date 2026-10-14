import { WorkerEntrypoint } from "cloudflare:workers";

let calls = [];
let failure = false;
let result = { revocation: "revoked" };

export class CreatorSyncDeviceRevocation extends WorkerEntrypoint {
  async revokeAccessDevice(request) {
    calls.push(structuredClone(request));
    if (failure) throw new Error("mock_revocation_failure");
    return structuredClone(result);
  }
}

export class CreatorSyncDeviceRevocationControl extends WorkerEntrypoint {
  async reset() {
    calls = [];
    failure = false;
    result = { revocation: "revoked" };
  }

  async setFailure(enabled) {
    failure = enabled;
  }

  async setResult(value) {
    result = structuredClone(value);
  }

  async getCalls() {
    return structuredClone(calls);
  }
}
