use onecrew_ig_transport::IgError;

/// A known pre-effect refusal or an unverified remote outcome. Instagram's
/// free-form API errors also carry malformed/missing receipt failures and
/// cannot safely be treated as proof of refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgWriteFailure {
    Refused,
    Unknown,
}

impl From<&IgError> for IgWriteFailure {
    fn from(error: &IgError) -> Self {
        match error {
            IgError::Session(_) | IgError::InvalidHeader => Self::Refused,
            IgError::Network(_)
            | IgError::Api(_)
            | IgError::Challenge(_)
            | IgError::Response { .. } => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_receipts_and_provider_errors_do_not_prove_refusal() {
        for error in [
            IgError::Api("direct send response missing item_id SECRET".into()),
            IgError::Network("SECRET".into()),
            IgError::Challenge("SECRET".into()),
        ] {
            assert_eq!(IgWriteFailure::from(&error), IgWriteFailure::Unknown);
            assert!(!format!("{:?}", IgWriteFailure::from(&error)).contains("SECRET"));
        }
        assert_eq!(
            IgWriteFailure::from(&IgError::InvalidHeader),
            IgWriteFailure::Refused
        );
    }

    #[test]
    fn a_native_account_api_response_does_not_prove_a_send_refused() {
        for error in [
            IgError::Response {
                status: 429,
                code: Some("NEW_NATIVE_CODE".into()),
            },
            IgError::Response {
                status: 409,
                code: None,
            },
        ] {
            assert_eq!(IgWriteFailure::from(&error), IgWriteFailure::Unknown);
        }
    }
}
