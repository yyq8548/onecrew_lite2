-- Access owns applications, grants and private distribution metadata. No legacy grants.
CREATE TABLE distribution_application (
 id TEXT PRIMARY KEY, user_id TEXT NOT NULL UNIQUE REFERENCES "user"(id) ON DELETE CASCADE,
 display_name TEXT NOT NULL, purpose TEXT NOT NULL, company TEXT,
 locale TEXT NOT NULL DEFAULT 'zh-CN' CHECK(locale IN ('zh-CN','en-US')),
 review_reason TEXT, decision_revision INTEGER,
 status TEXT NOT NULL CHECK(status IN ('pending','approved','rejected')),
 revision INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
 last_request_id TEXT NOT NULL
);
CREATE TABLE distribution_entitlement (
 user_id TEXT PRIMARY KEY REFERENCES "user"(id) ON DELETE CASCADE,
 status TEXT NOT NULL CHECK(status IN ('active','revoked')), revision INTEGER NOT NULL,
 updated_at INTEGER NOT NULL
);
CREATE TABLE distribution_operation (
 request_id TEXT PRIMARY KEY, fingerprint TEXT NOT NULL, application_id TEXT NOT NULL,
 created_at INTEGER NOT NULL
);
CREATE TABLE distribution_audit (
 id TEXT PRIMARY KEY, application_id TEXT NOT NULL REFERENCES distribution_application(id),
 action TEXT NOT NULL, actor_id TEXT NOT NULL, reason TEXT, revision INTEGER NOT NULL,
 created_at INTEGER NOT NULL
);
CREATE TABLE distribution_link (
 token_hash TEXT PRIMARY KEY, user_id TEXT NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
 encrypted_token TEXT NOT NULL, request_id TEXT NOT NULL UNIQUE, created_at INTEGER NOT NULL,
 expires_at INTEGER NOT NULL, entitlement_revision INTEGER NOT NULL
);
CREATE INDEX distribution_link_user ON distribution_link(user_id, created_at);
CREATE TABLE distribution_mail (
 id TEXT PRIMARY KEY, application_id TEXT NOT NULL REFERENCES distribution_application(id),
 kind TEXT NOT NULL CHECK(kind IN ('approved','rejected')), decision_revision INTEGER NOT NULL,
 recipient TEXT NOT NULL, payload_json TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('pending','accepted','failed','unknown')),
 attempts INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL, last_attempt_at INTEGER,
 accepted_at INTEGER, provider_id TEXT, lease_until INTEGER, blocked_reason TEXT
);
CREATE INDEX distribution_mail_queue ON distribution_mail(status, created_at);
CREATE TABLE distribution_release (
 version TEXT PRIMARY KEY, notes TEXT NOT NULL, pub_date TEXT NOT NULL, published_at INTEGER NOT NULL,
 request_id TEXT NOT NULL UNIQUE, fingerprint TEXT NOT NULL
);
CREATE TABLE distribution_asset (
 id TEXT PRIMARY KEY, version TEXT NOT NULL REFERENCES distribution_release(version),
 object_key TEXT NOT NULL UNIQUE, filename TEXT NOT NULL, platform TEXT NOT NULL,
 arch TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('installer','updater')),
 size INTEGER NOT NULL CHECK(size > 0), sha256 TEXT NOT NULL, signature TEXT, etag TEXT NOT NULL,
 UNIQUE(version,platform,arch,kind)
);
CREATE TABLE distribution_settings (
 id INTEGER PRIMARY KEY CHECK(id=1), applications_open INTEGER NOT NULL CHECK(applications_open IN (0,1)),
 revision INTEGER NOT NULL, updated_at INTEGER NOT NULL, last_request_id TEXT
);
INSERT INTO distribution_settings(id,applications_open,revision,updated_at) VALUES(1,0,1,0);
CREATE TABLE distribution_settings_audit (
 request_id TEXT PRIMARY KEY, fingerprint TEXT NOT NULL, actor_id TEXT NOT NULL,
 applications_open INTEGER NOT NULL, revision INTEGER NOT NULL, created_at INTEGER NOT NULL
);
