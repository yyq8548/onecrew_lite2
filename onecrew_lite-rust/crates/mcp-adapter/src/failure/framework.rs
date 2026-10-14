use rmcp::{ErrorData, model::ErrorCode};

/// Preserve RMCP's protocol classification while replacing every potentially
/// unsafe message/data field. Native invalid-parameter, method, and internal
/// codes remain distinct; this layer never promotes an ordinary rejection to
/// `INTERNAL_ERROR`.
pub(crate) fn sanitize_error_data(error: ErrorData) -> ErrorData {
    let code = error.code;
    let (message, safe_code, next_action) = if code == ErrorCode::INVALID_PARAMS {
        (
            "The native MCP tool router rejected this request.",
            "native_tool_request_rejected",
            "Use tools/list and the registered input schema before trying the request again.",
        )
    } else if code == ErrorCode::INTERNAL_ERROR {
        (
            "The MCP handler or result could not be completed safely.",
            "native_internal_failure",
            "Read the owning status or detail before repeating any write.",
        )
    } else {
        (
            "The MCP request could not be completed safely.",
            "native_protocol_failure",
            "Follow the protocol error code and refresh the current tool surface if needed.",
        )
    };
    ErrorData::new(
        code,
        message,
        Some(serde_json::json!({
            "code": safe_code,
            "nextAction": next_action,
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SENTINELS: &[&str] = &[
        "API_KEY_SENTINEL",
        "PASSCODE_SENTINEL",
        "OTP_SENTINEL",
        "COOKIE_SENTINEL",
        "RAW_CAUSE_SENTINEL",
    ];

    #[test]
    fn sanitizer_preserves_native_error_code_and_drops_unsafe_payload() {
        for code in [
            ErrorCode::INVALID_PARAMS,
            ErrorCode::METHOD_NOT_FOUND,
            ErrorCode::INTERNAL_ERROR,
        ] {
            let raw = ErrorData::new(
                code,
                "RAW_CAUSE_SENTINEL PASSCODE_SENTINEL",
                Some(serde_json::json!({
                    "api_key": "API_KEY_SENTINEL",
                    "otp": "OTP_SENTINEL"
                })),
            );
            let safe = sanitize_error_data(raw);
            assert_eq!(safe.code, code);
            let wire = serde_json::to_string(&safe).unwrap();
            let debug = format!("{safe:?}");
            for forbidden in SENTINELS {
                assert!(!wire.contains(forbidden), "ErrorData leaked {forbidden}");
                assert!(!debug.contains(forbidden), "debug leaked {forbidden}");
            }
        }
    }
}
