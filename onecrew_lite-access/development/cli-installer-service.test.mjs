import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const mac = await readFile(new URL("../src/cli-distribution/install.sh.txt", import.meta.url), "utf8");
const windows = await readFile(new URL("../src/cli-distribution/install.ps1.txt", import.meta.url), "utf8");
const version = "0.5.22";
const ready = { running: true, version, startup: { phase: "ready" }, data: { state: "ready" } };
const fixture = `#!/usr/bin/env node
const fs = require('node:fs');
const args = process.argv.slice(2);
fs.appendFileSync(process.env.TEST_CALLS, args[0]+'\\n');
if (args[0] === '--version') { console.log('onecrew 0.5.22'); }
else if (args[0] === '__bootstrap-update') {
  if (process.env.TEST_ACTION === 'failed') process.exit(2);
  const destination = args[args.indexOf('--install-path')+1];
  fs.copyFileSync(process.argv[1], destination); fs.chmodSync(destination, 0o755);
  console.log(JSON.stringify({action:process.env.TEST_ACTION,version:'0.5.22',installed_path:destination,next_action:process.env.TEST_ACTION==='installed'?{args:['login','--json']}:null}));
} else if (args[0] === 'status') {
  const path = process.env.TEST_INDEX;
  const index = fs.existsSync(path)?Number(fs.readFileSync(path,'utf8')):0;
  fs.writeFileSync(path,String(index+1));
  const states = JSON.parse(process.env.TEST_STATUSES);
  const state = states[Math.min(index,states.length-1)];
  if (state === null) process.exit(1);
  console.log(JSON.stringify({...state,binary_sha256:state.binary_sha256||process.env.TEST_SHA,web_url:'private-launch-url-must-not-escape'}));
} else if (args[0] === 'stop') { console.log('{"stopping":true,"data_preserved":true}'); }
else process.exit(3);
`;

async function execute(action, states, fastTimeout = false, downloadStatus = "200") {
  const root = await mkdtemp(join(tmpdir(), "onecrew-installer-service-"));
  try {
    const bin = join(root, "commands"), home = join(root, "user's home");
    await mkdir(bin); await mkdir(home);
    if (downloadStatus !== "200") {
      await mkdir(join(home, ".local/bin"), { recursive: true });
      await writeFile(join(home, ".local/bin/onecrew"), "existing CLI");
    }
    const candidate = join(root, "candidate");
    await writeFile(candidate, fixture, { mode: 0o755 });
    const sha = createHash("sha256").update(fixture).digest("hex");
    const shims = {
      uname: '#!/bin/sh\nif [ "$1" = -s ]; then echo Darwin; else echo arm64; fi\n',
      // The fake executable models CLI responses; production still validates real Mach-O bytes.
      od: '#!/bin/sh\necho cffaedfe0c000001\n',
      sleep: '#!/bin/sh\nexit 0\n',
      curl: `#!/usr/bin/env node\nconst fs=require('node:fs');const a=process.argv.slice(2);const status=process.env.TEST_DOWNLOAD_STATUS;if(status==='200'){fs.copyFileSync(process.env.TEST_CANDIDATE,a[a.indexOf('--output')+1]);}else{console.error('private-download-token-must-not-escape');}process.stdout.write(status);process.exit(status==='200'?0:22);\n`,
      date: `#!/usr/bin/env node\nconst fs=require('node:fs');const p=process.env.TEST_CLOCK;const n=fs.existsSync(p)?Number(fs.readFileSync(p,'utf8')):0;fs.writeFileSync(p,String(n+Number(process.env.TEST_TICK)));console.log(String(100+n));\n`,
    };
    for (const [name, body] of Object.entries(shims)) await writeFile(join(bin, name), body, { mode: 0o755 });
    const values = { VERSION: version, BOOTSTRAP_UPDATE: "1" };
    for (const target of ["MAC_ARM", "MAC_INTEL", "WINDOWS"]) {
      values[`${target}_URL`] = "https://access.example/cli/install/test/files/target";
      values[`${target}_SHA256`] = sha;
      values[`${target}_SIZE`] = String(Buffer.byteLength(fixture));
    }
    const script = mac.replace(/__ONECREW_([A-Z0-9_]+)__/g, (_, key) => values[key]);
    const path = join(root, "install.sh"); await writeFile(path, script);
    const callsPath = join(root, "calls");
    await writeFile(callsPath, "");
    const result = spawnSync("/bin/sh", [path], {
      encoding: "utf8", timeout: 30000,
      env: { ...process.env, HOME: home, PATH: `${bin}:${process.env.PATH}`, TEST_CANDIDATE: candidate,
        TEST_CALLS: callsPath, TEST_ACTION: action, TEST_STATUSES: JSON.stringify(states), TEST_DOWNLOAD_STATUS: downloadStatus,
        TEST_SHA: sha, TEST_INDEX: join(root, "index"), TEST_CLOCK: join(root, "clock"), TEST_TICK: fastTimeout ? "30" : "1" },
    });
    assert.ifError(result.error);
    assert.doesNotMatch(result.stdout + result.stderr, /private-launch-url-must-not-escape/);
    assert.doesNotMatch(result.stdout + result.stderr, /private-download-token-must-not-escape/);
    if (downloadStatus !== "200") assert.equal(await readFile(join(home, ".local/bin/onecrew"), "utf8"), "existing CLI");
    const calls = (await readFile(callsPath, "utf8")).trim().split("\n").filter(Boolean);
    return { ...result, calls };
  } finally { await rm(root, { recursive: true, force: true }); }
}

