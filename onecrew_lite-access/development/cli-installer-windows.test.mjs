import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import test from "node:test";

// Exercise the production command runner on Windows PowerShell 5.1, without
// downloading OneCrew, changing client configuration or launching its service.
test("Windows native runner preserves streams, arguments, exit codes and bounds hangs", { skip: process.platform !== "win32" }, async () => {
  const root = await mkdtemp(join(tmpdir(), "onecrew-powershell-"));
  try {
    const stage = join(root, "user's command output");
    await mkdir(stage);
    const fixture = join(root, "native fixture.cjs");
    await writeFile(fixture, `
if (process.argv[2] === 'hang') { setInterval(() => {}, 1000); }
else {
  process.stdout.write(JSON.stringify({args:process.argv.slice(2)}));
  process.stderr.write('onecrew: service_unavailable\\n');
  process.exitCode = Number(process.argv[2]);
}
`);
    const script = join(root, "test.ps1");
    await writeFile(script, `
$ErrorActionPreference = 'Stop'
$tokens = $null
$parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($env:TEST_INSTALLER, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw ($parseErrors | Out-String) }
$definition = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Invoke-OneCrewCommand' }, $true)
Invoke-Expression $definition.Extent.Text
$stage = $env:TEST_STAGE
$arguments = '"{0}" 0 "a path with spaces" "user''s file"' -f $env:TEST_FIXTURE
$success = Invoke-OneCrewCommand $env:TEST_NODE $arguments 10
if ($success.ExitCode -ne 0) { throw 'lost success exit code' }
if ($success.Stderr.Trim() -ne 'onecrew: service_unavailable') { throw 'lost native stderr' }
$value = $success.Stdout | ConvertFrom-Json
if ($value.args[1] -ne 'a path with spaces' -or $value.args[2] -ne "user's file") { throw 'argument quoting changed' }
$failure = Invoke-OneCrewCommand $env:TEST_NODE ('"{0}" 7' -f $env:TEST_FIXTURE) 10
if ($failure.ExitCode -ne 7) { throw 'lost native failure exit code' }
$clock = [Diagnostics.Stopwatch]::StartNew()
$timedOut = $false
try { $null = Invoke-OneCrewCommand $env:TEST_NODE ('"{0}" hang' -f $env:TEST_FIXTURE) 1 }
catch { if ($_.Exception.Message -ne 'command_timeout') { throw }; $timedOut = $true }
if (-not $timedOut -or $clock.Elapsed.TotalSeconds -gt 10) { throw 'unbounded native command' }
Write-Output 'windows-runner-ok'
`);
    const result = spawnSync(join(process.env.SystemRoot, "System32/WindowsPowerShell/v1.0/powershell.exe"), ["-NoProfile", "-NonInteractive", "-File", script], {
      encoding: "utf8", timeout: 30000,
      env: { ...process.env, TEST_INSTALLER: fileURLToPath(new URL("../src/cli-distribution/install.ps1.txt", import.meta.url)),
        TEST_STAGE: stage, TEST_NODE: process.execPath, TEST_FIXTURE: fixture },
    });
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /windows-runner-ok/);
  } finally { await rm(root, { recursive: true, force: true }); }
});
