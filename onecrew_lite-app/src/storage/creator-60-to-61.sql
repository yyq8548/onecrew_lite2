ALTER TABLE discovery_language_samples ADD COLUMN sample_format_version INTEGER NOT NULL DEFAULT 1
  CHECK(sample_format_version IN (1,2));
