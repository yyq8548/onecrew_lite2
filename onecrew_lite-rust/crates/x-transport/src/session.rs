use std::collections::BTreeMap;

use crate::TransportError;

#[derive(Debug, Clone)]
pub struct XSession {
    pub auth_token: String,
    pub csrf_token: String,
    pub cookies: BTreeMap<String, String>,
    pub language: String,
}

impl XSession {
    pub fn new(
        auth_token: impl Into<String>,
        csrf_token: impl Into<String>,
        cookies: BTreeMap<String, String>,
    ) -> Result<Self, TransportError> {
        let session = Self {
            auth_token: auth_token.into(),
            csrf_token: csrf_token.into(),
            cookies,
            language: "en".into(),
        };
        if session.auth_token.is_empty() || session.csrf_token.is_empty() {
            return Err(TransportError::MissingCredential);
        }
        Ok(session)
    }

    pub(crate) fn cookie_header(&self) -> String {
        let mut cookies = self.cookies.clone();
        cookies.insert("auth_token".into(), self.auth_token.clone());
        cookies.insert("ct0".into(), self.csrf_token.clone());
        cookies
            .into_iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_header_has_required_credentials_once() {
        let mut cookies = BTreeMap::new();
        cookies.insert("auth_token".into(), "stale".into());
        let session = XSession::new("auth", "csrf", cookies).unwrap();
        assert_eq!(session.cookie_header(), "auth_token=auth; ct0=csrf");
    }
}
