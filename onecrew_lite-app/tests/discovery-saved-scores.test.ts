import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { basename, dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { AccountContextService } from "../src/identity/account-context.ts";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { resolveDiscoveryScoreDirectory, SavedDiscoveryScores } from "../src/discovery/saved-scores.ts";
import { DiscoveryExecutionSupervisor } from "../src/discovery/execution-supervisor.ts";
import { DiscoveryAdmissionGate } from "../src/discovery/admission-gate.ts";
import { setImmediate } from "node:timers/promises";

test("真实账号私有目录只恢复未启动评分，串行写入不丢失且账号/修订隔离", async () => {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-score-queue-", import.meta.url))), directory = await createPrivateDirectory(join(root, "private"));
  const lease = await acquireInstanceLock(directory), accounts = new AccountContextService(), databases = new AccountDatabases(lease, accounts);
  const principal = { user: { id: "account-a", email: "a@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" as const };
  let closed = false;
  try {
    const context = accounts.activate(principal); await databases.createNewAccount(context); const owned = await resolveDiscoveryScoreDirectory(databases, context);
    const queue = new SavedDiscoveryScores(owned, "../campaign", 1), same = new SavedDiscoveryScores(owned, "../campaign", 1);
    assert.equal(dirname(queue.path), join(owned.directory, "discovery-score-queue"));
    assert.equal(basename(queue.path), `${createHash("sha256").update(JSON.stringify(["account-a", "../campaign", 1])).digest("hex")}.json`);
    await Promise.all([queue.add({ hit: "first", deferred_platforms: ["youtube"] }), same.add({ hit: "second", deferred_platforms: ["bilibili"] })]);
    assert.deepEqual((await queue.load()).map(row => row.hit), ["first", "second"]);
    await queue.remove("first"); assert.deepEqual(await same.load(), [{ hit: "second", deferred_platforms: ["bilibili"] }]);
    assert.deepEqual(await new SavedDiscoveryScores(owned, "../campaign", 2).load(), []);
    const next = accounts.activate({ ...principal, user: { id: "account-b", email: "b@example.com" } }); await databases.createNewAccount(next);
    assert.deepEqual(await new SavedDiscoveryScores(await resolveDiscoveryScoreDirectory(databases, next), "../campaign", 1).load(), []);
    for (const hit of ["third", "fourth", "fifth", "sixth"]) await queue.add({ hit, deferred_platforms: [] });
    const before = await readFile(queue.path); await assert.rejects(queue.add({ hit: "overflow", deferred_platforms: [] }), /exceeds_capacity/);
    assert.deepEqual(await readFile(queue.path), before);
    await queue.defer({ hit: "second", deferred_platforms: ["bilibili"] }, "review_requirement_citation_invalid");
    await queue.add({ hit: "overflow", deferred_platforms: [] });
    assert.equal((await queue.load()).length, 5);
    assert.deepEqual(await same.loadDeferred(), [{ hit: "second", deferred_platforms: ["bilibili"], reason: "review_requirement_citation_invalid" }]);
    await queue.remove("overflow");
    await queue.add({ hit: "second", deferred_platforms: ["bilibili"] });
    assert.deepEqual(await same.loadDeferred(), []);
    await queue.defer({ hit: "second", deferred_platforms: ["bilibili"] }, "review_requirement_citation_invalid");
    await same.remove("second");
    assert.deepEqual(await queue.loadDeferred(), []);
    await writeFile(queue.path, "invalid"); const malformed = await readFile(queue.path); await assert.rejects(queue.load());
    assert.deepEqual(await readFile(queue.path), malformed);
    databases.close(); closed = true; await assert.rejects(queue.load(), /closed/);
  } finally { if (!closed) databases.close(); await lease.close(); }
});

test("真实队列容量错误显示后，同一账号任务首次续跑可释放失败项并完成", async () => {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-score-resume-", import.meta.url))), directory = await createPrivateDirectory(join(root, "private"));
  const lease = await acquireInstanceLock(directory), accounts = new AccountContextService(), databases = new AccountDatabases(lease, accounts);
  const context = accounts.activate({ user: { id: "Ye7CHhx3OvrXP80JGxEJJVmabkhpM9Kn", email: "christ8548@gmail.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
  const supervisor = new DiscoveryExecutionSupervisor(accounts, new DiscoveryAdmissionGate(accounts));
  try {
    await databases.createNewAccount(context);
    const queue = new SavedDiscoveryScores(await resolveDiscoveryScoreDirectory(databases, context), "6b434f43-3da3-4957-8ada-46042ac36fd3", 1);
    const hits = ["f086f397-a0b8-4397-b7f2-636870ea6606", "3e0f8fe5-18cb-410f-a753-9bcaf558ed93", "99ef38df-01c9-46ce-82a4-3c313429f4dd", "c0bb14eb-c59c-4c1e-ac77-933e4532eb68"];
    for (const hit of hits) await queue.add({ hit, deferred_platforms: ["linkedin"] });
    const ray = JSON.parse(await readFile(new URL("./fixtures/linkedin-profile-or-post-20261020.json", import.meta.url), "utf8")).hit.id as string;
    await queue.add({ hit: ray, deferred_platforms: ["linkedin"] });
    const input = { campaign_id: "6b434f43-3da3-4957-8ada-46042ac36fd3", expected_revision: 1, profile_ref: "d5pskwub", excluded_platforms: [] };
    await supervisor.start(context, input, 50, async () => { await queue.load(); }, async owner => {
      owner.change(status => { status.failure_phase = "score"; status.failure_code = "score_queue_exceeds_capacity"; });
      await queue.add({ hit: "11052dd7-9ceb-4d4b-983b-86df710bdd1b", deferred_platforms: [] });
    });
    while (supervisor.blocksBackendSync()) await setImmediate();
    assert.equal(supervisor.snapshot(context)?.stage, "error");
    const resumed = await supervisor.start(context, input, 50, async () => { await queue.load(); }, async owner => {
      await queue.defer({ hit: hits[0]!, deferred_platforms: ["linkedin"] }, "review_requirement_citation_invalid");
      owner.change(status => { status.stage = "finished"; });
    });
    assert.equal(resumed.stage, "searching");
    assert.equal((await supervisor.waitForIdle(context))?.stage, "finished");
    assert.equal((await queue.loadDeferred()).length, 1);
    await supervisor.close();
  } finally { databases.close(); await lease.close(); }
});
