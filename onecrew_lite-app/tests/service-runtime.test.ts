import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { OperationRegistry } from "../src/application/operation-registry.ts";
import { AccountContextService } from "../src/identity/account-context.ts";
import { startLocalRuntime, readVerifiedRuntime } from "../src/service/runtime.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";

test("shutdown reports resource failures after removing the descriptor and releasing the native owner", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/service-runtime-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "failed-drain-"));
  const directory = await createPrivateDirectory(join(root, "private"));
  const failure = new Error("Resource drain failed");
  const runtime = await startLocalRuntime(directory, async () => ({
    operations: new OperationRegistry(new AccountContextService()),
    close: async () => { throw failure; },
  }));
  try {
    await assert.rejects(runtime.close(), error => error instanceof AggregateError && error.errors.includes(failure));
    await runtime.stopped;
    await assert.rejects(readVerifiedRuntime(directory));
    const next = await acquireInstanceLock(directory);
    await next.close();
  } finally { await rm(root, { recursive: true }); }
});

test("private runtime starts one owner, verifies live identity and restarts on its persisted port", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/service-runtime-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "runtime-"));
  const directory = await createPrivateDirectory(join(root, "private"));
  const registry = new OperationRegistry(new AccountContextService());
  const bootstrap = async () => ({ operations: registry });
  let runtime: Awaited<ReturnType<typeof startLocalRuntime>> | undefined;
  try {
    runtime = await startLocalRuntime(directory, bootstrap);
    const first = runtime.descriptor;
    assert.deepEqual(await readVerifiedRuntime(directory), first);
    await assert.rejects(startLocalRuntime(directory, bootstrap), { code: "runtime.already_owned" });
    await runtime.close();
    await assert.rejects(readVerifiedRuntime(directory));
    runtime = await startLocalRuntime(directory, bootstrap);
    assert.equal(runtime.descriptor.port, first.port);
    assert.notEqual(runtime.descriptor.instance_id, first.instance_id);
    assert.deepEqual(await readVerifiedRuntime(directory), runtime.descriptor);
    const shutdown = await fetch(`${runtime.descriptor.url}/_local/shutdown`, {
      method: "POST", headers: { authorization: `Bearer ${runtime.descriptor.control_token}` },
    });
    assert.equal(shutdown.status, 202);
    await runtime.stopped;
    await assert.rejects(readVerifiedRuntime(directory));
  } finally {
    if (runtime) await runtime.close();
    await rm(root, { recursive: true });
  }
});

test("runtime holds native ownership until resource drain finishes and does not bootstrap a second owner", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/service-runtime-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "drain-"));
  const directory = await createPrivateDirectory(join(root, "private"));
  let releaseDrain!: () => void;
  const drain = new Promise<void>(resolve => { releaseDrain = resolve; });
  let draining!: () => void;
  const enteredDrain = new Promise<void>(resolve => { draining = resolve; });
  let bootstrapCount = 0;
  let stoppedAccepting = false;
  const bootstrap = async () => {
    bootstrapCount++;
    return {
      operations: new OperationRegistry(new AccountContextService()),
      stopAcceptingWork: () => { stoppedAccepting = true; },
      close: async () => { draining(); await drain; },
    };
  };
  const runtime = await startLocalRuntime(directory, bootstrap);
  let closing: Promise<void> | undefined;
  try {
    await assert.rejects(startLocalRuntime(directory, bootstrap), { code: "runtime.already_owned" });
    assert.equal(bootstrapCount, 1);
    closing = runtime.close();
    await enteredDrain;
    assert.equal(stoppedAccepting, true);
    await assert.rejects(acquireInstanceLock(directory), { code: "runtime.already_owned" });
    releaseDrain();
    await closing;
    await runtime.stopped;
    const next = await acquireInstanceLock(directory);
    await next.close();
  } finally {
    releaseDrain();
    await (closing ?? runtime.close());
    await rm(root, { recursive: true });
  }
});
