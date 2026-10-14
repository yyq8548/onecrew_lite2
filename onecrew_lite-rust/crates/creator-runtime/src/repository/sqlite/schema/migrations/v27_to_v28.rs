use rusqlite::Connection;

// Frozen v27 -> v28 edge (Desktop app 0.5.10). Creator folders are additive:
// every existing Creator remains intact and starts uncategorized.

const CREATOR_GROUPS_V28_SQL: &str = "
CREATE TABLE creator_groups (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL COLLATE NOCASE UNIQUE
    CHECK(length(trim(name)) BETWEEN 1 AND 64),
  revision INTEGER NOT NULL CHECK(revision >= 1),
  position INTEGER NOT NULL CHECK(position >= 0),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
ALTER TABLE creators ADD COLUMN group_id TEXT
  REFERENCES creator_groups(id) ON DELETE SET NULL;
CREATE INDEX creators_by_group ON creators(group_id);";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(CREATOR_GROUPS_V28_SQL)
        .map_err(|error| error.to_string())
}
