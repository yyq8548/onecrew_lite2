//! Browser-only capabilities. Opening the local origin mints an HttpOnly
//! same-site session; a separate CSRF value protects every authenticated POST.
//!
//! By product decision this origin admits any local caller: a launch ticket
//! only survived the redirect that consumed it, so the address left in the
//! browser bar never opened a second browser. Reaching the loopback port is
//! therefore the whole admission, and every process on this machine can reach
//! it. What the session still carries is separation between browser tabs and
//! cross-site defence: Host, Origin and CSRF continue to reject a page served
//! from anywhere else.
use axum::http::HeaderMap;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

const SESSION_TTL: Duration = Duration::from_secs(12 * 60 * 60);
const MAX_SESSIONS: usize = 64;

struct Session {
    csrf: Zeroizing<String>,
    expires: Instant,
    // Independent authority scope, never a cookie/CSRF derivative.
    result_origin: onecrew_local_runtime::ResultOrigin,
}
#[derive(Default)]
struct State {
    sessions: HashMap<[u8; 32], Session>,
}

#[derive(Default)]
pub(crate) struct BrowserSessions {
    origin: OnceLock<String>,
    state: Mutex<State>,
}

fn digest(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}

impl BrowserSessions {
    pub(crate) fn set_port(&self, port: u16) -> Result<(), String> {
        self.origin
            .set(format!("http://127.0.0.1:{port}"))
            .map_err(|_| "web_origin_already_set".into())
    }

    pub(crate) fn origin(&self) -> Option<&str> {
        self.origin.get().map(String::as_str)
    }

    fn cookie_name(&self) -> Result<String, ()> {
        let port = self
            .origin()
            .and_then(|origin| origin.rsplit(':').next())
            .ok_or(())?;
        Ok(format!("onecrew_lite_session_{port}"))
    }

    /// The page URL. It carries no admission of its own, so the user can copy
    /// it out of the address bar into any browser.
    pub(crate) fn launch_url(&self) -> Result<String, String> {
        Ok(self.origin().ok_or("web_not_ready")?.to_owned())
    }

    /// Mints a session for a caller that reached the loopback origin, and
    /// returns its Set-Cookie value with the CSRF value that pairs with it.
    pub(super) fn open_session(&self) -> Result<(String, String), ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        state
            .sessions
            .retain(|_, session| session.expires > Instant::now());
        if state.sessions.len() >= MAX_SESSIONS {
            return Err(());
        }
        let token = Zeroizing::new(crate::ipc::random_secret());
        let csrf = Zeroizing::new(crate::ipc::random_secret());
        state.sessions.insert(
            digest(&token),
            Session {
                csrf: csrf.clone(),
                expires: Instant::now() + SESSION_TTL,
                result_origin: onecrew_local_runtime::ResultOrigin::new(format!(
                    "web:{}",
                    uuid::Uuid::new_v4()
                ))
                .map_err(|_| ())?,
            },
        );
        Ok((
            format!(
                "{}={}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
                self.cookie_name()?,
                token.as_str(),
                SESSION_TTL.as_secs()
            ),
            csrf.to_string(),
        ))
    }

    pub(super) fn csrf(&self, headers: &HeaderMap) -> Result<String, ()> {
        let token = cookie(headers, &self.cookie_name()?).ok_or(())?;
        let mut state = self.state.lock().map_err(|_| ())?;
        let session = state.sessions.get_mut(&digest(token)).ok_or(())?;
        if session.expires <= Instant::now() {
            return Err(());
        }
        session.expires = Instant::now() + SESSION_TTL;
        Ok(session.csrf.to_string())
    }

    pub(super) fn authorize(&self, headers: &HeaderMap, write: bool) -> Result<(), ()> {
        let csrf = Zeroizing::new(self.csrf(headers)?);
        if write {
            let supplied = headers
                .get("x-onecrew-csrf")
                .and_then(|h| h.to_str().ok())
                .ok_or(())?;
            if !bool::from(supplied.as_bytes().ct_eq(csrf.as_bytes())) {
                return Err(());
            }
        }
        Ok(())
    }

    pub(super) fn validate_origin(&self, headers: &HeaderMap, write: bool) -> Result<(), ()> {
        let origin = self.origin().ok_or(())?;
        let expected_host = origin.strip_prefix("http://").ok_or(())?;
        if headers.get("host").and_then(|h| h.to_str().ok()) != Some(expected_host) {
            return Err(());
        }
        match headers.get("origin").and_then(|h| h.to_str().ok()) {
            Some(supplied) if supplied != origin => return Err(()),
            None if write => return Err(()),
            _ => (),
        }
        if headers.get("sec-fetch-site").and_then(|h| h.to_str().ok()) == Some("cross-site") {
            return Err(());
        }
        Ok(())
    }

    /// Recover trusted result authority only after authenticating this POST.
    pub(super) fn result_origin(
        &self,
        headers: &HeaderMap,
    ) -> Result<onecrew_local_runtime::ResultOrigin, ()> {
        self.authorize(headers, true)?;
        let token = cookie(headers, &self.cookie_name()?).ok_or(())?;
        let state = self.state.lock().map_err(|_| ())?;
        let session = state.sessions.get(&digest(token)).ok_or(())?;
        if session.expires <= Instant::now() {
            return Err(());
        }
        Ok(session.result_origin.clone())
    }
}

fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let header = headers.get("cookie")?.to_str().ok()?;
    let prefix = format!("{name}=");
    let mut matching = header
        .split(';')
        .filter_map(|part| part.trim().strip_prefix(prefix.as_str()));
    let first = matching.next()?;
    if matching.next().is_some() {
        return None;
    }
    Some(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn result_authority_is_session_bound_and_requires_csrf() {
        let auth = BrowserSessions::default();
        auth.set_port(12345).unwrap();
        let authorize = || {
            let (cookie, _) = auth.open_session().unwrap();
            let mut headers = HeaderMap::new();
            headers.insert(
                "cookie",
                HeaderValue::from_str(cookie.split(';').next().unwrap()).unwrap(),
            );
            assert!(auth.result_origin(&headers).is_err());
            let csrf = auth.csrf(&headers).unwrap();
            headers.insert("x-onecrew-csrf", HeaderValue::from_str(&csrf).unwrap());
            (headers, cookie, csrf)
        };
        let (first, cookie, csrf) = authorize();
        let (second, _, _) = authorize();
        let origin = auth.result_origin(&first).unwrap();
        assert_eq!(origin, auth.result_origin(&first).unwrap());
        assert_ne!(origin, auth.result_origin(&second).unwrap());
        assert!(!cookie.contains(origin.as_str()));
        assert!(!csrf.contains(origin.as_str()));
        for session in auth.state.lock().unwrap().sessions.values_mut() {
            session.expires = Instant::now() - Duration::from_secs(1);
        }
        assert!(auth.result_origin(&first).is_err());
    }

    #[test]
    fn reaching_the_origin_opens_a_session_and_csrf_origin_host_guard_writes() {
        let auth = BrowserSessions::default();
        auth.set_port(12345).unwrap();
        // The address carries no admission, so any browser on this machine
        // opens the page and receives a session of its own.
        assert_eq!(auth.launch_url().unwrap(), "http://127.0.0.1:12345");
        let (set_cookie, _) = auth.open_session().unwrap();
        let (second_cookie, _) = auth.open_session().unwrap();
        assert_ne!(set_cookie, second_cookie);
        assert!(set_cookie.contains("HttpOnly; SameSite=Strict"));
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("127.0.0.1:12345"));
        headers.insert("origin", HeaderValue::from_static("http://127.0.0.1:12345"));
        headers.insert(
            "cookie",
            HeaderValue::from_str(set_cookie.split(';').next().unwrap()).unwrap(),
        );
        assert!(auth.validate_origin(&headers, true).is_ok());
        assert!(auth.authorize(&headers, false).is_ok());
        assert!(auth.authorize(&headers, true).is_err());
        headers.insert(
            "x-onecrew-csrf",
            HeaderValue::from_str(&auth.csrf(&headers).unwrap()).unwrap(),
        );
        assert!(auth.authorize(&headers, true).is_ok());
        headers.insert(
            "origin",
            HeaderValue::from_static("https://attacker.example"),
        );
        assert!(auth.validate_origin(&headers, true).is_err());
        headers.remove("origin");
        assert!(auth.validate_origin(&headers, true).is_err());
        headers.insert("host", HeaderValue::from_static("rebind.example:12345"));
        assert!(auth.validate_origin(&headers, false).is_err());
    }

    #[test]
    fn an_expired_session_and_a_missing_cookie_both_fail_closed() {
        let auth = BrowserSessions::default();
        auth.set_port(12345).unwrap();
        let (cookie, _) = auth.open_session().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            "cookie",
            HeaderValue::from_str(cookie.split(';').next().unwrap()).unwrap(),
        );
        assert!(auth.authorize(&headers, false).is_ok());
        for session in auth.state.lock().unwrap().sessions.values_mut() {
            session.expires = Instant::now() - Duration::from_secs(1);
        }
        assert!(auth.authorize(&headers, false).is_err());
        assert!(auth.authorize(&HeaderMap::new(), false).is_err());
    }

    #[test]
    fn different_local_services_keep_distinct_browser_sessions() {
        let first = BrowserSessions::default();
        first.set_port(23456).unwrap();
        let second = BrowserSessions::default();
        second.set_port(23457).unwrap();
        let issue = |owner: &BrowserSessions| {
            owner
                .open_session()
                .unwrap()
                .0
                .split(';')
                .next()
                .unwrap()
                .to_owned()
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            "cookie",
            HeaderValue::from_str(&format!("{}; {}", issue(&first), issue(&second))).unwrap(),
        );
        assert!(first.authorize(&headers, false).is_ok());
        assert!(second.authorize(&headers, false).is_ok());
    }
}
