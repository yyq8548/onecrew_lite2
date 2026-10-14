//! Deterministic HTTP transport for adapter integration tests. Admission still
//! validates the vault credential and every response through the real owner.
//! This module is absent unless the explicit `test-support` feature is enabled.

use super::{CreatorSyncRuntime, LocalVaultCreatorSyncStore};
use crate::http::{HttpFailure, HttpRequest, HttpResponse, HttpTransport};
use async_trait::async_trait;
use onecrew_local_vault::LocalVault;
use reqwest::StatusCode;
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub struct TestHttpResponse {
    pub method: &'static str,
    pub path: String,
    pub status: u16,
    pub body: Value,
}

#[derive(Clone)]
pub struct TestHttpProbe(Arc<Mutex<VecDeque<TestHttpResponse>>>);

impl TestHttpProbe {
    pub fn remaining_responses(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

struct TestTransport(TestHttpProbe);

#[async_trait]
impl HttpTransport for TestTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpFailure> {
        let mut responses = self.0.0.lock().map_err(|_| HttpFailure::Transport)?;
        let expected = responses.front().ok_or(HttpFailure::Transport)?;
        // Match request shape without recording or returning its bearer.
        if request.method.as_str() != expected.method
            || request.endpoint.path() != expected.path
            || request.endpoint.host_str() != Some("api.onecrewai.io")
            || request.endpoint.scheme() != "https"
            || request.authorization.is_none()
            || !request.body.is_empty()
        {
            return Err(HttpFailure::Transport);
        }
        let response = responses.pop_front().ok_or(HttpFailure::Transport)?;
        Ok(HttpResponse {
            status: StatusCode::from_u16(response.status).map_err(|_| HttpFailure::Transport)?,
            body: serde_json::to_vec(&response.body).map_err(|_| HttpFailure::Transport)?,
        })
    }
}

/// Replace only network delivery. This does not set or override admission state.
pub fn with_test_http_responses(
    vault: LocalVault,
    responses: Vec<TestHttpResponse>,
) -> (CreatorSyncRuntime, TestHttpProbe) {
    let probe = TestHttpProbe(Arc::new(Mutex::new(responses.into())));
    let runtime = CreatorSyncRuntime::from_parts(
        true,
        Some(Arc::new(TestTransport(probe.clone()))),
        Arc::new(LocalVaultCreatorSyncStore::new(vault)),
    );
    (runtime, probe)
}
