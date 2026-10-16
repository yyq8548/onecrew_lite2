export type NativeReadFailureCode = "provider_endpoint_not_found" | "provider_response_http_error" | "provider_response_redirected" | "provider_response_invalid_json" | "provider_request_failed" | "provider_source_unavailable" | "provider_response_shape_invalid";
export type YoutubeEvidenceFailureCode = "youtube_video_unavailable" | "youtube_watch_layout_unsupported" | "youtube_author_identity_invalid" | "youtube_description_unavailable";
export type OpenCliErrorCode = NativeReadFailureCode | YoutubeEvidenceFailureCode | "opencli_unavailable" | "opencli_timeout" | "opencli_cancelled" | "opencli_output_too_large" | "opencli_command_failed" | "opencli_risk_control" | "opencli_auth_required" | "opencli_invalid_output" | "opencli_version_changed" | "opencli_daemon_unavailable" | "opencli_extension_unavailable" | "opencli_profile_unavailable" | "opencli_read_command_unavailable" | "opencli_invalid_input";

export class OpenCliError extends Error {
  readonly code: OpenCliErrorCode;
  readonly exitCode: number | null;
  constructor(code: OpenCliErrorCode, exitCode: number | null = null) {
    super(code);
    this.code = code;
    this.exitCode = exitCode;
  }
}
