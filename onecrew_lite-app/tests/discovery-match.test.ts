import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { z } from "zod";
import { parseApplicationJson } from "../src/contracts/application-json.ts";
import { buildDiscoveryMatchPrompt, discoveryMatchEvidenceKey, discoveryMatchPromptMaterial, loadBundledDiscoveryMatchRubric } from "../src/discovery/semantic-match.ts";
import type { DiscoveryReviewMaterial } from "../src/discovery/review-material.ts";

test("真实旧评分材料使用GitHub新创作者规则，幂等键与旧规则回执隔离", async () => {
  const fixture = z.object({ count: z.number(), objective: z.string(), material: z.record(z.string(), z.unknown()), prompt_sha256: z.string(), request_id: z.string(), reassess_request_id: z.string() }).parse(parseApplicationJson(await readFile(new URL("./fixtures/discovery-match-rust.json", import.meta.url))));
  const material = fixture.material as DiscoveryReviewMaterial, rubric = await loadBundledDiscoveryMatchRubric();
  const automatic = discoveryMatchEvidenceKey(material, fixture.count, rubric, false);
  const reassess = discoveryMatchEvidenceKey(material, fixture.count, rubric, true);
  assert.notEqual(automatic, fixture.request_id);
  assert.notEqual(reassess, fixture.reassess_request_id);
  assert.notEqual(automatic, reassess);
  assert.equal(discoveryMatchEvidenceKey(material, fixture.count, rubric, false), automatic);
  assert.equal(createHash("sha256").update(rubric).digest("hex"), "694f0cfbee3ebe89df2cdb44b93aca6bce31076048a6eec83651ee265c9a6a67");
  const projection = discoveryMatchPromptMaterial(material, fixture.count);
  assert.equal(projection.language_samples, null);
  const prompt = buildDiscoveryMatchPrompt(fixture.objective, material, projection, rubric);
  assert.notEqual(createHash("sha256").update(prompt).digest("hex"), fixture.prompt_sha256);
  assert.ok(prompt.includes("the saved evidence concerns this author's relevant creator activity"));
  assert.ok(prompt.includes("apply any explicit customer allowance for curators or aggregators"));
});
