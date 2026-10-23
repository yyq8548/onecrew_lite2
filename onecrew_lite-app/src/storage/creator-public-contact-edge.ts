import type { DatabaseSync } from "node:sqlite";
import { rebuildCreatorTableChanges } from "./creator-schema.ts";

const oldKinds = "'email','telegram','hidden_email','social_profile','whatsapp','wechat','wechat_official'";
const newKinds = `${oldKinds},'website','messenger','phone'`;
const profileCompletion = [
  "OR (status='completed' AND target_platform IN ('instagram','tiktok') AND native_author_id IS NOT NULL",
  "AND native_material_json IS NOT NULL AND json_valid(native_material_json) AND json_type(native_material_json)='object' AND length(CAST(native_material_json AS BLOB))<=65536",
  "AND ((target_platform='tiktok' AND length(native_author_id) BETWEEN 1 AND 25 AND substr(native_author_id,1,1)!='0' AND native_author_id NOT GLOB '*[^0-9]*')",
  "OR (target_platform='instagram' AND native_author_id IS ('https://www.instagram.com/' || video_id || '/') AND json_extract(native_material_json,'$.identity.identity_kind') IS 'profile_url' AND json_extract(native_material_json,'$.identity.execution_mode') IS 'computer_use'))",
  "AND json_extract(native_material_json,'$.identity.id') IS native_author_id AND json_extract(native_material_json,'$.identity.username') IS video_id",
  "AND json_extract(native_material_json,'$.profile.id') IS native_author_id AND json_extract(native_material_json,'$.profile.username') IS video_id",
  "AND json_extract(native_material_json,'$.profile.profile_url') IS (CASE target_platform WHEN 'instagram' THEN 'https://www.instagram.com/' || video_id || '/' ELSE 'https://www.tiktok.com/@' || video_id END)",
  "AND stop_code IS NULL) OR (status='failed'",
].join(" ");

export function migrateCreatorPublicContactEdge(db: DatabaseSync, reverse = false): void {
  const contacts: ReadonlyArray<readonly [string, string]> = [[oldKinds, newKinds]];
  const web: ReadonlyArray<readonly [string, string]> = [
    ["origin_platform IN ('google','yahoo')", "origin_platform IN ('google','yahoo','instagram')"],
    ["target_platform IN ('youtube','twitter','douyin')", "target_platform IN ('youtube','twitter','douyin','instagram','tiktok')"],
    ["video_id NOT GLOB '*[^0-9]*'))", "video_id NOT GLOB '*[^0-9]*') OR (target_platform IN ('instagram','tiktok') AND length(video_id) BETWEEN 1 AND 30 AND video_id NOT GLOB '*[^a-zA-Z0-9_.]*'))"],
    ["OR (status='failed'", profileCompletion],
  ];
  for (const [table, changes] of [["discovery_contact_observations", contacts], ["discovery_web_source_reads", web]] as const)
    rebuildCreatorTableChanges(db, table, reverse ? changes.map(([from, to]) => [to, from] as const) : changes);
}
