import assert from 'node:assert/strict';
import { AccessClient, AccessError, type Credential } from '../../src/modules/access/client.js';
import { AccessService, type CredentialStore } from '../../src/modules/access/service.js';

const credential = {
  bearer: 'test-bearer',
  installation: { credential_id: 'credential-a' },
} as Credential;
const user = { id: 'user-a', email: 'a@example.com' };
let now = 1_000_000;
const originalNow = Date.now;
Date.now = () => now;

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

class MockClient extends AccessClient {
  validations = 0;
  profiles = 0;
  reports = 0;
  validateImpl: () => Promise<void> = async () => {};
  profileImpl: () => Promise<{ id: string; email: string }> = async () => user;
  override async validate() {
    this.validations++;
    await this.validateImpl();
  }
  override async profile() {
    this.profiles++;
    return this.profileImpl();
  }
  override async reportSystemInfo() {
    this.reports++;
  }
}

function makeService(client: MockClient) {
  let stored: Credential | undefined = credential;
  const store: CredentialStore = {
    async load() {
      return stored;
    },
    async save(value) {
      stored = value;
    },
    async clear() {
      stored = undefined;
    },
  };
  return new AccessService(store, client, 'darwin');
}

try {
  {
    const client = new MockClient();
    const gate = deferred<void>();
    client.validateImpl = () => gate.promise;
    const service = makeService(client);
    const calls = Array.from({ length: 12 }, () => service.status());
    await new Promise<void>((resolve) => setImmediate(resolve));
    assert.equal(client.validations, 1);
    gate.resolve();
    const statuses = await Promise.all(calls);
    assert.ok(statuses.every((status) => status.state === 'signed_in'));
    assert.equal(client.validations, 1);
    assert.equal(client.profiles, 1);
  }

  {
    const client = new MockClient();
    const gate = deferred<void>();
    client.validateImpl = () => gate.promise;
    const service = makeService(client);
    const calls = Array.from({ length: 8 }, () => service.status());
    await new Promise<void>((resolve) => setImmediate(resolve));
    gate.reject(new AccessError('access.remote_unavailable'));
    assert.ok((await Promise.all(calls)).every((status) => status.state === 'unavailable'));
    assert.equal(client.validations, 1);
    client.validateImpl = async () => {};
    assert.equal((await service.status()).state, 'unavailable');
    assert.equal(client.validations, 1);
    now += 3000;
    assert.equal((await service.status()).state, 'signed_in');
    assert.equal(client.validations, 2);
    assert.equal(client.profiles, 1);
  }

  {
    const client = new MockClient();
    const service = makeService(client);
    assert.equal((await service.status()).state, 'signed_in');
    now += 29_999;
    assert.equal((await service.status()).state, 'signed_in');
    assert.equal(client.validations, 1);
    now += 1;
    assert.equal((await service.status()).state, 'signed_in');
    assert.equal(client.validations, 2);
    client.validateImpl = async () => {
      throw new AccessError('access.revoked', 401);
    };
    now += 30_000;
    const revoked = await service.status();
    assert.deepEqual(revoked, { state: 'unavailable', user: null, issue: 'access.revoked' });
    assert.deepEqual(await service.status(), revoked);
    assert.equal(client.validations, 3);
  }

  for (const stopAfterProfile of [false, true]) {
    const client = new MockClient();
    const gate = deferred<void>();
    if (stopAfterProfile)
      client.profileImpl = async () => {
        await gate.promise;
        return user;
      };
    else client.validateImpl = () => gate.promise;
    const service = makeService(client);
    const pending = service.status();
    await new Promise<void>((resolve) => setImmediate(resolve));
    if (stopAfterProfile) await new Promise<void>((resolve) => setImmediate(resolve));
    assert.equal((await service.logout()).state, 'signed_out');
    gate.resolve();
    assert.equal((await pending).state, 'signed_out');
    assert.equal((await service.status()).state, 'signed_out');
  }
  console.log(
    'Credential verification concurrency, cache expiry, revocation, and logout races passed.',
  );
} finally {
  Date.now = originalNow;
}
