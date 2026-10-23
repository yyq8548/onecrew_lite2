import type { DatabaseSync } from "node:sqlite";

// 原 v79→80 为两个操作分别创建同一条 source identity 约束。
export function migrateCreatorFacebookProfileEdge(db: DatabaseSync): void {
  for (const operation of ["INSERT", "UPDATE"]) {
    db.exec(`CREATE TRIGGER discovery_facebook_native_profile_${operation.toLowerCase()}
      BEFORE ${operation} ON discovery_profile_reads
      WHEN NEW.platform='facebook' AND NEW.status='completed'
      AND json_extract(NEW.native_profile_json,'$.adapter_version') IS '2.0.0'
      AND NOT EXISTS(SELECT 1 FROM discovery_facebook_identity_reads i
        JOIN discovery_search_authors a ON a.search_hit_id=i.search_hit_id
        WHERE i.search_hit_id=NEW.search_hit_id AND i.discovery_campaign_id=NEW.discovery_campaign_id
        AND i.profile_ref=NEW.profile_ref AND i.expected_revision=NEW.expected_revision
        AND i.status='completed' AND i.stop_code IS NULL AND i.call_number<NEW.call_number
        AND json_extract(i.native_identity_json,'$.id') IS NEW.author_id AND a.author_id=NEW.author_id
        AND a.author_url='https://www.facebook.com/profile.php?id='||NEW.author_id
        AND json_extract(NEW.native_profile_json,'$.id') IS NEW.author_id
        AND json_extract(NEW.native_profile_json,'$.profile_url') IS a.author_url
        AND json_extract(NEW.native_profile_json,'$.evidence_source') IS 'native_profile_header_response'
        AND lower(json_extract(NEW.native_profile_json,'$.observed_profile_url')) IS lower(rtrim(i.source_url,'/')))
      BEGIN SELECT RAISE(ABORT,'Facebook native profile requires source identity'); END;`);
  }
}
