import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";

import {
  containsUnsafeAnyType,
  generateValidator,
  compareDirectories,
  replaceDirectory,
  validateManifest,
} from "./generate.mjs";

test("unsafe type detection distinguishes any types from enum literals and docs", () => {
  assert.equal(containsUnsafeAnyType('export type Filter = "any" | "include" | "exclude";'), false);
  assert.equal(containsUnsafeAnyType('/** any source */ export interface X { any: unknown; }'), false);
  assert.equal(containsUnsafeAnyType('export interface X { value: any; }'), true);
  assert.equal(containsUnsafeAnyType('export type X = Record<string, any>;'), true);
});

test("shared-reference validators retain strict objects and numeric bounds", async () => {
  await withTemporaryDirectory(async (directory) => {
    const id = "https://reacher.local/contracts/Example.schema.json";
    await generateValidator({ name: "Example", id, schema: {
      $id: id, type: "object", additionalProperties: false, required: ["filter"],
      properties: { filter: { $ref: "#/$defs/Filter" } },
      $defs: { Filter: { type: "object", additionalProperties: false, required: ["mode", "count", "metric"], properties: {
        mode: { enum: ["any", "include"] },
        count: { type: "integer", format: "uint8", minimum: 0, maximum: 255 },
        metric: { type: "number", format: "double", minimum: 0, maximum: 1 },
      } } },
    } }, directory);
    const { validateExample } = await import(pathToFileURL(join(directory, "Example.js")).href);
    const filter = { mode: "any", count: 10, metric: 0.5 };
    assert.equal(validateExample({ filter }), true);
    assert.equal(validateExample({ filter, extra: true }), false);
    assert.equal(validateExample({ filter: { ...filter, extra: true } }), false);
    assert.equal(validateExample({ filter: { ...filter, count: 256 } }), false);
    assert.equal(validateExample({ filter: { ...filter, metric: NaN } }), false);
  });
});

async function withTemporaryDirectory(run) {
  const directory = await mkdtemp(join(tmpdir(), "onecrew-schema-codegen-"));
  try {
    await run(directory);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

function validManifest() {
  return {
    schema_version: 1,
    target: "test-target",
    manifest_revision: "sha256:manifest",
    roots: [
      {
        name: "Example",
        direction: "serialize",
        id: "https://reacher.local/contracts/Example.schema.json",
        revision: "sha256:root",
        schema: {
          $id: "https://reacher.local/contracts/Example.schema.json",
          type: "object",
        },
      },
    ],
  };
}

test("manifest validation fails closed for duplicate roots and wrong identity", () => {
  const manifest = validManifest();
  assert.equal(validateManifest(manifest, "test-target"), manifest);

  assert.throws(
    () => validateManifest({ ...manifest, target: "other" }, "test-target"),
    /Invalid schema manifest/,
  );
  assert.throws(
    () => validateManifest({ ...manifest, roots: [...manifest.roots, manifest.roots[0]] }, "test-target"),
    /Invalid schema root Example/,
  );
  assert.throws(
    () => validateManifest({ ...manifest, roots: [{ ...manifest.roots[0], direction: "both" }] }, "test-target"),
    /Invalid schema root Example/,
  );
  assert.throws(
    () => validateManifest({ ...manifest, roots: [{ ...manifest.roots[0], schema: { ...manifest.roots[0].schema, $id: "wrong" } }] }, "test-target"),
    /Invalid schema root Example/,
  );
});

test("directory comparison is deterministic and never mutates expected output", async () => {
  await withTemporaryDirectory(async (directory) => {
    const expected = join(directory, "expected");
    const actual = join(directory, "actual");
    await mkdir(join(expected, "nested"), { recursive: true });
    await mkdir(join(actual, "nested"), { recursive: true });
    await writeFile(join(expected, "nested", "contract.ts"), "stable\n");
    await writeFile(join(actual, "nested", "contract.ts"), "stable\n");

    assert.deepEqual(await compareDirectories(expected, actual), []);
    await writeFile(join(actual, "nested", "contract.ts"), "drifted\n");
    assert.deepEqual(
      await compareDirectories(expected, actual),
      [join("nested", "contract.ts")],
    );
    assert.equal(await readFile(join(expected, "nested", "contract.ts"), "utf8"), "stable\n");
  });
});

test("atomic replacement installs only the complete staged directory", async () => {
  await withTemporaryDirectory(async (directory) => {
    const output = join(directory, "contracts");
    const staging = join(directory, "staging");
    await mkdir(output);
    await mkdir(staging);
    await writeFile(join(output, "old.txt"), "old\n");
    await writeFile(join(staging, "new.txt"), "new\n");

    await replaceDirectory(output, staging);

    assert.equal(await readFile(join(output, "new.txt"), "utf8"), "new\n");
    await assert.rejects(readFile(join(output, "old.txt"), "utf8"));
    await assert.rejects(readFile(`${output}.backup`, "utf8"));
  });
});

test("replacement failure restores the prior complete output", async () => {
  await withTemporaryDirectory(async (directory) => {
    const output = join(directory, "contracts");
    const staging = join(directory, "staging");
    await mkdir(output);
    await mkdir(staging);
    await writeFile(join(output, "old.txt"), "old\n");
    await writeFile(join(staging, "new.txt"), "new\n");
    let moveCount = 0;

    await assert.rejects(
      replaceDirectory(output, staging, {
        rename: async (from, to) => {
          moveCount += 1;
          if (moveCount === 2) throw new Error("simulated replacement failure");
          await rename(from, to);
        },
        rm,
      }),
      /simulated replacement failure/,
    );

    assert.equal(await readFile(join(output, "old.txt"), "utf8"), "old\n");
    assert.equal(await readFile(join(staging, "new.txt"), "utf8"), "new\n");
    await assert.rejects(readFile(`${output}.backup`, "utf8"));
  });
});
