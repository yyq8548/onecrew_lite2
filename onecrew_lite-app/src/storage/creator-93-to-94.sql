UPDATE discovery_campaigns SET max_calls = NULL WHERE max_calls IS NOT NULL;
UPDATE discovery_platform_runs
SET status = 'completed', stop_code = NULL, active_query = NULL
WHERE status = 'budget_exhausted';
