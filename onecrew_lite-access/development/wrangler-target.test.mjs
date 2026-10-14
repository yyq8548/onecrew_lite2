import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  assertSafeDeployArgs,
  defaultWranglerConfig,
  parseWranglerTargetArgs,
  repositoryRoot,
  validateAccessWranglerConfig,
  WranglerTargetError,
  wranglerTargetArgs,
} from "./wrangler-target.mjs";

test("validates the pinned Access config and explicit auth profile", async () => {
  const target = parseWranglerTargetArgs(
    ["--", "--config", "wrangler.toml", "--profile=onecrew"],
    { cwd: repositoryRoot },
  );
  assert.equal(target.configPath, defaultWranglerConfig);
  assert.equal(target.profile, "onecrew");
  assert.deepEqual(wranglerTargetArgs(target), [
    "--config",
    "wrangler.toml",
    "--profile",
    "onecrew",
  ]);

  const validated = await validateAccessWranglerConfig(target.configPath);
  assert.equal(validated.accessOrigin, "https://access.onecrewai.io");
  assert.equal(validated.routeHost, "access.onecrewai.io");
  assert.equal(validated.creatorSyncApiOrigin, "https://api.onecrewai.io");
  assert.equal(validated.databaseId, "c373bd25-4aef-4b00-96e0-b88d95133f9f");
});

test("defaults to and only accepts the onecrew auth profile", () => {
  const target = parseWranglerTargetArgs([]);
  assert.equal(target.profile, "onecrew");
  assert.deepEqual(wranglerTargetArgs(target), [
    "--config",
    "wrangler.toml",
    "--profile",
    "onecrew",
  ]);
  assert.throws(
    () => parseWranglerTargetArgs(["--profile", "default"]),
    WranglerTargetError,
  );
});

test("pins the credential verifier to Sync's default entrypoint and requires request cancellation support", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "access-wrangler-verifier-"));
  t.after(() => rm(directory, { force: true, recursive: true }));
  const source = (await readFile(defaultWranglerConfig, "utf8")).replace(/\r\n/g, "\n");
  const configPath = join(directory, "wrangler.toml");
  for (const changed of [
    source.replace(
      'binding = "SYNC_CREDENTIAL_VERIFIER"\nservice = "sync"',
      'binding = "SYNC_CREDENTIAL_VERIFIER"\nservice = "other"',
    ),
    source.replace(
      'binding = "SYNC_CREDENTIAL_VERIFIER"',
      'binding = "SYNC_CREDENTIAL_VERIFIER"\nentrypoint = "WrongOwner"',
    ),
    source.replace(
      'compatibility_flags = ["enable_request_signal"]',
      "compatibility_flags = []",
    ),
    source.replace(
      'CREATOR_SYNC_API_ORIGIN = "https://api.onecrewai.io"',
      'CREATOR_SYNC_API_ORIGIN = "https://api.onecrewai.io/path"',
    ),
    source.replace(
      'CREATOR_SYNC_API_ORIGIN = "https://api.onecrewai.io"',
      'CREATOR_SYNC_API_ORIGIN = "https://sync.example.workers.dev"',
    ),
    source.replace(
      'CREATOR_SYNC_API_ORIGIN = "https://api.onecrewai.io"',
      'CREATOR_SYNC_API_ORIGIN = "https://api.other.example"',
    ),
  ]) {
    assert.notEqual(changed, source, "negative fixture must change the configuration");
    await writeFile(configPath, changed);
    await assert.rejects(
      validateAccessWranglerConfig(configPath),
      WranglerTargetError,
    );
  }
});

test("rejects a config that could select the retired D1 target", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "access-wrangler-target-"));
  t.after(() => rm(directory, { force: true, recursive: true }));
  const source = await readFile(defaultWranglerConfig, "utf8");
  const configPath = join(directory, "wrangler.toml");
  await writeFile(
    configPath,
    source.replace(
      "c373bd25-4aef-4b00-96e0-b88d95133f9f",
      "0493befd-0d56-4f68-bf02-b89003f443d9",
    ),
  );
  await assert.rejects(
    validateAccessWranglerConfig(configPath),
    WranglerTargetError,
  );
});

test("rejects an origin that differs from the custom-domain route", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "access-wrangler-origin-"));
  t.after(() => rm(directory, { force: true, recursive: true }));
  const source = await readFile(defaultWranglerConfig, "utf8");
  const configPath = join(directory, "wrangler.toml");
  await writeFile(
    configPath,
    source.replace(
      'ACCESS_ORIGIN = "https://access.onecrewai.io"',
      'ACCESS_ORIGIN = "https://other.example"',
    ),
  );
  await assert.rejects(
    validateAccessWranglerConfig(configPath),
    WranglerTargetError,
  );
});

test("rejects target-changing deploy options and duplicate selectors", () => {
  assert.throws(
    () => assertSafeDeployArgs(["--var", "CREATOR_SYNC_API_ORIGIN:https://other.example"]),
    WranglerTargetError,
  );
  assert.doesNotThrow(() =>
    assertSafeDeployArgs(["--message", "Access release", "--strict"]),
  );
  assert.throws(
    () => assertSafeDeployArgs(["--name", "another-worker"]),
    WranglerTargetError,
  );
  assert.throws(
    () => assertSafeDeployArgs(["--var", "WEB_ORIGIN:https://other.example"]),
    WranglerTargetError,
  );
  assert.throws(
    () => parseWranglerTargetArgs(["--profile", "one", "--profile", "two"]),
    WranglerTargetError,
  );
  assert.throws(
    () => assertSafeDeployArgs(["src/other-worker.ts"]),
    WranglerTargetError,
  );
});
