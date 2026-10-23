import type { DatabaseSync } from "node:sqlite";
import { rebuildCreatorTableChanges } from "./creator-schema.ts";

const douyinCompleted = "OR (status='completed' AND target_platform='douyin' AND native_author_id IS NOT NULL AND length(native_author_id) BETWEEN 19 AND 99 AND substr(native_author_id,1,12)='MS4wLjABAAAA' AND native_author_id NOT GLOB '*[^a-zA-Z0-9_-]*' AND native_material_json IS NOT NULL AND json_valid(native_material_json) AND json_type(native_material_json)='object' AND length(CAST(native_material_json AS BLOB))<=65536 AND json_extract(native_material_json,'$.content.author_id') IS native_author_id AND json_extract(native_material_json,'$.profile.author_id') IS native_author_id AND json_extract(native_material_json,'$.content.post_id') IS video_id AND json_extract(native_material_json,'$.content.source_url') IS source_url AND json_extract(native_material_json,'$.profile.profile_url') IS ('https://www.douyin.com/user/' || native_author_id) AND json_extract(native_material_json,'$.content.profile_url') IS json_extract(native_material_json,'$.profile.profile_url') AND stop_code IS NULL) OR (status='failed'";

// 原 v81→82、v82→83 的约束变更；表行先复制，显式对象在恢复数据后重建。
export function migrateCreatorWebSourceEdge(db: DatabaseSync, version: 77 | 81 | 82, reverse = false): void {
  const changes: Array<readonly [string, string]> = version === 81 ? [
    ["json_extract(native_material_json,'$.identity.source_url') IS source_url", "json_extract(native_material_json,'$.identity.source_url') IS lower(source_url)"],
  ] : version === 82 ? [
    ["CHECK(target_platform IN ('youtube','twitter'))", "CHECK(target_platform IN ('youtube','twitter','douyin'))"],
    ["target_platform='twitter' AND length(video_id)", "target_platform IN ('twitter','douyin') AND length(video_id)"],
    ["OR (status='failed'", douyinCompleted],
  ] : [
    ["CHECK(target_platform='youtube')", "CHECK(target_platform IN ('youtube','twitter'))"],
    ["CHECK(length(video_id)=11 AND video_id NOT GLOB '*[^a-zA-Z0-9_-]*')", "CHECK((target_platform='youtube' AND length(video_id)=11 AND video_id NOT GLOB '*[^a-zA-Z0-9_-]*') OR (target_platform='twitter' AND length(video_id) BETWEEN 1 AND 20 AND substr(video_id,1,1)!='0' AND video_id NOT GLOB '*[^0-9]*'))"],
    ["OR (status='completed' AND native_author_id IS NOT NULL AND length(native_author_id)=24", "OR (status='completed' AND target_platform='youtube' AND native_author_id IS NOT NULL AND length(native_author_id)=24"],
    ["OR (status='failed'", "OR (status='completed' AND target_platform='twitter' AND native_author_id IS NOT NULL AND length(native_author_id) BETWEEN 1 AND 20 AND substr(native_author_id,1,1)!='0' AND native_author_id NOT GLOB '*[^0-9]*' AND native_material_json IS NOT NULL AND json_valid(native_material_json) AND json_type(native_material_json)='object' AND length(CAST(native_material_json AS BLOB))<=65536 AND json_extract(native_material_json,'$.identity.account_id') IS native_author_id AND json_extract(native_material_json,'$.identity.post_id') IS video_id AND json_extract(native_material_json,'$.identity.source_url') IS source_url AND json_extract(native_material_json,'$.profile.account_id') IS native_author_id AND json_extract(native_material_json,'$.content.identity') IS json_extract(native_material_json,'$.identity') AND stop_code IS NULL) OR (status='failed'"],
  ];
  rebuildCreatorTableChanges(db, "discovery_web_source_reads", reverse ? changes.map(([from, to]) => [to, from] as const) : changes);
}
