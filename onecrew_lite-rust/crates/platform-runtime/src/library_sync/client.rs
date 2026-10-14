//! Library instance discovery and its private, account-bound HTTP protocol.

#[cfg(test)]
mod tests;
use super::{LibraryTable, MAX_ROWS};
use reqwest::{Client, Method, StatusCode, redirect::Policy};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

#[cfg(test)]
impl crate::LocalPlatform {
    // Isolated receiver only: never discover or rewrite the customer handshake.
    pub(crate) async fn assert_test_library_publication(
        &self,
        publication: &onecrew_creator_runtime::DiscoveryLibraryPublication,
    ) {
        let base = std::env::var("ONECREW_TEST_LIBRARY_URL").unwrap();
        let url = url::Url::parse(&base).unwrap();
        assert_eq!(url.scheme(), "http");
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        let account = self.library_access_user().await.unwrap();
        let client = LibraryClient {
            http: Client::builder()
                .redirect(Policy::none())
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
            base,
            account_id: account.clone(),
            control_token: std::env::var("ONECREW_TEST_LIBRARY_TOKEN").unwrap(),
        };
        let source = &publication.material.sources;
        let name = onecrew_creator_runtime::DiscoveryLibraryTableName {
            name: "联系方式现场验收_1005_1".into(),
            previous_auto_name: "联系方式现场验收_1005_1".into(),
        };
        let sync = self
            .sync_library_snapshot(&client, source, &account, "", &name)
            .await
            .unwrap();
        client
            .publish_identity(&sync.table_id, publication)
            .await
            .unwrap();
        let table = client
            .campaign_table(&source.campaign_id)
            .await
            .unwrap()
            .unwrap();
        let score = &table
            .columns
            .iter()
            .find(|column| column.name == "Match evaluation")
            .unwrap()
            .id;
        let view: Value = client
            .request(
                Method::GET,
                &format!("/api/v1/dynamic-tables/{}/creators", sync.table_id),
                None,
            )
            .await
            .unwrap();
        assert_eq!(view["state"], "current");
        assert_eq!(view["sources"].as_array().unwrap().len(), source.rows.len());
        for expected in &source.rows {
            let actual = view["sources"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["search_hit_id"] == expected.hit.id)
                .unwrap();
            assert_eq!(actual["verdict"], expected.verdict);
            assert_eq!(actual["reasons"], serde_json::json!(expected.reasons));
            assert_eq!(actual["contacts"], serde_json::json!(expected.contacts));
            // Pending sources retain their model receipt in the owner, while
            // the Library deliberately hides their recommendation rating.
            let visible_rating = if expected.verdict == "qualified"
                || (expected.verdict == "rejected"
                    && expected.match_rating.as_ref().is_some_and(|rating| {
                        rating.level == onecrew_creator_runtime::DiscoveryMatchLevel::Low
                    })) {
                serde_json::json!(expected.match_rating)
            } else {
                Value::Null
            };
            assert_eq!(actual["row"]["values"][score], visible_rating);
        }
        let retry = self
            .sync_library_snapshot(&client, source, &account, "", &name)
            .await
            .unwrap();
        assert_eq!(retry.inserted_rows, 0);
        assert_eq!(retry.table_id, sync.table_id);
        if std::env::var("ONECREW_TEST_NATIVE_ENTRANCE").is_ok()
            || std::env::var("ONECREW_TEST_LIBRARY_NO_CONTACT").as_deref() == Ok("true")
        {
            // The integration launcher owns this disposable receiver and paths;
            // no customer handshake or persistent Library is discovered here.
            let response = client
                .http
                .get(format!(
                    "{}/api/v1/dynamic-tables/{}/export",
                    client.base, sync.table_id
                ))
                .header("X-OneCrew-Account-Id", &account)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.headers()["content-type"],
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            );
            let bytes = response.bytes().await.unwrap();
            assert!(bytes.len() < 1024 * 1024);
            std::fs::write(std::env::var("ONECREW_TEST_LIBRARY_EXPORT").unwrap(), bytes).unwrap();
            let columns: Vec<_> = table
                .columns
                .iter()
                .map(|column| serde_json::json!({"id":column.id,"name":column.name}))
                .collect();
            let expected = serde_json::json!({"table":{"columns":columns},"view":view});
            std::fs::write(
                std::env::var("ONECREW_TEST_NATIVE_EXPECTATIONS").unwrap(),
                serde_json::to_vec(&expected).unwrap(),
            )
            .unwrap();
        }
        println!(
            "Live owner publication -> isolated Library creator-view and idempotent retry PASS"
        );
        println!(
            "isolated Library result URL: {}/library/{}",
            client.base, sync.table_id
        );
    }
}

