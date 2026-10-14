-- Never assign older unbound receipts to whichever browser logs in next.
ALTER TABLE analytics_success_event ADD COLUMN attribution_context TEXT
  CHECK (attribution_context IS NULL OR (
    length(attribution_context)=64 AND attribution_context NOT GLOB '*[^0-9a-f]*'
  ));
CREATE INDEX analytics_success_event_context_pending
  ON analytics_success_event(user_id, attribution_context, created_at)
  WHERE claimed_at IS NULL;
