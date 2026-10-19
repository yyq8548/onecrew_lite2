import assert from "node:assert/strict";
import { copyFile, mkdir, mkdtemp, appendFile, readFile, rm } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { supplementalLicense } from "../src/packaging/supplemental-license.ts";
import { createPayloadManifest, parsePayloadManifest, payloadDigest } from "../src/packaging/payload-manifest.ts";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import JSZip from "jszip";

const root = fileURLToPath(new URL("../../", import.meta.url));
const dependency = (name: string) => join(root, "node_modules", ".pnpm", name, "node_modules");

test("verified supplemental licenses use real pinned dependency texts and preserve missing private licenses", async () => {
  const sources = [
    join(dependency("isarray@1.0.0"), "isarray"),
    join(dependency("saxes@5.0.1"), "saxes"),
    join(dependency("boolbase@1.0.0"), "boolbase"),
    join(dependency("buffers@0.1.1"), "buffers"),
  ];
  const binding = process.platform === "darwin" ? `darwin-${process.arch}` : process.platform === "win32" && process.arch === "x64" ? "win32-x64-msvc" : null;
  if (binding) sources.push(join(dependency(`@wreq-js+binding-${binding}@3.2.0`), "@wreq-js", `binding-${binding}`));
  for (const directory of sources) {
    const evidence = await supplementalLicense(directory);
    assert.ok(evidence);
    const bytes = await readFile(evidence.path);
    assert.equal(payloadDigest(bytes), evidence.sha256);
    assert.equal(new URL(evidence.source).protocol, "https:");
    assert.match(bytes.toString("utf8"), /Permission (?:to use|is hereby granted)/u);
  }
  assert.equal(await supplementalLicense(join(root, "onecrew_lite-app")), null);
  assert.equal((await supplementalLicense(join(dependency("buffers@0.1.1"), "buffers")))?.license, "MIT");
});

test("the real candidate archive retains verified license bytes and rejects changed provenance", { skip: !process.env.ONECREW_SEA_CANDIDATE_EXECUTABLE }, async () => {
  const directory = dirname(resolve(process.env.ONECREW_SEA_CANDIDATE_EXECUTABLE!));
  const manifest = parsePayloadManifest(await readFile(join(directory, "runtime-manifest.json")));
  const archive = parseApplicationJson(await readFile(join(directory, "archive.json"))) as { path: string; sha256: string };
  const bytes = await readFile(join(directory, archive.path));
  assert.equal(payloadDigest(bytes), archive.sha256);
  const zip = await JSZip.loadAsync(bytes);
  const supplemental = manifest.components.filter(component => component.licenseProvenance);
  assert.equal(supplemental.length, 5);
  assert.equal(manifest.components.filter(component => component.licenseEvidence === "declared_only").length, 11);
  for (const component of supplemental) {
    assert.equal(component.licenseEvidence, "full_text");
    for (const evidence of component.licenseProvenance!) {
      const entry = zip.file(evidence.file); assert.ok(entry);
      assert.equal(payloadDigest(await entry.async("nodebuffer")), evidence.sha256);
    }
  }
  const { payloadSha256: _, ...base } = manifest;
  const changed = structuredClone(base);
  changed.components.find(component => component.licenseProvenance)!.licenseProvenance![0]!.sha256 = "0".repeat(64);
  assert.throws(() => createPayloadManifest(changed), { message: "package.payload_license_provenance_invalid" });
});

test("a changed real package README fails instead of retaining its previous full-text proof", async () => {
  const scratch = join(root, ".scratch", "license-tests");
  await mkdir(scratch, { recursive: true }); const directory = await mkdtemp(join(scratch, "changed-"));
  try {
    const source = join(dependency("isarray@1.0.0"), "isarray");
    for (const name of ["package.json", "README.md"]) await copyFile(join(source, name), join(directory, name));
    assert.ok(await supplementalLicense(directory));
    await appendFile(join(directory, "README.md"), "\n");
    await assert.rejects(supplementalLicense(directory), { message: "package.supplemental_license_changed" });
  } finally { await rm(directory, { recursive: true }); }
});