#[derive(Deserialize)]
struct RuntimeDescriptor {
    service: String,
    protocol_version: u32,
    pid: u32,
    port: u16,
    url: String,
    instance_id: String,
    control_token: String,
    data_dir: PathBuf,
}

fn runtime_file() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|p| p.join("onecrew_lite_tables"));
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|p| p.join("Library/Application Support/onecrew_lite_tables"));
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|p| p.join(".local/share"))
        })
        .map(|p| p.join("onecrew_lite_tables"));
    #[cfg(not(any(unix, target_os = "windows")))]
    let base: Option<PathBuf> = None;
    base.filter(|p| p.is_absolute())
        .map(|p| p.join("run/server.json"))
        .ok_or_else(|| "library_runtime_path_unavailable".into())
}

fn read_descriptor(path: &Path) -> Result<RuntimeDescriptor, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| "library_runtime_unavailable")?;
    if !metadata.is_file() || metadata.len() > 16384 {
        return Err("library_runtime_invalid".into());
    }
    let file = std::fs::File::open(path).map_err(|_| "library_runtime_unavailable")?;
    let mut bytes = Vec::new();
    file.take(16385)
        .read_to_end(&mut bytes)
        .map_err(|_| "library_runtime_unavailable")?;
    if bytes.len() > 16384 {
        return Err("library_runtime_invalid".into());
    }
    let descriptor: RuntimeDescriptor =
        serde_json::from_slice(&bytes).map_err(|_| "library_runtime_invalid")?;
    if descriptor.service != "onecrew_lite"
        || descriptor.protocol_version != 1
        || descriptor.pid == 0
        || descriptor.port == 0
        || descriptor.url != format!("http://127.0.0.1:{}", descriptor.port)
        || uuid::Uuid::parse_str(&descriptor.instance_id).is_err()
        || descriptor.control_token.len() != 64
        || !descriptor
            .control_token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || !descriptor.data_dir.is_absolute()
    {
        return Err("library_runtime_invalid".into());
    }
    Ok(descriptor)
}

#[derive(Deserialize)]
struct Health {
    ok: bool,
    service: String,
    protocol_version: u32,
    engine: String,
    #[serde(default)]
    capabilities: Vec<String>,
}

fn require_sync_capabilities(health: &Health) -> Result<(), String> {
    if super::REQUIRED_LIBRARY_CAPABILITIES
        .iter()
        .all(|required| health.capabilities.iter().any(|value| value == required))
    {
        Ok(())
    } else {
        Err("library_sync_upgrade_required".into())
    }
}
#[derive(Deserialize)]
struct AccessStatus {
    state: String,
    user: Option<AccessUser>,
}
#[derive(Deserialize)]
struct AccessUser {
    id: String,
}
#[derive(Deserialize)]
struct InstanceStatus {
    instance_id: String,
}
#[derive(Deserialize)]
struct CampaignTable {
    table: Option<LibraryTable>,
}
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(super) struct ProjectionRow {
    pub row_id: String,
    pub search_hit_id: String,
    pub candidate_id: Option<String>,
    pub native_author_id: Option<String>,
    pub verdict: String,
    pub reasons: Vec<String>,
    pub evidence_version: String,
    #[serde(default)]
    pub contacts: Vec<onecrew_creator_runtime::DiscoveryContact>,
    pub criteria_revision: u32,
    pub source_revision: u64,
}
#[derive(Deserialize)]
struct StatePage {
    table_id: String,
    campaign_id: String,
    bound: bool,
    rows: Vec<ProjectionRow>,
    has_more: bool,
}

pub(super) struct LibraryClient {
    http: Client,
    pub base: String,
    account_id: String,
    control_token: String,
}

