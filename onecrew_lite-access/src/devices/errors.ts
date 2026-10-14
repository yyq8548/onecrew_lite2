export type AccessServiceErrorCode =
  | "invalid_request" | "invalid_request_id" | "device_unauthorized" | "device_inactive"
  | "account_not_authorized" | "connection_not_found" | "connection_not_ready"
  | "authorization_expired" | "authorization_terminal" | "provider_rejected"
  | "provider_response_invalid" | "provider_unavailable" | "revocation_unknown"
  | "configuration_required" | "not_supported" | "identity_service_unavailable"
  | "storage_unavailable" | "request_timeout";

export class IntegrationError extends Error {
  constructor(
    readonly code: AccessServiceErrorCode,
    readonly status: 400 | 401 | 403 | 404 | 409 | 410 | 429 | 501 | 502 | 503 | 504,
    readonly retryable = false,
  ) {
    super(code);
    this.name = "IntegrationError";
  }
}
