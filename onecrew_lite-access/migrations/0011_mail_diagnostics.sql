-- Historical outcomes remain unknown: never infer past HTTP failures.
ALTER TABLE distribution_mail ADD COLUMN last_http_status INTEGER
  CHECK (last_http_status IS NULL OR last_http_status BETWEEN 100 AND 599);
ALTER TABLE distribution_mail ADD COLUMN last_error_code TEXT
  CHECK (last_error_code IS NULL OR last_error_code IN (
    'http_client_error', 'rate_limited', 'http_server_error',
    'unexpected_http_status', 'timeout', 'transport_error',
    'invalid_provider_response', 'persistence_after_ack'
  ));

-- One-shot authorization for an administrator-requested exact-event retry.
-- The sender consumes this marker atomically when claiming the event.
ALTER TABLE distribution_mail ADD COLUMN manual_request_id TEXT;
