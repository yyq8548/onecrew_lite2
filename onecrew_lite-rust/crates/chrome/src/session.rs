//! Thin orchestrator for the login flow: launch the user's Chrome with a fresh
//! isolated profile through its 家宽 forwarder for the human to log in, then
//! close gracefully and extract this profile's platform cookies.

use std::path::{Path, PathBuf};
use std::process::Child;

use onecrew_local_domain::BrowserCookie;

use crate::fingerprint::Fingerprint;
use crate::{ChromeError, cookies, launch};

/// A resolved Chrome install; one per app, reused across profiles.
pub struct Chrome {
    binary: PathBuf,
}

impl Chrome {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
        }
    }

    /// Launch Chrome with a fresh isolated `--user-data-dir` routed through the
    /// profile's loopback proxy forwarder (`proxy_host:proxy_port`), opening
    /// `url` for the user to log in.
    pub fn launch_login(
        &self,
        profile_dir: &Path,
        fingerprint: &Fingerprint,
        proxy_host: &str,
        proxy_port: u16,
        url: Option<String>,
    ) -> Result<Child, ChromeError> {
        std::fs::create_dir_all(profile_dir)?;
        let opts = launch::LaunchOptions {
            binary: self.binary.clone(),
            profile_dir: profile_dir.to_path_buf(),
            proxy: Some((proxy_host.to_string(), proxy_port)),
            fingerprint: fingerprint.clone(),
            url,
        };
        Ok(launch::spawn(&opts)?)
    }

    /// Close Chrome gracefully (flushing cookies to disk) and extract + decrypt
    /// the profile's platform cookies. Blocks until the process exits.
    pub fn close_and_extract(
        &self,
        child: Child,
        profile_dir: &Path,
        host_suffixes: &[&str],
    ) -> Result<Vec<BrowserCookie>, ChromeError> {
        let mut child = child;
        launch::close_and_wait(&mut child)?;
        cookies::read_cookies(profile_dir, host_suffixes)
    }
}
