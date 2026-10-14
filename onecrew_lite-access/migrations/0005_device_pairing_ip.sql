ALTER TABLE "access_device"
ADD COLUMN "pairingIp" text;

ALTER TABLE "access_device"
ADD COLUMN "pairingCountry" text;

ALTER TABLE "access_device"
ADD COLUMN "pairingRegion" text;

ALTER TABLE "access_device"
ADD COLUMN "pairingCity" text;

ALTER TABLE "access_device"
ADD COLUMN "pairingColo" text;

CREATE INDEX "access_device_user_pairing_ip_idx"
ON "access_device" ("user_id", "created_at" DESC, "id" DESC)
WHERE "pairingIp" IS NOT NULL;