pub(super) fn query(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

impl LibraryClient {
    pub(super) async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<T, String> {
        let mut request = self
            .http
            .request(method.clone(), format!("{}{}", self.base, path))
            .header("X-OneCrew-Account-Id", &self.account_id);
        if path.starts_with("/_local/") {
            request = request.bearer_auth(&self.control_token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| "library_request_unavailable")?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "library_invalid_response")?
        {
            if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
                return Err("library_invalid_response".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            let code = body["error"]["code"].as_str().unwrap_or("");
            return Err(match code {
                "data.projection_stale" | "data.identity_stale" => "library_source_stale",
                "data.identity_source_version_conflict"
                | "data.identity_version_conflict"
                | "data.identity_sources_changed"
                | "data.identity_sources_incomplete"
                | "data.identity_stored_invalid" => "library_projection_conflict",
                "data.projection_row_missing" => "library_row_missing",
                "data.projection_unmanaged_table" => "library_unmanaged_table",
                "data.projection_version_conflict"
                | "data.projection_row_conflict"
                | "data.projection_table_conflict"
                | "data.projection_readback_conflict"
                | "data.projection_field_conflict"
                | "data.projection_field_name_conflict" => "library_projection_conflict",
                _ => match status {
                    StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::CONFLICT => {
                        "library_account_changed"
                    }
                    StatusCode::NOT_FOUND if path.starts_with("/_local/discovery/") => {
                        "library_sync_upgrade_required"
                    }
                    StatusCode::NOT_FOUND => "library_table_missing",
                    StatusCode::BAD_REQUEST | StatusCode::PAYLOAD_TOO_LARGE => {
                        "library_request_invalid"
                    }
                    StatusCode::SERVICE_UNAVAILABLE => "library_service_unavailable",
                    _ if method == Method::GET => "library_read_failed",
                    _ => "library_write_failed",
                },
            }
            .into());
        }
        serde_json::from_slice(&bytes).map_err(|_| "library_invalid_response".into())
    }

    pub(super) async fn campaign_table(
        &self,
        campaign: &str,
    ) -> Result<Option<LibraryTable>, String> {
        let response: CampaignTable = self
            .request(
                Method::GET,
                &format!("/_local/discovery/campaign?campaign_id={}", query(campaign)),
                None,
            )
            .await?;
        Ok(response.table)
    }

    pub(super) async fn state(
        &self,
        table: &str,
        campaign: &str,
    ) -> Result<HashMap<String, ProjectionRow>, String> {
        let mut rows = HashMap::new();
        let mut ids = HashSet::new();
        let mut after = String::new();
        for _ in 0..100 {
            let path = format!(
                "/_local/discovery/state?table_id={}&campaign_id={}&limit=100{}",
                query(table),
                query(campaign),
                if after.is_empty() {
                    String::new()
                } else {
                    format!("&after_hit_id={}", query(&after))
                }
            );
            let page: StatePage = self.request(Method::GET, &path, None).await?;
            if page.table_id != table
                || page.campaign_id != campaign
                || (!page.bound && (!page.rows.is_empty() || page.has_more))
                || (page.has_more && page.rows.is_empty())
            {
                return Err("library_invalid_response".into());
            }
            for row in page.rows {
                if row.row_id.is_empty()
                    || row.search_hit_id <= after
                    || !ids.insert(row.row_id.clone())
                    || row.source_revision == 0
                    || row.source_revision > 9_007_199_254_740_991
                    || row.criteria_revision == 0
                {
                    return Err("library_row_identity_changed".into());
                }
                after = row.search_hit_id.clone();
                rows.insert(row.search_hit_id.clone(), row);
            }
            if rows.len() > MAX_ROWS {
                return Err("library_capacity_exceeded".into());
            }
            if !page.has_more {
                return Ok(rows);
            }
        }
        Err("library_row_list_too_large".into())
    }
}

pub(super) async fn discover_library(expected_user: &str) -> Result<LibraryClient, String> {
    let descriptor = tokio::task::spawn_blocking(|| read_descriptor(&runtime_file()?))
        .await
        .map_err(|_| "library_runtime_unavailable")??;
    connect_descriptor(descriptor, expected_user).await
}

async fn connect_descriptor(
    descriptor: RuntimeDescriptor,
    expected_user: &str,
) -> Result<LibraryClient, String> {
    let http = Client::builder()
        .no_proxy()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "library_client_unavailable")?;
    let library = LibraryClient {
        http,
        base: descriptor.url,
        account_id: expected_user.into(),
        control_token: descriptor.control_token,
    };
    let health: Health = library.request(Method::GET, "/healthz", None).await?;
    if !health.ok
        || health.service != "onecrew_lite"
        || health.protocol_version != 1
        || health.engine != "teable-v2-pglite"
    {
        return Err("library_runtime_invalid".into());
    }
    require_sync_capabilities(&health)?;
    let instance: InstanceStatus = library.request(Method::GET, "/_local/status", None).await?;
    if instance.instance_id != descriptor.instance_id {
        return Err("library_instance_changed".into());
    }
    let access: AccessStatus = library
        .request(Method::GET, "/api/v1/access/status", None)
        .await?;
    if access.state != "signed_in" || access.user.is_none_or(|user| user.id != expected_user) {
        return Err("library_account_mismatch".into());
    }
    Ok(library)
}