const nativeMac = { skip: process.platform !== "darwin" };
test("POSIX installer is LF-only and accepted by the native shell", nativeMac, () => {
  assert.doesNotMatch(mac, /\r/);
  const result = spawnSync("/bin/sh", ["-n"], { input: mac, encoding: "utf8" });
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr);
});
test("first installation preserves login guidance without starting a service", nativeMac, async () => {
  const result = await execute("installed", [ready]);
  assert.equal(result.status, 0);
  assert.deepEqual(JSON.parse(result.stdout).next_action, { args: ["login", "--json"] });
  assert.deepEqual(result.calls, ["--version", "__bootstrap-update"]);
  assert.match(JSON.parse(result.stdout).installed_path, /user's home/);
  assert.match(result.stderr, /follow the installation guide for your AI client/);
});
test("download failures give scoped recovery guidance without executing or replacing the CLI", nativeMac, async () => {
  for (const [status, code] of [
    ["401", "download_access_denied"], ["403", "download_access_denied"],
    ["404", "download_refresh_required"], ["409", "download_refresh_required"], ["410", "download_refresh_required"],
    ["000", "download_retryable"], ["429", "download_retryable"], ["503", "download_retryable"],
  ]) {
    const result = await execute("installed", [ready], false, status);
    assert.equal(result.status, 1);
    assert.equal(result.stdout, "");
    assert.ok(result.stderr.includes(code));
    assert.deepEqual(result.calls, []);
  }
});
test("an update waits for startup and verifies the installed service", nativeMac, async () => {
  const result = await execute("updated", [null, { ...ready, startup: { phase: "inspecting" }, data: null }, ready]);
  assert.equal(result.status, 0);
  assert.equal(JSON.parse(result.stdout).action, "updated");
  assert.equal(result.calls.filter(x => x === "status").length, 3);
  assert.ok(!result.calls.includes("stop"));
});
test("already_current repairs an old host and rechecks after stop acknowledgement", nativeMac, async () => {
  const old = { ...ready, version: "0.5.21", binary_sha256: "b".repeat(64) };
  const result = await execute("already_current", [old, old, ready]);
  assert.equal(result.status, 0);
  assert.equal(result.calls.filter(x => x === "stop").length, 2);
  assert.equal(result.calls.at(-1), "status");
});
test("same-version different bytes are also replaced", nativeMac, async () => {
  const result = await execute("already_current", [{ ...ready, binary_sha256: "b".repeat(64) }, ready]);
  assert.equal(result.status, 0);
  assert.equal(result.calls.filter(x => x === "stop").length, 1);
});
test("startup failure and timeout are not reported as successful updates", nativeMac, async () => {
  for (const state of [{ ...ready, startup: { phase: "failed" } }, { ...ready, data: null }]) {
    const result = await execute("updated", [state], true);
    assert.equal(result.status, 1);
    assert.equal(result.stdout, "");
    assert.match(result.stderr, /CLI is installed, but service verification failed/);
    assert.doesNotMatch(result.stderr, /existing installation preserved/);
  }
});
test("failed bootstrap does not start or stop services", nativeMac, async () => {
  const result = await execute("failed", [ready]);
  assert.equal(result.status, 1);
  assert.deepEqual(result.calls, ["--version", "__bootstrap-update"]);
});
test("Windows postflight covers update and same-binary repair without starting first-install login", () => {
  assert.match(windows, /'installed' \{\}/);
  assert.match(windows, /'updated' \{ Confirm-OneCrewService \}/);
  assert.match(windows, /'already_current' \{ Confirm-OneCrewService \}/);
  for (const fragment of ["$status.version -ceq $version", "$status.binary_sha256 -ceq $sha", "$status.startup.phase -ceq 'ready'", "$status.data.state -ceq 'ready'", "Invoke-OneCrewCommand $installed 'stop --json' $remaining", "[DateTime]::UtcNow.AddSeconds(60)"]) assert.ok(windows.includes(fragment));
});
test("Windows diagnostics use stderr while successful bootstrap keeps its original output", () => {
  assert.match(windows, /\[Console\]::Out.Write\(\$result.Stdout\)\s+Write-ConnectionGuidance/);
  assert.match(windows, /\[Console\]::Error.WriteLine\(\$downloadFailure\)/);
  assert.doesNotMatch(windows, /Write-Output 'OneCrew .*failed/);
  for (const code of ["download_refresh_required", "download_access_denied", "download_retryable"]) assert.ok(windows.includes(code));
});
