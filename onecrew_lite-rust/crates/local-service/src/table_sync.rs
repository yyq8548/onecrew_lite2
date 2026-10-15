//! Independent, consent-gated recommendation snapshots; never occupies the discovery runner.
use crate::{ServiceState, blocking::run_blocking};
use base64::Engine as _;
use onecrew_creator_sync::{TableSyncRequest as Request, TableSyncResponse};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const PART: usize = 8 * 1024 * 1024;
#[derive(Default)]
pub(crate) struct TableSyncOwner {
    pub(crate) lock: tokio::sync::Mutex<()>,
    started: AtomicBool,
    startup_reconciled: AtomicBool,
    retry_after: AtomicU64,
    #[cfg(test)]
    remote: std::sync::Mutex<Option<std::collections::VecDeque<Result<TableSyncResponse, String>>>>,
    #[cfg(test)]
    requests: std::sync::Mutex<Vec<Value>>,
    #[cfg(test)]
    capture: std::sync::Mutex<Option<Value>>,
}
#[derive(Default, Serialize, Deserialize)]
struct Store {
    local_source_id: Option<Uuid>,
    tables: BTreeMap<String, Table>,
    consent_operation: Option<Value>,
    #[serde(default)]
    consent_user: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Table {
    campaign_id: String,
    user_id: String,
    enabled: bool,
    next_check: u64,
    committed_hash: Option<String>,
    #[serde(default)]
    committed_consent: Option<Uuid>,
    #[serde(default)]
    committed_source: Option<Uuid>,
    #[serde(default)]
    failures: u8,
    previous_version: Option<Uuid>,
    sequence: u64,
    status: String,
    error: Option<String>,
    pending: Option<Pending>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Pending {
    input: Value,
    archive: String,
    archive_sha256: String,
    content_hash: String,
    record_count: u64,
    finished: String,
    parts: Vec<Value>,
    attempt: Option<Uuid>,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
fn key(user: &str, campaign: &str) -> String {
    format!("{:x}", Sha256::digest(format!("{user}\0{campaign}")))
}
async fn load(state: &ServiceState) -> Result<Store, String> {
    let path = state.data_dir.join("dynamic-table-sync.json");
    run_blocking("table_sync_store_read", move || match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| "table_sync_store_invalid".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Store::default()),
        Err(_) => Err("table_sync_store_unavailable".into()),
    })
    .await?
}
async fn save(state: &ServiceState, store: &Store) -> Result<(), String> {
    let path = state.data_dir.join("dynamic-table-sync.json");
    let bytes = serde_json::to_vec(store).map_err(|_| "table_sync_store_invalid")?;
    run_blocking("table_sync_store_write", move || atomic_save(path, &bytes)).await?
}
fn atomic_save(path: PathBuf, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    crate::private_file::write_private_file(&tmp, bytes)
        .map_err(|_| "table_sync_store_unavailable")?;
    let result = std::fs::rename(&tmp, &path).map_err(|_| "table_sync_store_unavailable".into());
    if result.is_err() && std::fs::remove_file(tmp).is_err() {
        log::warn!("table sync temporary file cleanup failed");
    }
    result
}
async fn remote(state: &ServiceState, request: Request) -> Result<TableSyncResponse, String> {
    send(state, None, request).await
}
async fn remote_for_user(
    state: &ServiceState,
    user: &str,
    request: Request,
) -> Result<TableSyncResponse, String> {
    send(state, Some(user), request).await
}
async fn send(
    state: &ServiceState,
    user: Option<&str>,
    request: Request,
) -> Result<TableSyncResponse, String> {
    let consent_control = matches!(request, Request::Consent | Request::SetConsent { .. });
    if !consent_control && now() < state.table_sync.retry_after.load(Ordering::Acquire) {
        return Err("table_sync_backoff".into());
    }
    #[cfg(test)]
    let mocked = test_remote(state, &request);
    #[cfg(not(test))]
    let mocked: Option<Result<TableSyncResponse, String>> = None;
    let result = if let Some(result) = mocked {
        result
    } else {
        state
            .sync()
            .await?
            .table_sync_request_for_user(request, user)
            .await
    };
    if let Ok(reply) = &result
        && (reply.status == 429 || reply.status == 503)
    {
        let delay = reply
            .retry_after
            .as_deref()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(300);
        state
            .table_sync
            .retry_after
            .fetch_max(now().saturating_add(delay), Ordering::AcqRel);
    }
    result
}
#[cfg(test)]
fn test_remote(
    state: &ServiceState,
    request: &Request,
) -> Option<Result<TableSyncResponse, String>> {
    let mut queue = state.table_sync.remote.lock().unwrap();
    let queue = queue.as_mut()?;
    state
        .table_sync
        .requests
        .lock()
        .unwrap()
        .push(serde_json::to_value(request).unwrap());
    Some(
        queue
            .pop_front()
            .unwrap_or_else(|| Err("unexpected_test_request".into())),
    )
}
fn success(reply: TableSyncResponse) -> Result<Value, String> {
    if !(200..300).contains(&reply.status) {
        return Err(format!("table_sync_http_{}", reply.status));
    }
    Ok(reply.body)
}
async fn capabilities(state: &ServiceState) -> Result<Value, String> {
    let value = success(remote(state, Request::Capabilities).await?)?;
    if value["protocol"] != "onecrew.dynamic_table_sync"
        || value["protocol_version"] != 1
        || value["limits"]["part_bytes"] != PART as u64
        || value["limits"]["collection_interval_seconds"] != 300
    {
        return Err("table_sync_protocol_mismatch".into());
    }
    Ok(value)
}
fn identity(cap: &Value) -> Result<(&str, Uuid), String> {
    let user = cap["identity"]["access_user_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("table_sync_identity_missing")?;
    let source = cap["identity"]["source_instance_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or("table_sync_identity_missing")?;
    Ok((user, source))
}
fn consent(cap: &Value) -> Result<Uuid, String> {
    if cap["upload_enabled"] != true
        || cap["consent_current"] != true
        || cap["consent"]["status"] != "granted"
    {
        return Err("table_sync_consent_required".into());
    }
    let scopes = cap["consent"]["scopes"]
        .as_array()
        .ok_or("table_sync_consent_required")?;
    if !scopes.contains(&json!("dynamic_table_schema"))
        || !scopes.contains(&json!("dynamic_table_records"))
    {
        return Err("table_sync_consent_required".into());
    }
    cap["consent"]["consent_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or("table_sync_consent_required".into())
}
pub(crate) async fn status(
    state: &Arc<ServiceState>,
    campaign_id: String,
) -> Result<Value, String> {
    ensure_started(state);
    let cap = capabilities(state).await?;
    let (user, _) = identity(&cap)?;
    let _lock = state.table_sync.lock.lock().await;
    let store = load(state).await?;
    let table = store.tables.get(&key(user, &campaign_id));
    Ok(
        json!({"capabilities":cap,"enabled":table.is_some_and(|t|t.enabled),
        "status":table.map(|t|t.status.as_str()).unwrap_or("disabled"),"error":table.and_then(|t|t.error.as_ref()),
        "version_id":table.and_then(|t|t.previous_version),"next_check":table.map(|t|t.next_check)}),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConsentInput {
    action: String,
    operation_id: Uuid,
    policy_fingerprint: Option<String>,
}
pub(crate) async fn change_consent(
    state: &ServiceState,
    input: ConsentInput,
) -> Result<Value, String> {
    if input.action != "grant" && input.action != "revoke" {
        return Err("table_sync_invalid_consent".into());
    }
    let _lock = state.table_sync.lock.lock().await;
    let mut store = load(state).await?;
    if input.action == "revoke" {
        disable_tables(&mut store);
        save(state, &store).await?;
    }
    let body = if input.action == "grant" {
        let cap = capabilities(state).await?;
        if cap["consent_management_enabled"] != true
            || cap["policy_fingerprint"].as_str() != input.policy_fingerprint.as_deref()
        {
            return Err("table_sync_policy_changed".into());
        }
        json!({"action":"grant","operation_id":input.operation_id,"policy_fingerprint":input.policy_fingerprint,"scopes":["dynamic_table_records","dynamic_table_schema"]})
    } else {
        json!({"action":"revoke","operation_id":input.operation_id})
    };
    let current = success(remote(state, Request::Consent).await?)?;
    let user = if let Some(user) = current["consent"]["identity"]["access_user_id"].as_str() {
        user.to_owned()
    } else {
        state.sync().await?.table_sync_access_user_id().await?
    };
    let mut body = body;
    // Recover an unknown operation after a UI or service restart with the original ID.
    // Revocation always supersedes a pending grant after reading the latest consent.
    if store.consent_user.as_deref() == Some(&user)
        && let Some(saved) = &store.consent_operation
        && saved["action"] == body["action"]
        && saved["policy_fingerprint"] == body["policy_fingerprint"]
    {
        body = saved.clone();
        if current["consent"]["operation_id"] == body["operation_id"] {
            store.consent_operation = None;
            store.consent_user = None;
            if input.action == "revoke" {
                disable_tables(&mut store);
            } else {
                resume_consent_tables(&mut store, &user);
            }
            save(state, &store).await?;
            return Ok(json!({"consent":current["consent"]}));
        }
    }
    store.consent_operation = Some(body.clone());
    store.consent_user = Some(user);
    save(state, &store).await?;
    let etag = current["consent_etag"].as_str().map(str::to_owned);
    let user = store
        .consent_user
        .as_deref()
        .ok_or("table_sync_identity_missing")?
        .to_owned();
    let expected_operation = body["operation_id"].clone();
    let reply = remote_for_user(state, &user, Request::SetConsent { input: body, etag }).await?;
    if reply.status == 412 {
        store.consent_operation = None;
        store.consent_user = None;
        save(state, &store).await?;
        return Err("table_sync_consent_changed".into());
    }
    let result = success(reply)?;
    let expected_status = if input.action == "grant" {
        "granted"
    } else {
        "revoked"
    };
    if result["consent"]["status"] != expected_status
        || result["consent"]["operation_id"] != expected_operation
    {
        return Err("table_sync_consent_unconfirmed".into());
    }
    store.consent_operation = None;
    store.consent_user = None;
    if input.action == "revoke" {
        disable_tables(&mut store);
    } else {
        resume_consent_tables(&mut store, &user);
    }
    save(state, &store).await?;
    Ok(result)
}
fn disable_tables(store: &mut Store) {
    for table in store.tables.values_mut() {
        table.enabled = false;
        table.status = "consent_revoked".into();
    }
}
fn resume_consent_tables(store: &mut Store, user: &str) {
    for table in store
        .tables
        .values_mut()
        .filter(|t| t.user_id == user && t.status == "consent_revoked")
    {
        table.enabled = true;
        table.status = "waiting".into();
        table.next_check = 0;
        table.failures = 0;
        table.error = None;
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfigureInput {
    campaign_id: String,
    enabled: bool,
}
pub(crate) async fn configure(
    state: &Arc<ServiceState>,
    input: ConfigureInput,
) -> Result<Value, String> {
    if !input.enabled {
        let _lock = state.table_sync.lock.lock().await;
        let mut store = load(state).await?;
        for table in store
            .tables
            .values_mut()
            .filter(|t| t.campaign_id == input.campaign_id)
        {
            table.enabled = false;
            table.status = "disabled".into();
        }
        save(state, &store).await?;
        return Ok(json!({"enabled":false}));
    }
    let cap = capabilities(state).await?;
    let (user, _) = identity(&cap)?;
    if input.enabled {
        let _ = consent(&cap)?;
    }
    let _lock = state.table_sync.lock.lock().await;
    let mut store = load(state).await?;
    let table = store
        .tables
        .entry(key(user, &input.campaign_id))
        .or_insert(Table {
            campaign_id: input.campaign_id,
            user_id: user.into(),
            enabled: false,
            next_check: 0,
            committed_hash: None,
            committed_consent: None,
            committed_source: None,
            failures: 0,
            previous_version: None,
            sequence: 0,
            status: "disabled".into(),
            error: None,
            pending: None,
        });
    table.enabled = input.enabled;
    table.failures = 0;
    table.next_check = 0;
    table.status = if input.enabled { "waiting" } else { "disabled" }.into();
    save(state, &store).await?;
    drop(_lock);
    ensure_started(state);
    Ok(json!({"enabled":input.enabled}))
}
pub(crate) fn ensure_started(state: &Arc<ServiceState>) {
    if state.table_sync.started.swap(true, Ordering::AcqRel) {
        return;
    }
    let weak = Arc::downgrade(state);
    tokio::spawn(async move {
        loop {
            let Some(state) = weak.upgrade() else { break };
            if state.shutdown.is_cancelled() {
                break;
            }
            if tick(&state).await.is_err() {
                log::debug!("table sync background tick did not complete");
            }
            let shutdown = state.shutdown.clone();
            drop(state);
            tokio::select! { _=shutdown.cancelled()=>break,_=tokio::time::sleep(std::time::Duration::from_secs(30))=>{} }
        }
    });
}
/// Register a successfully published local recommendation table. Upload admission
/// remains in tick(): registration never creates or substitutes server consent.
pub(crate) async fn register_result(
    state: &ServiceState,
    campaign_id: &str,
    expected_account: &str,
) -> Result<(), String> {
    let user = state.sync().await?.table_sync_access_user_id().await?;
    let _lock = state.table_sync.lock.lock().await;
    if crate::discovery_execution::account(state).await? != expected_account {
        return Err("table_sync_account_changed".into());
    }
    let mut store = load(state).await?;
    if register_table(&mut store, &user, campaign_id) {
        save(state, &store).await?;
    }
    Ok(())
}
fn register_table(store: &mut Store, user: &str, campaign_id: &str) -> bool {
    use std::collections::btree_map::Entry;
    match store.tables.entry(key(user, campaign_id)) {
        Entry::Occupied(mut entry) => {
            let table = entry.get_mut();
            if table.status != "needs_attention" {
                if !table.enabled {
                    return false;
                }
                let changed = table.next_check != 0 || table.status != "waiting";
                table.next_check = 0;
                table.status = "waiting".into();
                return changed;
            }
            table.enabled = true;
            table.status = "waiting".into();
            table.next_check = 0;
            table.failures = 0;
            table.error = None;
            true
        }
        Entry::Vacant(entry) => {
            entry.insert(Table {
                campaign_id: campaign_id.into(),
                user_id: user.into(),
                enabled: true,
                next_check: 0,
                committed_hash: None,
                committed_consent: None,
                committed_source: None,
                failures: 0,
                previous_version: None,
                sequence: 0,
                status: "waiting".into(),
                error: None,
                pending: None,
            });
            true
        }
    }
}
fn queued(table: &Table) -> bool {
    table.enabled
        && !matches!(table.status.as_str(), "synced" | "up_to_date")
        && table.next_check <= now()
}
async fn tick(state: &ServiceState) -> Result<(), String> {
    let _lock = state.table_sync.lock.lock().await;
    // Admission holds this same lock: a search cannot start during an upload,
    // and no snapshot can start while reads, scores or final Library writes drain.
    if state.discovery_execution.blocks_backend_sync() {
        return Ok(());
    }
    let mut store = load(state).await?;
    if !state.table_sync.startup_reconciled.load(Ordering::Acquire) {
        // Reconcile once after restart, including legacy periodic-sync records:
        // the process may have stopped between final publication and enqueue.
        let mut changed = false;
        for table in store.tables.values_mut().filter(|t| t.enabled) {
            if matches!(table.status.as_str(), "synced" | "up_to_date") {
                table.status = "waiting".into();
                table.next_check = 0;
                changed = true;
            }
        }
        if changed {
            save(state, &store).await?;
        }
        state
            .table_sync
            .startup_reconciled
            .store(true, Ordering::Release);
    }
    if !store.tables.values().any(queued) {
        return Ok(());
    }
    let admitted = async {
        let cap = capabilities(state).await?;
        let (user, source) = identity(&cap)?;
        Ok::<_, String>((user.to_owned(), source, consent(&cap)?))
    }
    .await;
    let (user, source, consent_id) = match admitted {
        Ok(v) => v,
        Err(error) => {
            for table in store.tables.values_mut().filter(|t| queued(t)) {
                table.next_check =
                    (now() + 300).max(state.table_sync.retry_after.load(Ordering::Acquire));
                table.error = Some(error.clone());
                table.status = "error".into();
            }
            save(state, &store).await?;
            return Err(error);
        }
    };
    let local = *store.local_source_id.get_or_insert_with(Uuid::new_v4);
    save(state, &store).await?;
    let keys: Vec<_> = store
        .tables
        .iter()
        .filter(|(_, t)| queued(t) && t.user_id == user)
        .map(|(k, _)| k.clone())
        .collect();
    for k in keys {
        let mut table = store.tables.remove(&k).ok_or("table_sync_store_invalid")?;
        table.next_check = now() + 300;
        let recovering_snapshot = table.pending.is_some();
        let result = run_table(state, &mut table, local, source, consent_id).await;
        if result.is_ok() {
            // A published result or settled search explicitly queues the next
            // capture. The timer only drains queued work and transient retries.
            table.next_check = 0;
            // A durable retry may contain the previous pause's snapshot. After
            // recovering that exact commit, reconcile the latest stopped result
            // once; its content hash prevents a duplicate version.
            if recovering_snapshot {
                table.status = "waiting".into();
            }
        }
        if let Err(error) = result {
            table.error = Some(error);
            table.failures = table.failures.saturating_add(1);
            table.next_check = (now() + 300 * (1u64 << table.failures.min(3)))
                .max(state.table_sync.retry_after.load(Ordering::Acquire));
            // Background sync remains queued after transient failures. The capped
            // backoff prevents a retry loop; every attempt still checks consent.
            table.status = "error".into();
        }
        store.tables.insert(k, table);
        save(state, &store).await?;
    }
    Ok(())
}
pub(crate) async fn search_settled(
    state: &ServiceState,
    campaign_id: &str,
    credential: &str,
) -> Result<(), String> {
    let _lock = state.table_sync.lock.lock().await;
    if crate::discovery_execution::account(state).await? != credential {
        return Err("table_sync_account_changed".into());
    }
    let user = state.sync().await?.table_sync_access_user_id().await?;
    let mut store = load(state).await?;
    if let Some(table) = store
        .tables
        .get_mut(&key(&user, campaign_id))
        .filter(|t| t.enabled)
    {
        table.next_check = 0;
        table.status = "waiting".into();
        save(state, &store).await?;
    }
    Ok(())
}
async fn run_table(
    state: &ServiceState,
    table: &mut Table,
    local: Uuid,
    source: Uuid,
    consent_id: Uuid,
) -> Result<(), String> {
    if table.pending.is_none() {
        let version = Uuid::new_v4();
        let started = timestamp();
        #[cfg(test)]
        let captured = state.table_sync.capture.lock().unwrap().clone();
        #[cfg(not(test))]
        let captured: Option<Value> = None;
        let capture = match captured {
            Some(value) => value,
            None => state.platform().await?.capture_discovery_recommendations(table.campaign_id.clone(),table.user_id.clone(),json!({
                "version_id":version,"source_instance_id":source,"consent_id":consent_id,"capture_started_at":started})).await?,
        };
        let hash = capture["content_hash"]
            .as_str()
            .ok_or("table_sync_capture_invalid")?
            .to_owned();
        if table.committed_hash.as_ref() == Some(&hash)
            && table.committed_consent == Some(consent_id)
            && table.committed_source == Some(source)
        {
            table.failures = 0;
            table.status = "up_to_date".into();
            table.error = None;
            return Ok(());
        }
        let archive = capture["archive"]
            .as_str()
            .ok_or("table_sync_capture_invalid")?
            .to_owned();
        let count = capture["record_count"]
            .as_u64()
            .ok_or("table_sync_capture_invalid")?;
        validate_archive(&archive, count)?;
        table.sequence = table
            .sequence
            .checked_add(1)
            .ok_or("table_sync_sequence_exhausted")?;
        let input = json!({"version_id":version,"consent_id":consent_id,"local_source_id":local,
            "table_id":capture["table_id"],"table_name":capture["table_name"],"capture_seq":table.sequence.to_string(),
            "capture_kind":"full_sample","capture_started_at":started,"previous_version_id":table.previous_version,
            "scopes":["dynamic_table_records","dynamic_table_schema"],"format":"onecrew.dynamic_table_capture","format_version":1});
        table.pending = Some(Pending {
            input,
            archive_sha256: format!("{:x}", Sha256::digest(archive.as_bytes())),
            archive,
            content_hash: hash,
            record_count: count,
            finished: timestamp(),
            parts: Vec::new(),
            attempt: None,
        });
    }
    checkpoint(state, table).await?;
    upload(state, table, consent_id).await
}
fn validate_archive(archive: &str, count: u64) -> Result<(), String> {
    let mut schema = None;
    let mut fields = Vec::new();
    let mut ids = std::collections::HashSet::new();
    let mut rows = 0;
    for (index, line) in archive.lines().enumerate() {
        let v: Value = serde_json::from_str(line).map_err(|_| "table_sync_capture_invalid")?;
        match v["kind"].as_str() {
            Some("header")
                if index == 0
                    && v["format"] == "onecrew.dynamic_table_capture"
                    && v["capture_kind"] == "full_sample" =>
            {
                if v["format_version"] != 1
                    || ["version_id", "source_instance_id", "consent_id"]
                        .iter()
                        .any(|f| {
                            v[*f]
                                .as_str()
                                .and_then(|s| Uuid::parse_str(s).ok())
                                .is_none()
                        })
                    || v["table_id"]
                        .as_str()
                        .is_none_or(|s| s.is_empty() || s.len() > 128)
                {
                    return Err("table_sync_capture_invalid".into());
                }
            }
            Some("schema") if index == 1 => {
                let expected = [
                    ("name", "people"),
                    ("category", "string"),
                    ("platform", "string"),
                    ("score", "score"),
                    ("contacts", "string"),
                ];
                let fs = v["fields"].as_array().ok_or("table_sync_capture_invalid")?;
                if fs.len() != 5
                    || fs
                        .iter()
                        .zip(expected)
                        .any(|(f, (id, ty))| f["id"] != id || f["semantic_type"] != ty)
                {
                    return Err("table_sync_capture_invalid".into());
                }
                fields = fs
                    .iter()
                    .map(|f| f["id"].as_str().unwrap().to_owned())
                    .collect();
                schema = v["schema_id"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned);
            }
            Some("row.upsert") if index >= 2 => {
                if v["schema_id"].as_str() != schema.as_deref()
                    || v["values"].as_object().map(|m| m.len()) != Some(5)
                    || fields.iter().any(|f| v["values"].get(f).is_none())
                    || v["values"]["name"]["name"]
                        .as_str()
                        .is_none_or(|s| s.trim().is_empty())
                    || v["values"]["name"]["identity"]["platform_id"]
                        .as_str()
                        .is_none_or(|s| s.is_empty())
                    || !matches!(
                        v["values"]["score"]["level"].as_str(),
                        Some("excellent" | "good" | "medium")
                    )
                    || v["values"]["score"]["reason"]
                        .as_str()
                        .is_none_or(|s| s.trim().is_empty())
                    || v["values"]["contacts"]
                        .as_str()
                        .is_none_or(|s| s.trim().is_empty())
                    || v["order"].as_u64() != Some(rows)
                    || !ids.insert(
                        v["row_id"]
                            .as_str()
                            .ok_or("table_sync_capture_invalid")?
                            .to_owned(),
                    )
                {
                    return Err("table_sync_capture_invalid".into());
                }
                rows += 1;
            }
            _ => return Err("table_sync_capture_invalid".into()),
        }
    }
    if schema.is_none() || rows != count {
        return Err("table_sync_capture_invalid".into());
    }
    Ok(())
}
async fn checkpoint(state: &ServiceState, table: &Table) -> Result<(), String> {
    let mut store = load(state).await?;
    store
        .tables
        .insert(key(&table.user_id, &table.campaign_id), table.clone());
    save(state, &store).await
}
async fn upload(state: &ServiceState, table: &mut Table, consent: Uuid) -> Result<(), String> {
    let pending = table.pending.as_ref().ok_or("table_sync_pending_missing")?;
    let version = Uuid::parse_str(
        pending.input["version_id"]
            .as_str()
            .ok_or("table_sync_pending_invalid")?,
    )
    .map_err(|_| "table_sync_pending_invalid")?;
    validate_pending(pending)?;
    let status = remote_for_user(
        state,
        &table.user_id,
        Request::Status {
            version_id: version,
        },
    )
    .await?;
    let envelope = if status.status == 404 {
        if pending.input["consent_id"] != consent.to_string() {
            return Err("table_sync_pending_consent_changed".into());
        }
        success(
            remote_for_user(
                state,
                &table.user_id,
                Request::Begin {
                    input: pending.input.clone(),
                },
            )
            .await?,
        )?
    } else {
        success(status)?
    };
    let mut body = envelope
        .get("upload")
        .cloned()
        .ok_or("table_sync_upload_response_invalid")?;
    if body["version_id"] != version.to_string() {
        return Err("table_sync_version_mismatch".into());
    }
    if body["status"] == "committed" {
        return commit(state, table, &body).await;
    }
    if body["status"] == "aborted" {
        table.pending = None;
        checkpoint(state, table).await?;
        return Err("table_sync_aborted".into());
    }
    if body["status"] == "initializing" {
        let pending = table.pending.as_ref().ok_or("table_sync_pending_missing")?;
        if pending.input["consent_id"] != consent.to_string() {
            return Err("table_sync_pending_consent_changed".into());
        }
        body = success(
            remote_for_user(
                state,
                &table.user_id,
                Request::Begin {
                    input: pending.input.clone(),
                },
            )
            .await?,
        )?["upload"]
            .clone();
        if body["status"] == "initializing" {
            return Err("table_sync_initializing".into());
        }
        if body["status"] == "aborted" {
            table.pending = None;
            checkpoint(state, table).await?;
            return Err("table_sync_aborted".into());
        }
        if body["status"] == "committed" {
            return commit(state, table, &body).await;
        }
    }
    if body["version_id"] != version.to_string() {
        return Err("table_sync_version_mismatch".into());
    }
    if body["status"] != "uploading" && body["status"] != "completing" {
        return Err("table_sync_upload_response_invalid".into());
    }
    let attempt = body["attempt_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or("table_sync_attempt_missing")?;
    if table
        .pending
        .as_ref()
        .ok_or("table_sync_pending_missing")?
        .input["consent_id"]
        != consent.to_string()
    {
        let ended = success(
            remote_for_user(
                state,
                &table.user_id,
                Request::Abort {
                    version_id: version,
                    attempt_id: attempt,
                },
            )
            .await?,
        )?;
        if ended["upload"]["status"] != "aborted" {
            return Err("table_sync_abort_unconfirmed".into());
        }
        table.pending = None;
        checkpoint(state, table).await?;
        return Err("table_sync_pending_consent_changed".into());
    }
    {
        let pending = table.pending.as_mut().ok_or("table_sync_pending_missing")?;
        if pending.attempt != Some(attempt) {
            pending.parts.clear();
            pending.attempt = Some(attempt);
        }
    }
    checkpoint(state, table).await?;
    let total = table
        .pending
        .as_ref()
        .ok_or("table_sync_pending_missing")?
        .archive
        .len()
        .div_ceil(PART);
    if total > 2048 {
        return Err("table_sync_archive_too_large".into());
    }
    for index in 0..total {
        let pending = table.pending.as_ref().ok_or("table_sync_pending_missing")?;
        if pending.parts.len() > index {
            continue;
        }
        // Recheck live consent before every part; stale cached permission is insufficient.
        let cap = capabilities(state).await?;
        if identity(&cap)?.0 != table.user_id || self::consent(&cap)? != consent {
            return Err("table_sync_consent_changed".into());
        }
        let bytes = pending.archive.as_bytes()
            [index * PART..((index + 1) * PART).min(pending.archive.len())]
            .to_vec();
        let digest = md5::compute(&bytes);
        let hex = format!("{digest:x}");
        let envelope = success(
            remote_for_user(
                state,
                &table.user_id,
                Request::Part {
                    version_id: version,
                    attempt_id: attempt,
                    number: (index + 1) as u16,
                    md5_base64: base64::engine::general_purpose::STANDARD.encode(digest.0),
                    bytes: bytes.clone(),
                },
            )
            .await?,
        )?;
        let part = envelope
            .get("part")
            .cloned()
            .ok_or("table_sync_part_unconfirmed")?;
        if part["part_number"] != (index + 1) as u64
            || part["bytes"] != bytes.len() as u64
            || part["md5"] != hex
            || part["etag"].as_str().is_none_or(|s| s.is_empty())
        {
            return Err("table_sync_part_unconfirmed".into());
        }
        table
            .pending
            .as_mut()
            .ok_or("table_sync_pending_missing")?
            .parts
            .push(part);
        checkpoint(state, table).await?;
    }
    let pending = table.pending.as_ref().ok_or("table_sync_pending_missing")?;
    let input = json!({"attempt_id":attempt,"capture_finished_at":pending.finished,"record_count":pending.record_count,
        "schema_count":1,"parts":pending.parts});
    let cap = capabilities(state).await?;
    if identity(&cap)?.0 != table.user_id || self::consent(&cap)? != consent {
        return Err("table_sync_consent_changed".into());
    }
    let result = success(
        remote_for_user(
            state,
            &table.user_id,
            Request::Complete {
                version_id: version,
                input,
            },
        )
        .await?,
    )?["upload"]
        .clone();
    if result["status"] == "committed" {
        commit(state, table, &result).await
    } else {
        Err("table_sync_completion_unconfirmed".into())
    }
}
async fn commit(state: &ServiceState, table: &mut Table, body: &Value) -> Result<(), String> {
    let pending = table.pending.as_ref().ok_or("table_sync_pending_missing")?;
    let m = &body["manifest"];
    validate_manifest(pending, body)?;
    table.previous_version = Some(
        Uuid::parse_str(
            m["version_id"]
                .as_str()
                .ok_or("table_sync_manifest_mismatch")?,
        )
        .map_err(|_| "table_sync_manifest_mismatch")?,
    );
    table.committed_hash = Some(pending.content_hash.clone());
    table.committed_consent = pending.input["consent_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok());
    table.committed_source = m["source_instance_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok());
    table.failures = 0;
    table.pending = None;
    table.status = "synced".into();
    table.error = None;
    checkpoint(state, table).await
}

fn validate_pending(p: &Pending) -> Result<(), String> {
    if format!("{:x}", Sha256::digest(p.archive.as_bytes())) != p.archive_sha256 {
        return Err("table_sync_pending_integrity_failed".into());
    }
    validate_archive(&p.archive, p.record_count)?;
    let h: Value = serde_json::from_str(
        p.archive
            .lines()
            .next()
            .ok_or("table_sync_pending_invalid")?,
    )
    .map_err(|_| "table_sync_pending_invalid")?;
    for f in [
        "version_id",
        "consent_id",
        "table_id",
        "table_name",
        "capture_started_at",
        "capture_kind",
        "format",
        "format_version",
    ] {
        if h[f] != p.input[f] {
            return Err("table_sync_pending_integrity_failed".into());
        }
    }
    for (i, part) in p.parts.iter().enumerate() {
        let bytes = p
            .archive
            .as_bytes()
            .chunks(PART)
            .nth(i)
            .ok_or("table_sync_pending_integrity_failed")?;
        if part["part_number"] != (i + 1) as u64
            || part["bytes"] != bytes.len() as u64
            || part["md5"] != format!("{:x}", md5::compute(bytes))
            || part["etag"].as_str().is_none_or(|s| s.is_empty())
        {
            return Err("table_sync_pending_integrity_failed".into());
        }
    }
    Ok(())
}
fn validate_manifest(p: &Pending, body: &Value) -> Result<(), String> {
    validate_pending(p)?;
    let m = &body["manifest"];
    if body["status"] != "committed" || body["version_id"] != p.input["version_id"] {
        return Err("table_sync_manifest_mismatch".into());
    }
    for f in [
        "version_id",
        "consent_id",
        "local_source_id",
        "table_id",
        "table_name",
        "capture_seq",
        "capture_kind",
        "capture_started_at",
        "previous_version_id",
        "scopes",
        "format",
        "format_version",
    ] {
        if m[f] != p.input[f] {
            return Err("table_sync_manifest_mismatch".into());
        }
    }
    let h: Value = serde_json::from_str(
        p.archive
            .lines()
            .next()
            .ok_or("table_sync_pending_invalid")?,
    )
    .map_err(|_| "table_sync_pending_invalid")?;
    if m["bytes"] != p.archive.len() as u64
        || m["record_count"] != p.record_count
        || m["schema_count"] != 1
        || m["source_instance_id"] != h["source_instance_id"]
        || m["capture_finished_at"] != p.finished
        || m["attempt_id"] != json!(p.attempt)
        || m["parts"] != json!(p.parts)
        || p.parts.len() != p.archive.len().div_ceil(PART)
        || m["etag"].as_str().is_none_or(|s| s.is_empty())
    {
        return Err("table_sync_manifest_mismatch".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn settled_uploads_wait_for_a_new_result_instead_of_periodic_recapture() {
        let mut store = super::Store::default();
        super::register_table(&mut store, "user", "campaign");
        let key = super::key("user", "campaign");
        assert!(super::queued(&store.tables[&key]));
        for status in ["synced", "up_to_date"] {
            store.tables.get_mut(&key).unwrap().status = status.into();
            assert!(!super::queued(&store.tables[&key]));
            assert!(super::register_table(&mut store, "user", "campaign"));
            assert!(super::queued(&store.tables[&key]));
        }
        let table = store.tables.get_mut(&key).unwrap();
        table.status = "error".into();
        table.next_check = super::now() + 300;
        assert!(!super::queued(table));
        table.next_check = 0;
        assert!(super::queued(table));
    }
    #[test]
    fn automatic_registration_is_account_scoped_and_preserves_existing_state() {
        let mut store = super::Store::default();
        assert!(super::register_table(&mut store, "user-a", "campaign"));
        assert!(store.tables[&super::key("user-a", "campaign")].enabled);
        let table = store
            .tables
            .get_mut(&super::key("user-a", "campaign"))
            .unwrap();
        table.enabled = false;
        table.status = "consent_revoked".into();
        assert!(!super::register_table(&mut store, "user-a", "campaign"));
        assert!(!store.tables[&super::key("user-a", "campaign")].enabled);
        assert!(super::register_table(&mut store, "user-b", "campaign"));
        assert_eq!(store.tables.len(), 2);
        super::resume_consent_tables(&mut store, "user-b");
        assert!(!store.tables[&super::key("user-a", "campaign")].enabled);
        super::resume_consent_tables(&mut store, "user-a");
        assert!(store.tables[&super::key("user-a", "campaign")].enabled);
        assert!(
            super::consent(&serde_json::json!({"upload_enabled":true,"consent_current":false}))
                .is_err()
        );
    }
    #[tokio::test]
    async fn registered_tables_wait_without_uploading_when_server_consent_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::ServiceState::new(
            dir.path().into(),
            &[],
            "/test/onecrew".into(),
            "test".into(),
        );
        let mut store = super::Store::default();
        super::register_table(&mut store, "user", "campaign");
        super::save(&state, &store).await.unwrap();
        *state.table_sync.remote.lock().unwrap() = Some(std::collections::VecDeque::from(vec![
            Ok(onecrew_creator_sync::TableSyncResponse {
                status: 200,
                etag: None,
                retry_after: None,
                body: serde_json::json!({"protocol":"onecrew.dynamic_table_sync", "protocol_version":1,
                    "limits":{"part_bytes":8388608,"collection_interval_seconds":300},
                    "identity":{"access_user_id":"user","source_instance_id":uuid::Uuid::new_v4()},
                    "upload_enabled":false,"consent_current":false}),
            }),
        ]));
        assert_eq!(
            super::tick(&state).await.err().as_deref(),
            Some("table_sync_consent_required")
        );
        let requests = state.table_sync.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["operation"], "capabilities");
    }
    use super::*;
    fn fixture(count: u64) -> Pending {
        let version = Uuid::new_v4();
        let source = Uuid::new_v4();
        let cid = Uuid::new_v4();
        let started = timestamp();
        let input = json!({"version_id":version,"consent_id":cid,"local_source_id":Uuid::new_v4(),"table_id":"table", "table_name":"Recommendations",
            "capture_seq":"1","capture_kind":"full_sample","capture_started_at":started,"previous_version_id":null,
            "scopes":["dynamic_table_records","dynamic_table_schema"],"format":"onecrew.dynamic_table_capture","format_version":1});
        let mut header = input.clone();
        let obj = header.as_object_mut().unwrap();
        for f in [
            "local_source_id",
            "capture_seq",
            "previous_version_id",
            "scopes",
        ] {
            obj.remove(f);
        }
        obj.insert("kind".into(), json!("header"));
        obj.insert("source_instance_id".into(), json!(source));
        let fields: Vec<_> = [
            ("name", "people"),
            ("category", "string"),
            ("platform", "string"),
            ("score", "score"),
            ("contacts", "string"),
        ]
        .iter()
        .map(|(id, ty)| json!({"id":id,"name":id,"semantic_type":ty}))
        .collect();
        let mut lines = vec![
            header,
            json!({"kind":"schema","schema_id":"schema","observed_at":started,"fields":fields}),
        ];
        for order in 0..count {
            lines.push(json!({"kind":"row.upsert","row_id":format!("row{order}"),"schema_id":"schema","observed_at":started,"source_row_version":"v1","order":order,
            "values":{"name":{"name":"Author","identity":{"platform":"youtube","platform_id":"channel"}},"category":null,"platform":"youtube","score":{"level":"good","reason":"Complete assessment"},"contacts":"YouTube: https://www.youtube.com/@author"}}));
        }
        let archive = lines
            .iter()
            .map(|v| v.to_string() + "\n")
            .collect::<String>();
        Pending {
            input,
            archive_sha256: format!("{:x}", Sha256::digest(archive.as_bytes())),
            archive,
            content_hash: "a".repeat(64),
            record_count: count,
            finished: timestamp(),
            parts: vec![],
            attempt: None,
        }
    }
    fn committed(p: &mut Pending) -> Value {
        p.attempt = Some(Uuid::new_v4());
        p.parts=p.archive.as_bytes().chunks(PART).enumerate().map(|(i,b)|json!({"part_number":i+1,"bytes":b.len(),"etag":"part-etag","md5":format!("{:x}",md5::compute(b))})).collect();
        let header: Value = serde_json::from_str(p.archive.lines().next().unwrap()).unwrap();
        let mut manifest = p.input.clone();
        let m = manifest.as_object_mut().unwrap();
        m.insert(
            "source_instance_id".into(),
            header["source_instance_id"].clone(),
        );
        for (k, v) in [
            ("bytes", json!(p.archive.len())),
            ("record_count", json!(p.record_count)),
            ("schema_count", json!(1)),
            ("capture_finished_at", json!(p.finished)),
            ("attempt_id", json!(p.attempt)),
            ("parts", json!(p.parts)),
            ("etag", json!("archive-etag")),
        ] {
            m.insert(k.into(), v);
        }
        json!({"version_id":p.input["version_id"],"status":"committed","manifest":manifest})
    }
    #[test]
    fn full_and_empty_snapshots_are_valid() {
        for count in [0, 2] {
            assert!(validate_pending(&fixture(count)).is_ok());
        }
    }
    #[test]
    fn corrupted_bytes_and_parts_cannot_resume() {
        let mut p = fixture(1);
        p.archive.push(' ');
        assert!(validate_pending(&p).is_err());
        let mut p = fixture(1);
        let _ = committed(&mut p);
        p.parts[0]["md5"] = json!("bad");
        assert!(validate_pending(&p).is_err());
    }
    #[test]
    fn only_matching_committed_receipts_are_accepted() {
        let mut p = fixture(1);
        let mut body = committed(&mut p);
        assert!(validate_manifest(&p, &body).is_ok());
        body["manifest"]["source_instance_id"] = json!(Uuid::new_v4());
        assert!(validate_manifest(&p, &body).is_err());
        let mut body = committed(&mut p);
        body["status"] = json!("completing");
        assert!(validate_manifest(&p, &body).is_err());
    }
    #[test]
    fn restart_keeps_identical_version_bytes_and_parts() {
        let mut p = fixture(2);
        let body = committed(&mut p);
        let path = std::env::temp_dir().join(format!("table-sync-{}.json", Uuid::new_v4()));
        let bytes = serde_json::to_vec(&p).unwrap();
        atomic_save(path.clone(), &bytes).unwrap();
        let restored: Pending = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(restored.archive, p.archive);
        assert_eq!(restored.input, p.input);
        assert_eq!(restored.parts, p.parts);
        assert!(validate_manifest(&restored, &body).is_ok());
    }
    #[test]
    fn recommendation_gate_rejects_empty_contact_and_low_score() {
        let p = fixture(1);
        let mut events: Vec<Value> = p
            .archive
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        events[2]["values"]["contacts"] = json!("");
        let archive = events
            .iter()
            .map(|v| v.to_string() + "\n")
            .collect::<String>();
        assert!(validate_archive(&archive, 1).is_err());
    }
    #[tokio::test]
    async fn unknown_completion_recovers_by_status_after_restart_without_reupload() {
        let dir = tempfile::tempdir().unwrap();
        let state = ServiceState::new(
            dir.path().into(),
            &[],
            "/test/onecrew".into(),
            "test".into(),
        );
        let mut final_pending = fixture(1);
        let receipt = committed(&mut final_pending);
        let header: Value =
            serde_json::from_str(final_pending.archive.lines().next().unwrap()).unwrap();
        let cap = json!({"protocol":"onecrew.dynamic_table_sync","protocol_version":1,"identity":{"access_user_id":"user","source_instance_id":header["source_instance_id"]},"upload_enabled":true,"consent_current":true,
            "consent":{"status":"granted","consent_id":final_pending.input["consent_id"],"scopes":["dynamic_table_records","dynamic_table_schema"]},"limits":{"part_bytes":PART,"collection_interval_seconds":300}});
        let response = |status, body| {
            Ok(TableSyncResponse {
                status,
                etag: None,
                retry_after: None,
                body,
            })
        };
        *state.table_sync.remote.lock().unwrap() = Some(std::collections::VecDeque::from(vec![
            response(404, json!({})),
            response(
                201,
                json!({"upload":{"version_id":final_pending.input["version_id"],"attempt_id":final_pending.attempt,"status":"uploading"}}),
            ),
            response(200, cap.clone()),
            response(200, json!({"part":final_pending.parts[0]})),
            response(200, cap),
            Err("table_sync_transport_unknown".into()),
        ]));
        let mut initial = final_pending.clone();
        initial.parts.clear();
        initial.attempt = None;
        let cid = Uuid::parse_str(initial.input["consent_id"].as_str().unwrap()).unwrap();
        let mut table = Table {
            campaign_id: "campaign".into(),
            user_id: "user".into(),
            enabled: true,
            next_check: 0,
            committed_hash: None,
            committed_consent: None,
            committed_source: None,
            failures: 0,
            previous_version: None,
            sequence: 1,
            status: "waiting".into(),
            error: None,
            pending: Some(initial),
        };
        checkpoint(&state, &table).await.unwrap();
        assert_eq!(
            upload(&state, &mut table, cid).await.err().as_deref(),
            Some("table_sync_transport_unknown")
        );
        let stored = load(&state).await.unwrap();
        let mut restored = stored.tables[&key("user", "campaign")].clone();
        assert_eq!(
            restored.pending.as_ref().unwrap().archive,
            final_pending.archive
        );
        assert_eq!(
            restored.pending.as_ref().unwrap().parts,
            final_pending.parts
        );
        let restarted = ServiceState::new(
            dir.path().into(),
            &[],
            "/test/onecrew".into(),
            "test".into(),
        );
        *restarted.table_sync.remote.lock().unwrap() = Some(std::collections::VecDeque::from(
            vec![response(200, json!({"upload":receipt}))],
        ));
        upload(&restarted, &mut restored, cid).await.unwrap();
        assert!(restored.pending.is_none());
        assert_eq!(restored.status, "synced");
        let requests = restarted.table_sync.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["operation"], "status");
    }
    #[tokio::test]
    async fn recovered_snapshot_reconciles_latest_result_once_without_duplicate_commit() {
        let dir = tempfile::tempdir().unwrap();
        let state = ServiceState::new(
            dir.path().into(),
            &[],
            "/test/onecrew".into(),
            "test".into(),
        );
        let mut pending = fixture(1);
        let receipt = committed(&mut pending);
        let header: Value = serde_json::from_str(pending.archive.lines().next().unwrap()).unwrap();
        let cap = json!({"protocol":"onecrew.dynamic_table_sync","protocol_version":1,
            "identity":{"access_user_id":"user","source_instance_id":header["source_instance_id"]},
            "upload_enabled":true,"consent_current":true,
            "consent":{"status":"granted","consent_id":pending.input["consent_id"],"scopes":["dynamic_table_records","dynamic_table_schema"]},
            "limits":{"part_bytes":PART,"collection_interval_seconds":300}});
        let response = |body| {
            Ok(TableSyncResponse {
                status: 200,
                etag: None,
                retry_after: None,
                body,
            })
        };
        *state.table_sync.remote.lock().unwrap() = Some(std::collections::VecDeque::from(vec![
            response(cap.clone()),
            response(json!({"upload":receipt})),
            response(cap.clone()),
        ]));
        *state.table_sync.capture.lock().unwrap() =
            Some(json!({"content_hash":pending.content_hash}));
        let mut store = Store::default();
        register_table(&mut store, "user", "campaign");
        store
            .tables
            .get_mut(&key("user", "campaign"))
            .unwrap()
            .pending = Some(pending);
        save(&state, &store).await.unwrap();
        tick(&state).await.unwrap();
        let after_recovery = load(&state).await.unwrap();
        let table = &after_recovery.tables[&key("user", "campaign")];
        assert!(table.pending.is_none());
        assert!(queued(table));
        let version = table.previous_version;
        tick(&state).await.unwrap();
        let after_reconcile = load(&state).await.unwrap();
        let table = &after_reconcile.tables[&key("user", "campaign")];
        assert_eq!(table.status, "up_to_date");
        assert_eq!(table.previous_version, version);
        assert!(!queued(table));
        tick(&state).await.unwrap();
        {
            let requests = state.table_sync.requests.lock().unwrap();
            assert_eq!(requests.len(), 3);
            assert!(
                requests
                    .iter()
                    .all(|r| matches!(r["operation"].as_str(), Some("capabilities" | "status")))
            );
        }
        let restarted = ServiceState::new(
            dir.path().into(),
            &[],
            "/test/onecrew".into(),
            "test".into(),
        );
        *restarted.table_sync.capture.lock().unwrap() =
            Some(json!({"content_hash":table.committed_hash}));
        *restarted.table_sync.remote.lock().unwrap() =
            Some(std::collections::VecDeque::from(vec![response(cap)]));
        tick(&restarted).await.unwrap();
        tick(&restarted).await.unwrap();
        assert_eq!(restarted.table_sync.requests.lock().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn unchanged_success_resets_failures_without_upload_or_sequence_change() {
        let dir = tempfile::tempdir().unwrap();
        let state = ServiceState::new(
            dir.path().into(),
            &[],
            "/test/onecrew".into(),
            "test".into(),
        );
        let source = Uuid::new_v4();
        let consent = Uuid::new_v4();
        *state.table_sync.capture.lock().unwrap() = Some(json!({"content_hash":"same"}));
        let mut table = Table {
            campaign_id: "campaign".into(),
            user_id: "user".into(),
            enabled: true,
            next_check: 0,
            committed_hash: Some("same".into()),
            committed_consent: Some(consent),
            committed_source: Some(source),
            failures: 2,
            previous_version: Some(Uuid::new_v4()),
            sequence: 7,
            status: "error".into(),
            error: Some("library_request_unavailable".into()),
            pending: None,
        };
        let previous_version = table.previous_version;
        run_table(&state, &mut table, Uuid::new_v4(), source, consent)
            .await
            .unwrap();
        assert_eq!(table.failures, 0);
        assert_eq!(table.status, "up_to_date");
        assert!(table.error.is_none());
        assert!(table.enabled);
        assert_eq!(table.sequence, 7);
        assert_eq!(table.previous_version, previous_version);
        assert!(table.pending.is_none());
        assert!(state.table_sync.requests.lock().unwrap().is_empty());
        checkpoint(&state, &table).await.unwrap();
        assert_eq!(
            load(&state).await.unwrap().tables[&key("user", "campaign")].failures,
            0
        );
    }
    #[tokio::test]
    async fn local_disable_works_even_when_cloud_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let state = ServiceState::new(
            dir.path().into(),
            &[],
            "/test/onecrew".into(),
            "test".into(),
        );
        *state.table_sync.remote.lock().unwrap() = Some(std::collections::VecDeque::new());
        configure(
            &state,
            ConfigureInput {
                campaign_id: "campaign".into(),
                enabled: false,
            },
        )
        .await
        .unwrap();
        assert!(state.table_sync.requests.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn rate_limit_blocks_replay_but_independent_consent_read_remains_available() {
        let dir = tempfile::tempdir().unwrap();
        let state = ServiceState::new(
            dir.path().into(),
            &[],
            "/test/onecrew".into(),
            "test".into(),
        );
        *state.table_sync.remote.lock().unwrap() = Some(std::collections::VecDeque::from(vec![
            Ok(TableSyncResponse {
                status: 429,
                etag: None,
                retry_after: Some("600".into()),
                body: json!({}),
            }),
            Ok(TableSyncResponse {
                status: 200,
                etag: None,
                retry_after: None,
                body: json!({"consent":null,"consent_etag":null}),
            }),
        ]));
        assert_eq!(
            remote(&state, Request::Capabilities).await.unwrap().status,
            429
        );
        assert_eq!(
            remote(&state, Request::Capabilities).await.err().as_deref(),
            Some("table_sync_backoff")
        );
        assert!(state.table_sync.retry_after.load(Ordering::Acquire) >= now() + 599);
        assert!(remote(&state, Request::Consent).await.is_ok());
        assert_eq!(state.table_sync.requests.lock().unwrap().len(), 2);
    }
    #[tokio::test]
    async fn revoke_does_not_depend_on_capabilities_and_stops_local_sync() {
        let dir = tempfile::tempdir().unwrap();
        let state = ServiceState::new(
            dir.path().into(),
            &[],
            "/test/onecrew".into(),
            "test".into(),
        );
        let response = |body| {
            Ok(TableSyncResponse {
                status: 200,
                etag: None,
                retry_after: None,
                body,
            })
        };
        let operation = Uuid::new_v4();
        *state.table_sync.remote.lock().unwrap() = Some(std::collections::VecDeque::from(vec![
            response(
                json!({"consent":{"identity":{"access_user_id":"user"}},"consent_etag":"etag"}),
            ),
            response(json!({"consent":{"status":"revoked","operation_id":operation}})),
        ]));
        state
            .table_sync
            .retry_after
            .store(now() + 600, Ordering::Release);
        change_consent(
            &state,
            ConsentInput {
                action: "revoke".into(),
                operation_id: operation,
                policy_fingerprint: None,
            },
        )
        .await
        .unwrap();
        let requests = state.table_sync.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0]["operation"], "consent");
        assert_eq!(requests[1]["operation"], "set_consent");
    }
}
