export const NATIVE_READ_FAILURE_CODES = ["provider_endpoint_not_found", "provider_response_http_error", "provider_response_redirected", "provider_response_invalid_json", "provider_request_failed", "provider_source_unavailable", "provider_response_shape_invalid"] as const;
export type NativeReadFailureCode = typeof NATIVE_READ_FAILURE_CODES[number];
export type YoutubeEvidenceFailureCode = "youtube_video_unavailable" | "youtube_watch_layout_unsupported" | "youtube_author_identity_invalid" | "youtube_description_unavailable";
export const PLATFORM_READ_FAILURE_CODES = ["linkedin_profile_unavailable", "linkedin_profile_timeout", "linkedin_content_unavailable", "linkedin_contact_profile_route", "linkedin_contact_owner_conflict", "linkedin_contact_dialog_owner_conflict", "linkedin_contact_before_click", "linkedin_contact_projection_invalid", "linkedin_contact_unavailable", "linkedin_contact_timeout", "facebook_profile_timeout", "facebook_profile_layout_invalid", "facebook_contact_layout_invalid"] as const;
export type OpenCliErrorCode = NativeReadFailureCode | YoutubeEvidenceFailureCode | typeof PLATFORM_READ_FAILURE_CODES[number] | "opencli_unavailable" | "opencli_timeout" | "opencli_cancelled" | "opencli_cancel_pending" | "opencli_output_too_large" | "opencli_command_failed" | "opencli_risk_control" | "opencli_auth_required" | "opencli_invalid_output" | "opencli_version_changed" | "opencli_daemon_unavailable" | "opencli_extension_unavailable" | "opencli_profile_unavailable" | "opencli_read_command_unavailable" | "opencli_invalid_input";

export class OpenCliError extends Error {
  readonly code: OpenCliErrorCode;
  readonly exitCode: number | null;
  constructor(code: OpenCliErrorCode, exitCode: number | null = null) {
    super(code);
    this.code = code;
    this.exitCode = exitCode;
  }
}
