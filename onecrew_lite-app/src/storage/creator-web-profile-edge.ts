import type { DatabaseSync } from "node:sqlite";
import { rebuildCreatorTableChanges } from "./creator-schema.ts";

export function migrateCreatorWebProfileEdge(db: DatabaseSync): void {
  const identifiers = [
    "(target_platform='youtube' AND length(video_id)=24 AND substr(video_id,1,2)='UC' AND video_id NOT GLOB '*[^a-zA-Z0-9_-]*')",
    "(target_platform='twitter' AND length(video_id) BETWEEN 2 AND 16 AND substr(video_id,1,1)='@' AND substr(video_id,2) NOT GLOB '*[^a-z0-9_]*')",
    "(target_platform='linkedin' AND length(video_id) BETWEEN 1 AND 512 AND video_id NOT GLOB '*[^a-zA-Z0-9_%-]*' AND lower(video_id) NOT IN ('me','self'))",
    "(target_platform IN ('bilibili','weibo') AND length(video_id) BETWEEN 1 AND 20 AND substr(video_id,1,1)!='0' AND video_id NOT GLOB '*[^0-9]*')",
  ].join(" OR ");
  const completion = [
    "OR (status='completed' AND native_author_id IS NOT NULL AND native_material_json IS NOT NULL AND json_valid(native_material_json) AND json_type(native_material_json)='object' AND length(CAST(native_material_json AS BLOB))<=65536",
    "AND json_extract(native_material_json,'$.identity.evidence_source') IS 'native_profile_link' AND json_extract(native_material_json,'$.identity.account_id') IS native_author_id",
    "AND json_extract(native_material_json,'$.identity.profile_url') IS (CASE target_platform WHEN 'linkedin' THEN 'https://www.linkedin.com/in/'||video_id||'/' WHEN 'bilibili' THEN 'https://space.bilibili.com/'||video_id WHEN 'weibo' THEN 'https://weibo.com/u/'||video_id WHEN 'twitter' THEN 'https://x.com/'||substr(video_id,2) WHEN 'youtube' THEN 'https://www.youtube.com/channel/'||video_id END)",
    "AND json_extract(native_material_json,CASE WHEN target_platform='youtube' THEN '$.profile.url' ELSE '$.profile.profile_url' END) IS json_extract(native_material_json,'$.identity.profile_url')",
    "AND ((target_platform='linkedin' AND length(native_author_id) BETWEEN 19 AND 128 AND substr(native_author_id,1,3)='ACo' AND native_author_id NOT GLOB '*[^a-zA-Z0-9_-]*' AND json_extract(native_material_json,'$.profile.account_id') IS native_author_id AND json_extract(native_material_json,'$.profile.author_handle') IS video_id)",
    "OR (target_platform='bilibili' AND json_extract(native_material_json,'$.profile.mid') IS native_author_id AND native_author_id IS video_id)",
    "OR (target_platform='weibo' AND json_extract(native_material_json,'$.profile.id') IS native_author_id AND native_author_id IS video_id)",
    "OR (target_platform='twitter' AND length(native_author_id) BETWEEN 1 AND 20 AND substr(native_author_id,1,1)!='0' AND native_author_id NOT GLOB '*[^0-9]*' AND json_extract(native_material_json,'$.profile.account_id') IS native_author_id AND ('@'||json_extract(native_material_json,'$.profile.handle')) IS video_id AND json_extract(native_material_json,'$.profile.adapter_version') IS '1.0.0' AND json_extract(native_material_json,'$.profile.identity_kind') IS 'native_id' AND json_extract(native_material_json,'$.profile.evidence_source') IS 'x_user_by_screen_name' AND json_type(native_material_json,'$.profile.name') IS 'text' AND length(trim(json_extract(native_material_json,'$.profile.name')))>0 AND json_type(native_material_json,'$.profile.bio') IS 'text' AND json_type(native_material_json,'$.profile.self_declared_location') IS 'text' AND (json_type(native_material_json,'$.profile.follower_count') IS 'null' OR (json_type(native_material_json,'$.profile.follower_count') IS 'integer' AND json_extract(native_material_json,'$.profile.follower_count')>=0)))",
    "OR (target_platform='youtube' AND length(video_id)=24 AND substr(video_id,1,2)='UC' AND video_id NOT GLOB '*[^a-zA-Z0-9_-]*' AND json_extract(native_material_json,'$.profile.channel_id') IS native_author_id AND native_author_id IS video_id))",
    "AND stop_code IS NULL) OR (status='failed'",
  ].join(" ");
  rebuildCreatorTableChanges(db, "discovery_web_source_reads", [
    ["target_platform IN ('youtube','twitter','douyin','instagram','tiktok')", "target_platform IN ('youtube','twitter','douyin','instagram','tiktok','linkedin','bilibili','weibo')"],
    ["video_id NOT GLOB '*[^a-zA-Z0-9_.]*'))", `video_id NOT GLOB '*[^a-zA-Z0-9_.]*') OR ${identifiers})`],
    ["OR (status='failed'", completion],
  ]);
}
