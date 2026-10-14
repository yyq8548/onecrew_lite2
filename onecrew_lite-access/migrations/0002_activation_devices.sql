-- Access-owned devices and one-time activation verifiers.
-- Raw activation keys are never persisted.
create table "access_device" (
  "id" text not null primary key,
  "user_id" text not null references "user" ("id") on delete cascade,
  "label" text not null,
  "created_at" integer not null,
  "activated_at" integer,
  "last_seen_at" integer,
  "revoked_at" integer,
  check (
    length("id") = 28 and
    "id" glob 'dev_*' and
    substr("id", 5) not glob '*[^A-Za-z0-9_-]*'
  ),
  check (length("label") between 1 and 80 and trim("label") = "label"),
  check ("activated_at" is null or "activated_at" >= "created_at"),
  check ("last_seen_at" is null or "last_seen_at" >= "created_at"),
  check ("revoked_at" is null or "revoked_at" >= "created_at")
);

create table "device_activation" (
  "id" text not null primary key,
  "device_id" text not null unique references "access_device" ("id") on delete cascade,
  "key_verifier" text not null unique,
  "created_at" integer not null,
  "expires_at" integer not null,
  "consumed_at" integer,
  "revoked_at" integer,
  check (
    length("id") = 28 and
    "id" glob 'act_*' and
    substr("id", 5) not glob '*[^A-Za-z0-9_-]*'
  ),
  check (
    length("key_verifier") = 43 and
    "key_verifier" not glob '*[^A-Za-z0-9_-]*'
  ),
  check ("expires_at" > "created_at"),
  check (
    "consumed_at" is null or
    ("consumed_at" >= "created_at" and "consumed_at" <= "expires_at")
  ),
  check ("revoked_at" is null or "revoked_at" >= "created_at")
);

create table "activation_rate_limit" (
  "bucket" text not null primary key,
  "request_count" integer not null,
  "reset_at" integer not null,
  check (length("bucket") between 1 and 128),
  check ("request_count" between 1 and 10000)
);

create index "access_device_user_created_idx"
  on "access_device" ("user_id", "created_at" desc);
create index "device_activation_expiry_idx"
  on "device_activation" ("expires_at");
create index "activation_rate_limit_reset_idx"
  on "activation_rate_limit" ("reset_at");

create trigger "device_activation_consumed_update_device"
after update of "consumed_at" on "device_activation"
when old."consumed_at" is null and new."consumed_at" is not null
BEGIN
  update "access_device"
  set
    "activated_at" = coalesce("activated_at", new."consumed_at"),
    "last_seen_at" = new."consumed_at"
  where "id" = new."device_id" and "revoked_at" is null;
END;
