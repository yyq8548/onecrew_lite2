CREATE TABLE discovery_search_materials (
  search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
  material_json TEXT NOT NULL CHECK(json_valid(material_json) AND json_type(material_json)='object' AND length(CAST(material_json AS BLOB))<=64000)
);
CREATE TABLE discovery_identity_observations (
  search_hit_id TEXT PRIMARY KEY REFERENCES discovery_identity_reads(search_hit_id) ON DELETE CASCADE,
  call_number INTEGER NOT NULL CHECK(call_number>0),
  native_content_json TEXT NOT NULL CHECK(json_valid(native_content_json) AND json_type(native_content_json)='object' AND length(CAST(native_content_json AS BLOB))<=16000),
  native_profile_json TEXT CHECK(native_profile_json IS NULL OR(json_valid(native_profile_json) AND json_type(native_profile_json)='object' AND length(CAST(native_profile_json AS BLOB))<=16000)),
  captured_at INTEGER NOT NULL CHECK(captured_at>0)
);
