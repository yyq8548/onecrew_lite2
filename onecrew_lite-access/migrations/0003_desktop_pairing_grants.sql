-- Desktop-initiated browser pairing with PKCE. Raw codes, code verifiers and
-- approval tokens are never persisted. Legacy 0002 activation objects remain
-- inert so this expand migration can be applied before the coordinated A/B cutover.
create table "desktop_pairing_grant" (
  "id" text not null primary key,
  "device_id" text not null unique references "access_device" ("id") on delete cascade,
  "request_hmac" text not null unique,
  "code_hmac" text not null unique,
  "code_challenge" text not null,
  "platform" text not null,
  "app_version" text not null,
  "created_at" integer not null,
  "expires_at" integer not null,
  "consumed_at" integer,
  "attempt_count" integer not null default 0,
  check (
    length("id") = 29 and
    "id" glob 'pair_*' and
    substr("id", 6) not glob '*[^A-Za-z0-9_-]*'
  ),
  check (
    length("request_hmac") = 43 and
    "request_hmac" not glob '*[^A-Za-z0-9_-]*'
  ),
  check (
    length("code_hmac") = 43 and
    "code_hmac" not glob '*[^A-Za-z0-9_-]*'
  ),
  check (
    length("code_challenge") = 43 and
    "code_challenge" not glob '*[^A-Za-z0-9_-]*'
  ),
  check ("platform" in ('macos', 'windows', 'linux')),
  check (
    length("app_version") between 1 and 32 and
    "app_version" not glob '*[^0-9A-Za-z.+-]*'
  ),
  check ("expires_at" - "created_at" = 90000),
  check (
    "consumed_at" is null or
    ("consumed_at" >= "created_at" and "consumed_at" < "expires_at")
  ),
  check ("attempt_count" between 0 and 9)
);

create table "desktop_pairing_rate_limit" (
  "bucket" text not null primary key,
  "request_count" integer not null,
  "reset_at" integer not null,
  check (length("bucket") between 1 and 160),
  check ("request_count" between 1 and 10000)
);

create index "desktop_pairing_grant_expiry_idx"
  on "desktop_pairing_grant" ("expires_at");
create index "desktop_pairing_rate_limit_reset_idx"
  on "desktop_pairing_rate_limit" ("reset_at");
