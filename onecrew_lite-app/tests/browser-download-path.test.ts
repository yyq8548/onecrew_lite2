import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import test from "node:test";
import { createOwnedChromeProfile } from "../src/platforms/browser/chrome-host.ts";
import { createPrivateDirectory, verifyPrivateDirectory } from "../src/storage/private-backup.ts";
import { managedChromeProfilesDirectory, prepareManagedChromeProfilesDirectory } from "../src/platforms/browser/profile-directory.ts";
import { verifyAccountOwnedChromeDirectory } from "../src/platforms/browser/binding.ts";

test("managed Windows Chrome can create real webstore downloads without long-path opt-in", { skip: process.platform !== "win32", timeout: 60_000 }, async () => {
  const execute = promisify(execFile);
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/cdp-", import.meta.url)));
  let profiles: string | undefined;
  try {
    const executable = join(root, "download-path.exe");
    await execute(join(process.env.SystemRoot!, "Microsoft.NET", "Framework64", "v4.0.30319", "csc.exe"), ["/nologo", `/out:${executable}`, `/win32manifest:${fileURLToPath(new URL("./chrome-download-path.manifest", import.meta.url))}`, fileURLToPath(new URL("./chrome-download-path.cs", import.meta.url))], { windowsHide: true });
    profiles = await prepareManagedChromeProfilesDirectory(root);
    const profile = await createOwnedChromeProfile(profiles);
    await verifyPrivateDirectory(profile);
    assert.equal((await verifyAccountOwnedChromeDirectory(root, profile)).path, profile);
    assert.notEqual(profiles, managedChromeProfilesDirectory(join(root, "other-account")));
    await assert.rejects(verifyAccountOwnedChromeDirectory(join(root, "other-account"), profile), /outside/u);
    const downloads = join(profile, "Webstore Downloads");
    await mkdir(downloads);
    const target = join(downloads, "ildkmabpimmkaediidaifkhjpohdnifk_65534.crx");
    await execute(executable, [target], { windowsHide: true });
    await execute(executable, [`${target}.crdownload`], { windowsHide: true });
    await execute(executable, [`${target}:Zone.Identifier`], { windowsHide: true });
    const cache = join(profile, "GPUPersistentCache", "DawnGraphiteCache", "N".repeat(32));
    await mkdir(cache, { recursive: true });
    await execute(executable, [join(cache, "cache_data_0")], { windowsHide: true });
    const locale = join(profile, "Default", "Extensions", "ildkmabpimmkaediidaifkhjpohdnifk", "1.0.24_0", "_locales", "en_US");
    await mkdir(locale, { recursive: true });
    await execute(executable, [join(locale, "messages.json")], { windowsHide: true });
    const tooLong = await createPrivateDirectory(join(root, "q".repeat(125 - root.length)));
    await assert.rejects(createOwnedChromeProfile(tooLong), /browser.profile_path_too_long/u);
  } finally {
    if (profiles) await rm(profiles, { recursive: true, maxRetries: 3, retryDelay: 100 });
    await rm(root, { recursive: true, maxRetries: 3, retryDelay: 100 });
  }
});
