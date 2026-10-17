import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import test from "node:test";
import { launchOwnedChrome } from "../src/platforms/browser/chrome-host.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("actual Mac Chrome uses an isolated private profile and closes its owned process", { skip: process.platform !== "darwin", timeout: 60_000 }, async () => {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/owned-chrome-mac-", import.meta.url)));
  try {
    const profileDirectory = await createPrivateDirectory(join(root, "profile"));
    const browser = await launchOwnedChrome({ profileDirectory, url: new URL("about:blank"), signal: new AbortController().signal });
    try {
      browser.assertRunning();
      const { stdout } = await promisify(execFile)("/bin/ps", ["-axo", "command="]);
      const commands = stdout.split("\n").filter(line => line.includes(`--user-data-dir=${profileDirectory}`));
      assert.ok(commands.length > 0);
      for (const command of commands) {
        assert.doesNotMatch(command, /--disable-background-networking|--disable-component-update|--disable-client-side-phishing-detection/u);
      }
      assert.throws(() => JSON.stringify(browser), /process_authority_not_serializable/);
    } finally { await browser.close(); }
    assert.throws(() => browser.assertRunning(), /owned_process_not_running/);
    await browser.close();
  } finally { await rm(root, { recursive: true, maxRetries: 3, retryDelay: 100 }); }
});

test("actual Windows Chrome ownership inspection and graceful close preserve executable hash checks", { skip: process.platform !== "win32", timeout: 60_000 }, async () => {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/owned-chrome-native-", import.meta.url)));
  try {
    const profileDirectory = await createPrivateDirectory(join(root, "profile"));
    const previousDiagnostics = process.env.ONECREW_BROWSER_DIAGNOSTICS;
    process.env.ONECREW_BROWSER_DIAGNOSTICS = "1";
    const browser = await launchOwnedChrome({ profileDirectory, url: new URL("about:blank"), signal: new AbortController().signal });
    if (previousDiagnostics === undefined) delete process.env.ONECREW_BROWSER_DIAGNOSTICS;
    else process.env.ONECREW_BROWSER_DIAGNOSTICS = previousDiagnostics;
    browser.assertRunning();
    const execute = promisify(execFile);
    try {
      const actual = await execute(join(process.env.SystemRoot!, "System32", "WindowsPowerShell", "v1.0", "powershell.exe"), ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", fileURLToPath(new URL("./chrome-launch-arguments.ps1", import.meta.url)), "-ProfileDirectory", profileDirectory], { windowsHide: true });
      const { commandLine } = JSON.parse(actual.stdout) as { commandLine: string };
      assert.doesNotMatch(commandLine, /--disable-background-networking|--disable-component-update|--disable-client-side-phishing-detection/u);
      assert.match(commandLine, /--force-renderer-accessibility=complete/u);
    } finally { await browser.close(); }
    assert.throws(() => browser.assertRunning(), /owned_process_not_running/);
    await assert.rejects(execute(join(process.env.SystemRoot!, "System32", "WindowsPowerShell", "v1.0", "powershell.exe"), ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", fileURLToPath(new URL("../src/platforms/browser/browser-host.ps1", import.meta.url)), "-Operation", "Inspect", "-ProcessId", String(process.pid), "-Executable", process.execPath, "-ExecutableSha256", "0".repeat(64)], { windowsHide: true }), /Owned browser executable changed/);
  } finally { await rm(root, { recursive: true, maxRetries: 3, retryDelay: 100 }); }
});
