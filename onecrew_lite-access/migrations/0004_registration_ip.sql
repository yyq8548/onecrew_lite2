ALTER TABLE "user"
ADD COLUMN "registrationIp" text;

UPDATE "user"
SET "registrationIp" = (
  SELECT "session"."ipAddress"
  FROM "session"
  WHERE "session"."userId" = "user"."id"
  ORDER BY "session"."createdAt" ASC, "session"."id" ASC
  LIMIT 1
)
WHERE "registrationIp" IS NULL;
