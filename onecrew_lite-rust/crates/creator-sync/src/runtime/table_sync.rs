//! Device-authenticated dynamic-table protocol. Credentials never leave this owner.
use super::{CreatorSyncRuntime, credential_store::StoredCredentialState};
use reqwest::{Method, redirect::Policy};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum TableSyncRequest {
    Capabilities,
    Consent,
    SetConsent {
        input: Value,
        etag: Option<String>,
    },
    Begin {
        input: Value,
    },
    Status {
        version_id: Uuid,
    },
    Part {
        version_id: Uuid,
        attempt_id: Uuid,
        number: u16,
        md5_base64: String,
        bytes: Vec<u8>,
    },
    Complete {
        version_id: Uuid,
        input: Value,
    },
    Abort {
        version_id: Uuid,
        attempt_id: Uuid,
    },
}
#[derive(Serialize, Deserialize)]
pub struct TableSyncResponse {
    pub status: u16,
    pub etag: Option<String>,
    pub retry_after: Option<String>,
    pub body: Value,
}
impl CreatorSyncRuntime {
    /// Local credential identity for independent consent controls; no capability request.
    pub async fn table_sync_access_user_id(&self) -> Result<String, String> {
        let credentials = self.inner.credentials.clone();
        let stored = tokio::task::spawn_blocking(move || credentials.load())
            .await
            .map_err(|_| "table_sync_identity_unavailable")?
            .map_err(|_| "table_sync_identity_unavailable")?;
        match stored {
            Some(StoredCredentialState::Active(s)) => Ok(s.metadata.access_user_id),
            _ => Err("table_sync_login_required".into()),
        }
    }
    pub async fn table_sync_request(
        &self,
        request: TableSyncRequest,
    ) -> Result<TableSyncResponse, String> {
        self.table_sync_request_for_user(request, None).await
    }
    pub async fn table_sync_request_for_user(
        &self,
        request: TableSyncRequest,
        expected_user: Option<&str>,
    ) -> Result<TableSyncResponse, String> {
        let credentials = self.inner.credentials.clone();
        let stored = tokio::task::spawn_blocking(move || credentials.load())
            .await
            .map_err(|_| "table_sync_identity_unavailable")?
            .map_err(|_| "table_sync_identity_unavailable")?;
        let Some(StoredCredentialState::Active(stored)) = stored else {
            return Err("table_sync_login_required".into());
        };
        if expected_user.is_some_and(|user| stored.metadata.access_user_id != user) {
            return Err("table_sync_account_changed".into());
        }
        let (method, path, input) = match &request {
            TableSyncRequest::Capabilities => (Method::GET, "capabilities".to_owned(), None),
            TableSyncRequest::Consent => (Method::GET, "consent".to_owned(), None),
            TableSyncRequest::SetConsent { input, .. } => {
                (Method::PUT, "consent".to_owned(), Some(input))
            }
            TableSyncRequest::Begin { input } => (Method::POST, "uploads".to_owned(), Some(input)),
            TableSyncRequest::Status { version_id } => {
                (Method::GET, format!("uploads/{version_id}"), None)
            }
            TableSyncRequest::Part {
                version_id, number, ..
            } => {
                if *number == 0 || *number > 2048 {
                    return Err("table_sync_invalid_part".into());
                }
                (
                    Method::PUT,
                    format!("uploads/{version_id}/parts/{number}"),
                    None,
                )
            }
            TableSyncRequest::Complete { version_id, input } => (
                Method::POST,
                format!("uploads/{version_id}/complete"),
                Some(input),
            ),
            TableSyncRequest::Abort { version_id, .. } => {
                (Method::POST, format!("uploads/{version_id}/abort"), None)
            }
        };
        let segments: Vec<&str> = std::iter::once("dynamic-table-sync")
            .chain(path.split('/'))
            .collect();
        let endpoint = self.inner.endpoints.table_sync(&segments);
        let client = reqwest::Client::builder()
            .redirect(Policy::none())
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(45))
            .build()
            .map_err(|_| "table_sync_client_unavailable")?;
        let mut builder = client
            .request(method, endpoint)
            .header("Authorization", stored.bearer.authorization())
            .header("Cache-Control", "no-store");
        if let Some(input) = input {
            let bytes = serde_json::to_vec(input).map_err(|_| "table_sync_invalid_input")?;
            let max = if matches!(request, TableSyncRequest::Complete { .. }) {
                512 * 1024
            } else {
                16 * 1024
            };
            if bytes.len() > max {
                return Err("table_sync_control_too_large".into());
            }
            builder = builder
                .header("Content-Type", "application/json")
                .body(bytes);
        }
        match request {
            TableSyncRequest::SetConsent { etag, .. } => {
                builder = match etag {
                    Some(etag) => builder.header("If-Match", etag),
                    None => builder.header("If-None-Match", "*"),
                };
            }
            TableSyncRequest::Part {
                attempt_id,
                md5_base64,
                bytes,
                ..
            } => {
                if bytes.is_empty() || bytes.len() > 8 * 1024 * 1024 {
                    return Err("table_sync_invalid_part_size".into());
                }
                builder = builder
                    .header("Content-Type", "application/octet-stream")
                    .header("Content-Length", bytes.len())
                    .header("Content-MD5", md5_base64)
                    .header("X-OneCrew-Upload-Attempt", attempt_id.to_string())
                    .body(bytes);
            }
            TableSyncRequest::Abort { attempt_id, .. } => {
                builder = builder.json(&serde_json::json!({"attempt_id":attempt_id}));
            }
            _ => {}
        }
        let mut reply = builder
            .send()
            .await
            .map_err(|_| "table_sync_transport_unknown")?;
        let status = reply.status().as_u16();
        let etag = reply
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let retry_after = reply
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let mut bytes = Vec::new();
        while let Some(chunk) = reply
            .chunk()
            .await
            .map_err(|_| "table_sync_transport_unknown")?
        {
            if bytes.len() + chunk.len() > 1024 * 1024 {
                return Err("table_sync_response_too_large".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let body = serde_json::from_slice(&bytes).map_err(|_| "table_sync_invalid_response")?;
        Ok(TableSyncResponse {
            status,
            etag,
            retry_after,
            body,
        })
    }
}
