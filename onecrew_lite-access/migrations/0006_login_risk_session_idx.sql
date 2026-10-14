-- Keep the new-IP login heuristic bounded to the user's latest session that
-- actually has an IP. The partial index avoids scanning sessions with no IP.
CREATE INDEX "access_session_user_latest_ip_idx"
ON "session" ("userId", "updatedAt" DESC, "createdAt" DESC, "id" DESC)
WHERE "ipAddress" IS NOT NULL;
