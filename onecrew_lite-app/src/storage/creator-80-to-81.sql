CREATE TABLE discovery_facebook_original_attempts (
 search_hit_id TEXT NOT NULL REFERENCES discovery_facebook_content_targets(search_hit_id),
 discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id),
 profile_ref TEXT NOT NULL, author_id TEXT NOT NULL, source_url TEXT NOT NULL,
 expected_revision INTEGER NOT NULL CHECK(expected_revision>0),
 call_number INTEGER NOT NULL CHECK(call_number>0),
 status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
 native_content_json TEXT CHECK(native_content_json IS NULL OR (json_valid(native_content_json) AND json_type(native_content_json)='object' AND length(CAST(native_content_json AS BLOB))<=16000)),
 stop_code TEXT, updated_at INTEGER NOT NULL,
 PRIMARY KEY(search_hit_id,source_url),
 FOREIGN KEY(discovery_campaign_id,expected_revision) REFERENCES discovery_campaign_revisions(discovery_campaign_id,revision),
 CHECK((status='reading' AND native_content_json IS NULL AND stop_code IS NULL)
 OR(status='completed' AND native_content_json IS NOT NULL AND stop_code IS NULL)
 OR(status='failed' AND native_content_json IS NULL AND stop_code IS NOT NULL)));
CREATE TRIGGER discovery_facebook_original_source BEFORE INSERT ON discovery_facebook_original_attempts
 WHEN NOT EXISTS(SELECT 1 FROM discovery_facebook_content_targets t JOIN discovery_profile_reads p ON p.search_hit_id=t.search_hit_id
 JOIN discovery_content_reads c ON c.search_hit_id=t.search_hit_id
 WHERE t.search_hit_id=NEW.search_hit_id AND t.discovery_campaign_id=NEW.discovery_campaign_id
 AND t.profile_ref=NEW.profile_ref AND t.author_id=NEW.author_id AND t.expected_revision=NEW.expected_revision
 AND t.status='completed' AND t.stop_code IS NULL AND t.call_number<NEW.call_number
 AND p.status='completed' AND json_extract(p.native_profile_json,'$.adapter_version') IS '2.0.0'
 AND c.platform='facebook' AND c.discovery_campaign_id=NEW.discovery_campaign_id
 AND c.profile_ref=NEW.profile_ref AND c.author_id=NEW.author_id AND c.expected_revision=NEW.expected_revision
 AND c.status='reading' AND c.source_url=NEW.source_url AND c.call_number=NEW.call_number
 AND EXISTS(SELECT 1 FROM json_each(t.source_urls_json) l WHERE l.value=NEW.source_url))
 BEGIN SELECT RAISE(ABORT,'Facebook original source mismatch'); END;
CREATE TRIGGER discovery_facebook_original_immutable BEFORE UPDATE ON discovery_facebook_original_attempts
 WHEN OLD.status!='reading' OR NEW.search_hit_id!=OLD.search_hit_id OR NEW.discovery_campaign_id!=OLD.discovery_campaign_id
 OR NEW.profile_ref!=OLD.profile_ref OR NEW.author_id!=OLD.author_id OR NEW.source_url!=OLD.source_url
 OR NEW.expected_revision!=OLD.expected_revision OR NEW.call_number!=OLD.call_number
 BEGIN SELECT RAISE(ABORT,'Facebook original attempt is immutable'); END;
