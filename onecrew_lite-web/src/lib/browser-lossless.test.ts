import { expect, it } from "vitest";
import { parseApplicationJson, stringifyApplicationJson } from "@onecrew/runtime-contracts/json";
import { componentsReport, discoveryDraftProgress, discoveryExecutionStatus, discoveryIdentityMergeMaterial, discoveryReviewMaterial, discoveryReviewReceipt } from "@onecrew/runtime-contracts/discovery";
import { formatCount } from "./creator";
import { primaryFollowerEntry } from "@/pages/leads/creator-followers";
import { xReviewFixture } from "./discovery-review.fixture";
import { migrationImpact, localServiceStatus } from "@onecrew/runtime-contracts/status";

it("keeps u64 followers through Creator formatting and full discovery evidence", () => {
  const maximum = 18_446_744_073_709_551_615n;
  const material = { ...xReviewFixture(), native_followers: maximum, native_follower_estimate: { display: "18E", nominal: maximum, lower: maximum - 1n, upper: maximum } };
  const decoded = parseApplicationJson(stringifyApplicationJson(material));
  expect(discoveryReviewMaterial.parse(decoded).native_followers).toBe(maximum);
  expect(discoveryIdentityMergeMaterial.parse({ format_version: 1, campaign_id: "campaign", criteria_revision: 1, accounts: [decoded, decoded], material_hash: "hash" }).accounts[1].native_follower_estimate?.upper).toBe(maximum);
  expect(primaryFollowerEntry({ primary_platform: "x", platforms: ["x"], follower_counts: { x: maximum } })).toEqual(["x", maximum]);
  expect(formatCount(maximum, "en-US")).toBe(new Intl.NumberFormat("en-US", { maximumFractionDigits: 1, notation: "compact" }).format(maximum));
  expect(discoveryReviewMaterial.safeParse({ ...material, native_followers: maximum + 1n }).success).toBe(false);
  expect(discoveryReviewReceipt.safeParse({ subscribers: maximum }).success).toBe(false);
});

it("preserves every migration impact counter without accepting unknown or absent fields", () => {
  const maximum = 18_446_744_073_709_551_615n;
  const impact = Object.fromEntries(Object.keys(migrationImpact.shape).map(key => [key, maximum]));
  expect(migrationImpact.parse(parseApplicationJson(stringifyApplicationJson(impact)))).toEqual(impact);
  expect(migrationImpact.safeParse({ ...impact, private_path: "not permitted" }).success).toBe(false);
  const incomplete = { ...impact }; delete incomplete.removed_contact_confirmations;
  expect(migrationImpact.safeParse(incomplete).success).toBe(false);
  expect(localServiceStatus.safeParse({ running: true }).success).toBe(false);
});

it("accepts exact u64 progress durations and component timestamps", () => {
  const maximum = 18_446_744_073_709_551_615n;
  const progress = { id: "request", phase: "planning", elapsed_ms: maximum, phase_elapsed_ms: maximum, timings: [{ phase: "checking_connection", elapsed_ms: maximum }], error_code: null, unsupported_conditions: [] };
  expect(discoveryDraftProgress.parse(parseApplicationJson(stringifyApplicationJson(progress)))).toEqual(progress);
  const report = { binary_sha256: "hash", built_at: maximum, checked_at: maximum, library_state: "ready", library_version: "1", opencli_state: "ready", opencli_version: "1", plugin_state: "ready", plugin_version: "1", version: "1" };
  expect(componentsReport.parse(parseApplicationJson(stringifyApplicationJson(report)))).toEqual(report);
  const execution = { campaign_id: "campaign", revision: 1, stage: "ready", used: 0, deferred_hits: [], deferred_scores: 0, excluded_platforms: [], failure_code: null, failure_phase: null, reason: null, sync_error: null, sync_pending: false, synced_at: maximum, updated_at: maximum, library: null };
  expect(discoveryExecutionStatus.parse(parseApplicationJson(stringifyApplicationJson(execution)))).toEqual(execution);
});
