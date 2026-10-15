//! Supervised, explicitly started discovery. Browser lifetime is not execution authority.
mod pipeline;
mod saved_scores;
use crate::{ServiceState, discovery_workspace::WorkspaceStage};
use futures::FutureExt;
use onecrew_creator_runtime::{DiscoveryReviewMaterialInput, DiscoveryStep};
use serde::{Deserialize, Serialize};
use std::{
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct ExecutionInput {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub profile_ref: String,
    #[serde(default)]
    pub excluded_platforms: Vec<String>,
}

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct ExecutionStatus {
    pub campaign_id: String,
    pub revision: u32,
    pub stage: WorkspaceStage,
    pub updated_at: u64,
    pub used: u32,
    pub reason: Option<String>,
    pub failure_phase: Option<String>,
    pub failure_code: Option<String>,
    pub sync_error: Option<String>,
    pub synced_at: Option<u64>,
    pub deferred_scores: u32,
    pub sync_pending: bool,
    pub excluded_platforms: Vec<String>,
    pub deferred_hits: Vec<DeferredScore>,
    pub library: Option<ExecutionLibrary>,
}

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct DeferredScore {
    pub search_hit_id: String,
    pub reason: String,
}
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct ExecutionLibrary {
    pub campaign_id: String,
    pub table_id: String,
    pub web_url: String,
    pub total_hits: usize,
    pub inserted_rows: usize,
}

struct Active {
    credential: String,
    status: ExecutionStatus,
    cancel: CancellationToken,
}
#[derive(Default)]
struct Inner {
    active: Option<Active>,
    tasks: Vec<JoinHandle<()>>,
}
#[derive(Default)]
pub(crate) struct DiscoveryExecutionSupervisor {
    inner: Mutex<Inner>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn executing(stage: WorkspaceStage) -> bool {
    matches!(
        stage,
        WorkspaceStage::Searching | WorkspaceStage::Verifying | WorkspaceStage::Pausing
    )
}

impl DiscoveryExecutionSupervisor {
    pub(crate) fn blocks_backend_sync(&self) -> bool {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner
            .active
            .as_ref()
            .is_some_and(|run| executing(run.status.stage))
            || inner.tasks.iter().any(|task| !task.is_finished())
    }
    async fn reap(&self) -> Result<(), String> {
        let finished = {
            let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            let (finished, pending): (Vec<JoinHandle<()>>, Vec<JoinHandle<()>>) =
                std::mem::take(&mut inner.tasks)
                    .into_iter()
                    .partition(|task| task.is_finished());
            inner.tasks = pending;
            finished
        };
        for task in finished {
            task.await.map_err(|_| "discovery_execution_join_failed")?;
        }
        Ok(())
    }
    pub(crate) async fn shutdown(&self) -> Result<(), String> {
        self.cancel();
        let tasks = std::mem::take(&mut self.inner.lock().unwrap_or_else(|e| e.into_inner()).tasks);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        for mut task in tasks {
            match tokio::time::timeout_at(deadline, &mut task).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) => return Err("discovery_execution_join_failed".into()),
                Err(_) => {
                    task.abort();
                    drop(task.await);
                    return Err("discovery_execution_shutdown_timeout".into());
                }
            }
        }
        Ok(())
    }
    pub(crate) fn snapshot(&self, credential: &str) -> Option<ExecutionStatus> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .active
            .as_ref()
            .filter(|run| run.credential == credential)
            .map(|run| run.status.clone())
    }
    pub(crate) fn record_sync(
        &self,
        credential: &str,
        campaign: &str,
        revision: u32,
        result: &Result<onecrew_local_runtime::DiscoveryLibrarySyncResult, String>,
    ) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let Some(run) = inner.active.as_mut().filter(|r| {
            r.credential == credential
                && r.status.campaign_id == campaign
                && r.status.revision == revision
        }) else {
            return;
        };
        match result {
            Ok(r) => {
                run.status.sync_pending = false;
                run.status.sync_error = None;
                run.status.synced_at = Some(now());
                run.status.library = Some(ExecutionLibrary {
                    campaign_id: r.campaign_id.clone(),
                    table_id: r.table_id.clone(),
                    web_url: r.web_url.clone(),
                    total_hits: r.total_hits,
                    inserted_rows: r.inserted_rows,
                });
            }
            Err(code) => {
                run.status.sync_pending = true;
                run.status.sync_error = Some(safe_failure::<()>(&Ok(Err(code.clone()))));
            }
        }
        run.status.updated_at = now();
    }
    pub(crate) fn resolve_score(&self, credential: &str, campaign: &str, revision: u32, hit: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(run) = inner.active.as_mut().filter(|r| {
            r.credential == credential
                && r.status.campaign_id == campaign
                && r.status.revision == revision
        }) {
            run.status.deferred_hits.retain(|d| d.search_hit_id != hit);
            run.status.deferred_scores = run.status.deferred_hits.len() as u32;
            run.status.sync_pending = true;
            run.status.updated_at = now();
        }
    }
    fn change(&self, edit: impl FnOnce(&mut ExecutionStatus)) {
        if let Some(run) = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .active
            .as_mut()
        {
            edit(&mut run.status);
            if run.cancel.is_cancelled() && executing(run.status.stage) {
                run.status.stage = WorkspaceStage::Pausing;
            }
            run.status.updated_at = now();
        }
    }
    pub(crate) fn mark_sync_pending(&self, credential: &str, campaign: &str, revision: u32) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(run) = inner.active.as_mut().filter(|run| {
            run.credential == credential
                && run.status.campaign_id == campaign
                && run.status.revision == revision
        }) {
            run.status.sync_pending = true;
        }
    }
    pub(crate) fn cancel(&self) {
        self.cancel_for("user_paused");
    }
    pub(crate) fn cancel_for(&self, reason: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(run) = inner
            .active
            .as_mut()
            .filter(|run| executing(run.status.stage))
        {
            if !run.cancel.is_cancelled() {
                run.status.reason = Some(reason.into());
            }
            run.cancel.cancel();
            run.status.stage = WorkspaceStage::Pausing;
            run.status.updated_at = now();
        }
    }
    fn admit(
        &self,
        input: &ExecutionInput,
        credential: String,
        used: u32,
    ) -> Result<CancellationToken, String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "discovery_execution_unavailable")?;
        if let Some(run) = inner
            .active
            .as_ref()
            .filter(|run| executing(run.status.stage))
        {
            if run.credential == credential
                && run.status.campaign_id == input.campaign_id
                && run.status.revision == input.expected_revision
            {
                return Err("discovery_execution_already_running".into());
            }
            return Err("discovery_execution_busy".into());
        }
        if inner.tasks.iter().any(|task| !task.is_finished()) {
            return Err("discovery_execution_busy".into());
        }
        let cancel = CancellationToken::new();
        let previous = inner
            .active
            .as_ref()
            .filter(|r| {
                r.credential == credential
                    && r.status.campaign_id == input.campaign_id
                    && r.status.revision == input.expected_revision
            })
            .map(|r| r.status.clone());
        inner.active = Some(Active {
            credential,
            cancel: cancel.clone(),
            status: ExecutionStatus {
                campaign_id: input.campaign_id.clone(),
                revision: input.expected_revision,
                stage: WorkspaceStage::Searching,
                updated_at: now(),
                used,
                reason: None,
                failure_phase: None,
                failure_code: None,
                sync_error: previous.as_ref().and_then(|s| s.sync_error.clone()),
                synced_at: previous.as_ref().and_then(|s| s.synced_at),
                deferred_scores: previous.as_ref().map_or(0, |s| s.deferred_scores),
                sync_pending: previous.as_ref().is_some_and(|s| s.sync_pending),
                excluded_platforms: input.excluded_platforms.clone(),
                deferred_hits: previous
                    .as_ref()
                    .map_or_else(Vec::new, |s| s.deferred_hits.clone()),
                library: previous.and_then(|s| s.library),
            },
        });
        Ok(cancel)
    }
}

pub(crate) async fn account(state: &ServiceState) -> Result<String, String> {
    let login = state.validate_login().await?;
    if login.status.state != onecrew_creator_sync::CreatorSyncAdmissionState::Active {
        return Err("discovery_execution_login_required".into());
    }
    login
        .status
        .credential_id
        .ok_or_else(|| "discovery_execution_login_required".into())
}

pub(crate) async fn get(state: &ServiceState) -> Result<Option<ExecutionStatus>, String> {
    Ok(state.discovery_execution.snapshot(&account(state).await?))
}
pub(crate) async fn pause(
    state: &ServiceState,
    campaign_id: String,
) -> Result<Option<ExecutionStatus>, String> {
    let credential = account(state).await?;
    let run = state.discovery_execution.snapshot(&credential);
    if run
        .as_ref()
        .is_none_or(|run| run.campaign_id != campaign_id)
    {
        return Err("discovery_execution_campaign_changed".into());
    }
    state.discovery_execution.cancel();
    Ok(state.discovery_execution.snapshot(&credential))
}

pub(crate) async fn start(
    state: &Arc<ServiceState>,
    input: ExecutionInput,
) -> Result<ExecutionStatus, String> {
    if input.profile_ref.is_empty()
        || input.excluded_platforms.len() > 64
        || input.excluded_platforms.iter().any(|s| {
            s.len() > 80
                || s.is_empty()
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
    {
        return Err("discovery_execution_exclusions_invalid".into());
    }
    let credential = account(state).await?;
    let campaign = state
        .platform()
        .await?
        .creators()?
        .get_discovery_campaign_async(input.campaign_id.clone())
        .await
        .map_err(|_| "discovery_execution_campaign_unavailable")?;
    if campaign.phase != "active"
        || campaign.revision != input.expected_revision
        || campaign
            .scopes
            .iter()
            .any(|scope| scope.profile_ref != input.profile_ref)
    {
        return Err("discovery_execution_approval_changed".into());
    }
    if account(state).await? != credential {
        return Err("discovery_execution_account_changed".into());
    }
    state.platform().await?.discovery_library_readiness().await?;
    state.discovery_execution.reap().await?;
    let sync_admission = state.table_sync.lock.lock().await;
    if account(state).await? != credential {
        return Err("discovery_execution_account_changed".into());
    }
    let cancel =
        match state
            .discovery_execution
            .admit(&input, credential.clone(), campaign.used_calls)
        {
            Ok(cancel) => cancel,
            Err(code) if code == "discovery_execution_already_running" => {
                return state
                    .discovery_execution
                    .snapshot(&credential)
                    .ok_or_else(|| "discovery_execution_unavailable".into());
            }
            Err(code) => return Err(code),
        };
    drop(sync_admission);
    state
        .workspace_card_requests
        .started(&credential, &input.campaign_id);
    let owner = state.clone();
    let task = tokio::spawn(async move {
        let campaign_id = input.campaign_id.clone();
        let result = AssertUnwindSafe(run(&owner, input, &credential, cancel))
            .catch_unwind()
            .await;
        if result.is_err() {
            owner.discovery_execution.change(|s| {
                s.stage = WorkspaceStage::Error;
                s.reason = Some("worker_panicked".into());
                s.failure_phase = Some("execution".into());
            });
        }
        if crate::table_sync::search_settled(&owner, &campaign_id, &credential)
            .await
            .is_err()
        {
            log::warn!("backend table sync could not be queued after search");
        }
    });
    state
        .discovery_execution
        .inner
        .lock()
        .map_err(|_| "discovery_execution_unavailable")?
        .tasks
        .push(task);
    get(state)
        .await?
        .ok_or_else(|| "discovery_execution_unavailable".into())
}

fn deferred(code: &str) -> bool {
    // These errors quarantine only this candidate. The scoring owner reads
    // back its stable request receipt before returning uncertainty; retaining
    // it here grants neither a retry nor admission into the shortlist.
    matches!(
        code,
        "score_queued"
            | "review_submit_outcome_unknown"
            | "review_read_unavailable"
            | "initial_match_material_unavailable"
            | "initial_match_busy"
            | "initial_match_discovery_draft_codex_unavailable"
            | "initial_match_discovery_draft_codex_login_required"
            | "initial_match_discovery_draft_codex_failed"
            | "initial_match_discovery_draft_codex_timeout"
            | "initial_match_discovery_draft_plan_invalid"
            | "initial_match_single_finding_required"
            | "initial_match_finding_invalid"
            | "initial_match_language_request_invalid"
            | "initial_match_required_failure_must_be_low"
            | "initial_match_required_count_unknown"
            | "initial_match_author_evidence_required"
            | "initial_match_content_language_unavailable"
            | "initial_match_audience_language_invalid"
            | "initial_match_audience_sample_insufficient"
            | "review_material_changed"
            | "review_input_too_large"
            | "review_input_invalid"
            | "review_conditions_mismatch"
            | "review_citation_invalid"
            | "review_first_hand_evidence_required"
            | "review_transcript_required"
            | "review_match_level_invalid"
    )
}

async fn owned_read<T: Send + 'static>(
    work: &Arc<crate::work::WorkOwner>,
    cancel: CancellationToken,
    read: impl std::future::Future<Output = Result<T, String>> + Send + 'static,
) -> Result<Result<T, String>, String> {
    work.run(async move {
        if cancel.is_cancelled() {
            return Err("discovery_execution_paused".into());
        }
        read.await
    })
    .await
}

// Two model jobs may run while up to three prepared authors wait. Provider
// reads remain serial and keep their durable call reservation semantics.
struct ScoreJob {
    hit: String,
    deferred_platforms: Vec<String>,
    task: JoinHandle<Result<Result<bool, String>, String>>,
}

#[derive(Default)]
struct SyncLane(pipeline::LatestWork);
impl SyncLane {
    fn request(&mut self, state: &Arc<ServiceState>, input: &ExecutionInput, credential: &str) {
        let (state, input, credential) = (state.clone(), input.clone(), credential.to_owned());
        self.0
            .request(async move { sync_saved(&state, &input, &credential).await });
    }
    async fn reap(&mut self) {
        self.0.reap().await;
    }
    async fn drain(&mut self) {
        self.0.drain().await;
    }
}

async fn publish_changed(
    state: &Arc<ServiceState>,
    input: &ExecutionInput,
    credential: &str,
    requested: &mut Option<String>,
    sync: &mut SyncLane,
) {
    let current = recommendation_revision(state, &input.campaign_id).await;
    if should_sync(true, requested.as_deref(), current.as_deref()) {
        record_duration(state, &input.campaign_id, "recommendation.changed", 0);
        if let Ok(platform) = state.platform().await
            && let Ok(creators) = platform.creators()
            && let Ok(counts) = creators
                .count_discovery_candidates_async(input.campaign_id.clone())
                .await
            && counts.qualified > 0
        {
            record_duration(state, &input.campaign_id, "recommendation.available", 0);
        }
        *requested = current;
        sync.request(state, input, credential);
    }
}

async fn finish_score(
    job: ScoreJob,
    state: &Arc<ServiceState>,
    input: &ExecutionInput,
    credential: &str,
    requested: &mut Option<String>,
    sync: &mut SyncLane,
) -> bool {
    if !job.task.is_finished() {
        state.discovery_execution.change(|s| {
            if s.stage == WorkspaceStage::Searching {
                s.stage = WorkspaceStage::Verifying;
            }
        });
    }
    let waiting = std::time::Instant::now();
    let score = job
        .task
        .await
        .unwrap_or_else(|_| Err("host_work_failed".into()));
    record_timing(state, &input.campaign_id, "pipeline.score_wait", waiting);
    complete_score(job.hit, score, state, input, credential, requested, sync).await
}

async fn complete_score(
    hit: String,
    score: Result<Result<bool, String>, String>,
    state: &Arc<ServiceState>,
    input: &ExecutionInput,
    credential: &str,
    requested: &mut Option<String>,
    sync: &mut SyncLane,
) -> bool {
    let saved = matches!(score, Ok(Ok(true)));
    let proceed = record_score_outcome(&state.discovery_execution, hit, score);
    if saved {
        publish_changed(state, input, credential, requested, sync).await;
    }
    proceed
}

async fn finish_next_score(
    jobs: &mut Vec<ScoreJob>,
    state: &Arc<ServiceState>,
    input: &ExecutionInput,
    credential: &str,
    requested: &mut Option<String>,
    sync: &mut SyncLane,
) -> bool {
    let waiting = std::time::Instant::now();
    let (score, index, _) =
        futures::future::select_all(jobs.iter_mut().map(|job| &mut job.task)).await;
    record_timing(state, &input.campaign_id, "pipeline.score_wait", waiting);
    let job = jobs.remove(index);
    complete_score(
        job.hit,
        score.unwrap_or_else(|_| Err("host_work_failed".into())),
        state,
        input,
        credential,
        requested,
        sync,
    )
    .await
}

fn scoring_platforms(result: &onecrew_local_runtime::OpenCliDiscoveryAdvanceResult) -> Vec<String> {
    let platform = result
        .profile
        .as_ref()
        .map(|r| r.claim.platform.as_str())
        .or_else(|| result.content.as_ref().map(|r| r.claim.platform.as_str()))
        .or_else(|| {
            matches!(
                result.step,
                DiscoveryStep::ReadVideo { .. } | DiscoveryStep::ReadTranscript { .. }
            )
            .then_some("youtube")
        });
    // Cross-platform web sources can bind the same native author. Conservatively
    // hold those routes too; unknown composite material holds all scopes.
    let mut deferred = platform
        .filter(|p| !matches!(*p, "google" | "yahoo"))
        .map(|p| vec![p.to_owned()])
        .unwrap_or_else(|| {
            result
                .campaign
                .scopes
                .iter()
                .map(|s| s.platform.clone())
                .collect()
        });
    for origin in ["google", "yahoo"] {
        if !deferred.iter().any(|p| p == origin) {
            deferred.push(origin.into());
        }
    }
    deferred
}

#[allow(
    clippy::too_many_arguments,
    reason = "Keeps score queue ownership and cancellation explicit at this single scheduling boundary"
)]
async fn enqueue_score(
    state: &Arc<ServiceState>,
    input: &ExecutionInput,
    credential: &str,
    cancel: &CancellationToken,
    hit: String,
    deferred_platforms: Vec<String>,
    score_jobs: &mut Vec<ScoreJob>,
    score_slots: &pipeline::ScoreSlots,
    saved_scores: &Arc<saved_scores::SavedScores>,
    attempted_materials: &mut std::collections::HashSet<String>,
    published_recommendations: &mut Option<String>,
    sync_lane: &mut SyncLane,
) -> bool {
    if score_jobs.iter().any(|job| job.hit == hit) {
        return true;
    }
    if saved_scores
        .add(saved_scores::SavedScore {
            hit: hit.clone(),
            deferred_platforms: deferred_platforms.clone(),
        })
        .is_err()
    {
        return record_score_outcome::<()>(
            &state.discovery_execution,
            hit,
            Err("score_queue_unavailable".into()),
        );
    }
    let stop_queued_scores = cancel;
    let owner = state.clone();
    let review = DiscoveryReviewMaterialInput {
        campaign_id: input.campaign_id.clone(),
        expected_revision: input.expected_revision,
        search_hit_id: hit.clone(),
    };
    let campaign = input.campaign_id.clone();
    let prepared_started = std::time::Instant::now();
    let worker = owner.clone();
    let preparation_metrics = owner.clone();
    let preparation_campaign = campaign.clone();
    let preparation_target = hit.clone();
    let preparation = owner
        .work
        .run(async move {
            worker
                .platform()
                .await?
                .prepare_discovery_match_observed(review, move |phase, elapsed| {
                    crate::discovery_metrics::enqueue(
                        &preparation_metrics.data_dir,
                        &preparation_campaign,
                        phase,
                        elapsed,
                        Some(&preparation_target),
                    );
                })
                .await
        })
        .await;
    record_timing(&owner, &campaign, "score.prepare", prepared_started);
    match preparation {
        Ok(Ok(onecrew_local_runtime::DiscoveryMatchPreparation::Model(prepared))) => {
            if cancel.is_cancelled() || state.shutdown.is_cancelled() {
                record_timing(&owner, &campaign, "score.skip", prepared_started);
                record_timing(&owner, &campaign, "score", prepared_started);
                return true;
            }
            if !attempted_materials.insert(prepared.evidence_key().to_owned()) {
                if saved_scores.remove(&hit).is_err() {
                    return record_score_outcome::<()>(
                        &state.discovery_execution,
                        hit,
                        Err("score_queue_unavailable".into()),
                    );
                }
                // A failed model attempt must not be repeated in the same run
                // without new evidence. Retain its deferred/error receipt.
                record_timing(&owner, &campaign, "score.skip", prepared_started);
                record_timing(&owner, &campaign, "score", prepared_started);
                return true;
            }
            let saved_scores = saved_scores.clone();
            let saved_hit = hit.clone();
            let slots = score_slots.clone();
            let queued_cancel = stop_queued_scores.clone();
            score_jobs.push(ScoreJob {
                hit,
                deferred_platforms: deferred_platforms.clone(),
                task: tokio::spawn(async move {
                    let queued_at = std::time::Instant::now();
                    let permit = slots.acquire(&queued_cancel, &owner.shutdown).await;
                    record_timing(&owner, &campaign, "score.queue_wait", queued_at);
                    let Some(_permit) = permit else {
                        record_timing(&owner, &campaign, "score.cancelled", queued_at);
                        return Ok(Err("score_queued".into()));
                    };
                    // Remove before model admission. A crash after this point must
                    // never automatically replay an uncertain model request.
                    if saved_scores.remove(&saved_hit).is_err() {
                        return Err("score_queue_unavailable".into());
                    }
                    let started = prepared_started;
                    let admission_started = std::time::Instant::now();
                    let worker = owner.clone();
                    let metrics = owner.clone();
                    let metrics_campaign = campaign.clone();
                    let outcome = owner
                        .work
                        .run(async move {
                            record_timing(
                                &worker,
                                &metrics_campaign,
                                "score.admission_wait",
                                admission_started,
                            );
                            worker
                                .platform()
                                .await?
                                .evaluate_prepared_discovery_match(
                                    *prepared,
                                    move |phase, elapsed| {
                                        crate::discovery_metrics::enqueue(
                                            &metrics.data_dir,
                                            &metrics_campaign,
                                            phase,
                                            elapsed,
                                            Some(&saved_hit),
                                        );
                                    },
                                )
                                .await
                                .map(|r| r.is_some())
                        })
                        .await;
                    record_timing(&owner, &campaign, "score", started);
                    outcome
                }),
            });
        }
        Ok(Ok(prepared)) => {
            if saved_scores.remove(&hit).is_err() {
                return record_score_outcome::<()>(
                    &state.discovery_execution,
                    hit,
                    Err("score_queue_unavailable".into()),
                );
            }
            let cached = matches!(
                prepared,
                onecrew_local_runtime::DiscoveryMatchPreparation::Cached(_)
            );
            record_timing(
                &owner,
                &campaign,
                if cached { "score.cache" } else { "score.skip" },
                prepared_started,
            );
            record_timing(&owner, &campaign, "score", prepared_started);
            state.discovery_execution.resolve_score(
                credential,
                &input.campaign_id,
                input.expected_revision,
                &hit,
            );
            record_score_outcome(&state.discovery_execution, hit, Ok(Ok(cached)));
            if cached {
                publish_changed(
                    state,
                    input,
                    credential,
                    published_recommendations,
                    sync_lane,
                )
                .await;
            }
        }
        failure => {
            record_timing(&owner, &campaign, "score", prepared_started);
            if !record_score_outcome(&state.discovery_execution, hit, failure) {
                return false;
            }
        }
    }
    true
}

async fn run(
    state: &Arc<ServiceState>,
    input: ExecutionInput,
    credential: &str,
    cancel: CancellationToken,
) {
    let run_started = std::time::Instant::now();
    let mut used = state
        .discovery_execution
        .snapshot(credential)
        .map_or(0, |s| s.used);
    let mut published_recommendations = recommendation_revision(state, &input.campaign_id).await;
    let mut score_jobs: Vec<ScoreJob> = Vec::new();
    let score_slots = pipeline::ScoreSlots::default();
    let stop_queued_scores = cancel.child_token();
    let saved_scores = Arc::new(saved_scores::SavedScores::new(
        &state.data_dir,
        credential,
        &input.campaign_id,
        input.expected_revision,
    ));
    let mut restored = match saved_scores.load() {
        Ok(rows) => std::collections::VecDeque::from(rows),
        Err(_) => {
            state.discovery_execution.change(|s| {
                s.stage = WorkspaceStage::Error;
                s.reason = Some("request_failed".into());
                s.failure_phase = Some("score".into());
                s.failure_code = Some("score_queue_unavailable".into());
            });
            return;
        }
    };
    let mut read_ahead = 0usize;
    let mut attempted_materials = std::collections::HashSet::new();
    let mut sync_lane = SyncLane::default();
    record_duration(state, &input.campaign_id, "pipeline.started", 0);
    loop {
        flush_metrics().await;
        let active_scores = score_slots.active();
        record_duration(
            state,
            &input.campaign_id,
            &format!("pipeline.active.{active_scores}"),
            0,
        );
        let pending_scores = score_jobs
            .iter()
            .filter(|job| !job.task.is_finished())
            .count();
        record_duration(
            state,
            &input.campaign_id,
            &format!(
                "pipeline.buffer.{}",
                pending_scores.saturating_sub(active_scores)
            ),
            0,
        );
        sync_lane.reap().await;
        if score_jobs.iter().any(|job| job.task.is_finished()) {
            if !finish_next_score(
                &mut score_jobs,
                state,
                &input,
                credential,
                &mut published_recommendations,
                &mut sync_lane,
            )
            .await
            {
                break;
            }
            read_ahead = 0;
            continue;
        }

        if cancel.is_cancelled() || state.shutdown.is_cancelled() {
            state.discovery_execution.change(|s| {
                s.stage = WorkspaceStage::Paused;
                if s.reason.is_none() {
                    s.reason = Some("execution_interrupted".into());
                }
            });
            break;
        }
        if account(state).await.as_deref() != Ok(credential) {
            state.discovery_execution.change(|s| {
                s.stage = WorkspaceStage::Error;
                s.reason = Some("account_changed".into());
                s.failure_phase = Some("admission".into());
            });
            break;
        }
        if !score_jobs.is_empty() || !restored.is_empty() {
            let remaining = async {
                state
                    .platform()
                    .await?
                    .creators()
                    .map_err(|e| e.to_string())?
                    .discovery_remaining_slots_async(
                        input.campaign_id.clone(),
                        input.excluded_platforms.clone(),
                    )
                    .await
                    .map_err(|e| e.to_string())
            }
            .await;
            // A failed capacity read permits no additional provider dispatch.
            let capacity = match remaining {
                Ok(remaining) => pipeline::score_capacity(remaining),
                Err(_) => {
                    state.discovery_execution.change(|s| {
                        s.stage = WorkspaceStage::Error;
                        s.reason = Some("request_failed".into());
                        s.failure_phase = Some("score".into());
                        s.failure_code = Some("initial_match_campaign_unavailable".into());
                    });
                    break;
                }
            };
            if capacity == 0 && score_jobs.is_empty() {
                // Already at target: leave saved evidence for a later target revision.
                restored.clear();
            }
            if !score_jobs.is_empty()
                && (score_jobs.len() >= capacity || read_ahead >= pipeline::READ_AHEAD)
            {
                if !finish_next_score(
                    &mut score_jobs,
                    state,
                    &input,
                    credential,
                    &mut published_recommendations,
                    &mut sync_lane,
                )
                .await
                {
                    break;
                }
                read_ahead = 0;
                continue;
            }
        }
        if let Some(saved) = restored.pop_front() {
            if !enqueue_score(
                state,
                &input,
                credential,
                &stop_queued_scores,
                saved.hit,
                saved.deferred_platforms,
                &mut score_jobs,
                &score_slots,
                &saved_scores,
                &mut attempted_materials,
                &mut published_recommendations,
                &mut sync_lane,
            )
            .await
            {
                break;
            }
            continue;
        }
        state
            .discovery_execution
            .change(|s| s.stage = WorkspaceStage::Searching);
        let owner = state.clone();
        let next = onecrew_local_runtime::OpenCliDiscoveryAdvanceInput {
            campaign_id: input.campaign_id.clone(),
            expected_revision: input.expected_revision,
            excluded_platforms: input.excluded_platforms.clone(),
        };
        let deferred_platforms = score_jobs
            .iter()
            .flat_map(|job| job.deferred_platforms.clone())
            .collect();
        let scoring_hits = score_jobs.iter().map(|job| job.hit.clone()).collect();
        let read_started = std::time::Instant::now();
        let result = owned_read(&state.work, cancel.clone(), async move {
            crate::opencli_discovery::advance_scoring(
                &owner,
                next,
                deferred_platforms,
                scoring_hits,
            )
            .await
        })
        .await;
        let result = match result {
            Ok(Ok(result)) => result,
            Err(code) if code == "host_busy" && cancel.is_cancelled() => {
                state
                    .discovery_execution
                    .change(|s| s.stage = WorkspaceStage::Paused);
                break;
            }
            Ok(Err(code)) if code == "discovery_execution_paused" => {
                state
                    .discovery_execution
                    .change(|s| s.stage = WorkspaceStage::Paused);
                break;
            }
            failure => {
                record_timing(state, &input.campaign_id, "read", read_started);
                // A provider write/read claim may already be durable. Never repeat an unknown step.
                state.discovery_execution.change(|s| {
                    s.stage = WorkspaceStage::Error;
                    s.reason = Some("request_failed".into());
                    s.failure_phase = Some("read".into());
                    s.failure_code = Some(safe_failure(&failure));
                });
                break;
            }
        };
        let failure_code = result
            .provider_stop_code
            .as_deref()
            .or_else(|| result.profile.as_ref().and_then(|r| r.stop_code.as_deref()))
            .or_else(|| {
                result
                    .facebook_identity
                    .as_ref()
                    .and_then(|r| r.stop_code.as_deref())
            })
            .or_else(|| {
                result
                    .source_author
                    .as_ref()
                    .and_then(|r| r.stop_code.as_deref())
            })
            .or_else(|| {
                result
                    .web_source
                    .as_ref()
                    .and_then(|r| r.stop_code.as_deref())
            })
            .or_else(|| result.content.as_ref().and_then(|r| r.stop_code.as_deref()))
            .or_else(|| {
                result
                    .identity
                    .as_ref()
                    .and_then(|r| r.stop_code.as_deref())
            })
            .or_else(|| {
                result
                    .transcript
                    .as_ref()
                    .and_then(|r| r.stop_code.as_deref())
            })
            .or_else(|| {
                result
                    .public_page
                    .as_ref()
                    .and_then(|r| r.stop_code.as_deref())
            })
            .or_else(|| {
                result
                    .contact_lookup
                    .as_ref()
                    .and_then(|r| r.stop_code.as_deref())
            });
        if let Some(code) = failure_code {
            let platform = result
                .profile
                .as_ref()
                .map(|r| r.claim.platform.as_str())
                .or_else(|| result.facebook_identity.as_ref().map(|_| "facebook"))
                .or_else(|| {
                    result
                        .web_source
                        .as_ref()
                        .map(|r| r.claim.target_platform.as_str())
                })
                .or_else(|| result.content.as_ref().map(|r| r.claim.target_platform()));
            record_read_failure(
                state,
                &input.campaign_id,
                Some(&result.step),
                code,
                Some((result.campaign.used_calls, platform)),
            );
        }
        let detail = match &result.step {
            DiscoveryStep::ReadPublicPage { .. } => Some("read.public_page"),
            DiscoveryStep::ReadContactLookup { lookup } if lookup.method == "maigret" => {
                Some("read.maigret")
            }
            DiscoveryStep::ReadContactLookup { .. } => Some("read.contact_search"),
            DiscoveryStep::ReadVideo { .. } => Some("read.video"),
            DiscoveryStep::ReadFacebookIdentity { .. } => Some("read.facebook_identity"),
            DiscoveryStep::ReadSourceAuthor { .. } => Some("read.source_author"),
            DiscoveryStep::ReadWebSource { .. } => Some("read.web_source"),
            DiscoveryStep::Search { .. } => Some("read.platform_search"),
            DiscoveryStep::ReadContent { .. }
                if result.content.as_ref().is_some_and(|r| {
                    r.language_sample.is_some()
                        || matches!(r.claim.platform.as_str(), "google" | "yahoo")
                }) =>
            {
                Some("read.language")
            }
            DiscoveryStep::ReadProfile { .. } => Some("read.profile"),
            DiscoveryStep::ReadContent { .. } => Some("read.content"),
            DiscoveryStep::ReadIdentity { .. } => Some("read.identity"),
            DiscoveryStep::ReadTranscript { .. } => Some("read.transcript"),
            DiscoveryStep::ReadContentTargets { .. } => Some("read.content_targets"),
            _ => None,
        };
        if let Some(detail) = detail {
            let target = match &result.step {
                DiscoveryStep::ReadProfile { search_hit_id }
                | DiscoveryStep::ReadContent { search_hit_id }
                | DiscoveryStep::ReadSourceAuthor { search_hit_id }
                | DiscoveryStep::ReadWebSource { search_hit_id }
                | DiscoveryStep::ReadIdentity { search_hit_id }
                | DiscoveryStep::ReadTranscript { search_hit_id, .. }
                | DiscoveryStep::ReadContentTargets { search_hit_id } => {
                    Some(search_hit_id.as_str())
                }
                _ => None,
            };
            crate::discovery_metrics::enqueue(
                &state.data_dir,
                &input.campaign_id,
                detail,
                read_started.elapsed().as_millis() as u64,
                target,
            );
        }
        // Components belong to the composite source-author operation; never
        // add them to read/source_author totals when reporting overall time.
        if let Some(timings) = &result.source_author_timings {
            if let Some(elapsed) = timings.content_ms {
                record_duration(
                    state,
                    &input.campaign_id,
                    "read.source_author.content",
                    elapsed,
                );
            }
            if let Some(elapsed) = timings.profile_ms {
                record_duration(
                    state,
                    &input.campaign_id,
                    "read.source_author.profile",
                    elapsed,
                );
            }
        }
        let advancing = matches!(
            result.step,
            DiscoveryStep::Search { .. }
                | DiscoveryStep::ReadVideo { .. }
                | DiscoveryStep::ReadTranscript { .. }
                | DiscoveryStep::ReadProfile { .. }
                | DiscoveryStep::ReadContent { .. }
                | DiscoveryStep::ReadContentTargets { .. }
                | DiscoveryStep::ReadIdentity { .. }
                | DiscoveryStep::ReadFacebookIdentity { .. }
                | DiscoveryStep::ReadSourceAuthor { .. }
                | DiscoveryStep::ReadWebSource { .. }
                | DiscoveryStep::ReadPublicPage { .. }
                | DiscoveryStep::ReadContactLookup { .. }
        );
        if advancing {
            record_timing(state, &input.campaign_id, "read", read_started);
        }
        state.discovery_execution.change(|s| {
            s.used = result.campaign.used_calls;
            // A saved provider result can precede its Library projection. Do not
            // present a previous successful sync as proof that this result is visible.
            s.sync_pending = true;
        });
        let waited_for_score = !score_jobs.is_empty();
        if waited_for_score && advancing {
            read_ahead += 1;
        }
        let hit = if result.provider_stop_code.is_some() {
            None
        } else {
            match &result.step {
                DiscoveryStep::ReadVideo { search_hit_id, .. } => Some(search_hit_id.clone()),
                DiscoveryStep::ReadWebSource { search_hit_id }
                    if result.web_source.as_ref().is_some_and(|r| {
                        r.status == "completed" && r.is_primary_source && r.candidate_id.is_some()
                    }) =>
                {
                    Some(search_hit_id.clone())
                }
                DiscoveryStep::ReadSourceAuthor { search_hit_id }
                    if result
                        .source_author
                        .as_ref()
                        .is_some_and(|r| r.status == "completed" && r.duplicate_of.is_none()) =>
                {
                    Some(search_hit_id.clone())
                }
                DiscoveryStep::ReadProfile { search_hit_id }
                    if result
                        .profile
                        .as_ref()
                        .is_some_and(|r| r.status == "completed") =>
                {
                    Some(search_hit_id.clone())
                }
                DiscoveryStep::ReadContent { search_hit_id }
                    if result
                        .content
                        .as_ref()
                        .is_some_and(|r| r.status == "completed") =>
                {
                    Some(search_hit_id.clone())
                }
                DiscoveryStep::ReadTranscript { search_hit_id, .. }
                    if result.transcript.as_ref().is_some_and(|r| {
                        transcript_ready_for_review(&result.campaign, search_hit_id, r)
                    }) =>
                {
                    Some(search_hit_id.clone())
                }
                DiscoveryStep::ReadPublicPage { .. }
                    if result
                        .public_page
                        .as_ref()
                        .is_some_and(|r| r.status == "completed") =>
                {
                    result.public_page.as_ref().map(|r| r.search_hit_id.clone())
                }
                _ => None,
            }
        };
        // Contact reads follow an audited qualification. They publish contact
        // evidence through their owner and do not need another model score.
        let contact_only = if let Some(page) = &result.public_page {
            completed_contact_author(state, &input.campaign_id, &page.candidate_id).await
        } else {
            false
        };
        let hit = hit.filter(|_| !contact_only);
        if let Some(hit) = hit.filter(|_| !cancel.is_cancelled())
            && !cancel.is_cancelled()
            && !state.shutdown.is_cancelled()
        {
            let prior_jobs = score_jobs.len();
            if !enqueue_score(
                state,
                &input,
                credential,
                &stop_queued_scores,
                hit,
                scoring_platforms(&result),
                &mut score_jobs,
                &score_slots,
                &saved_scores,
                &mut attempted_materials,
                &mut published_recommendations,
                &mut sync_lane,
            )
            .await
            {
                break;
            }
            if prior_jobs == 1 && score_jobs.len() == 2 {
                read_ahead = 0;
            }
        }
        if result.public_page.is_some() {
            publish_changed(
                state,
                &input,
                credential,
                &mut published_recommendations,
                &mut sync_lane,
            )
            .await;
        }
        // Temporary exclusions or exhausted reads are not final while a score
        // can unlock contacts or change the target count. Replan after draining.
        if !advancing && waited_for_score {
            if !finish_next_score(
                &mut score_jobs,
                state,
                &input,
                credential,
                &mut published_recommendations,
                &mut sync_lane,
            )
            .await
            {
                break;
            }
            read_ahead = 0;
            continue;
        }
        if !advancing {
            if matches!(
                result.step,
                DiscoveryStep::SearchExhausted | DiscoveryStep::NeedsEvidence
            ) {
                let expansion = async {
                    state
                        .platform()
                        .await?
                        .expand_discovery_queries(
                            input.campaign_id.clone(),
                            input.expected_revision,
                            input.excluded_platforms.clone(),
                        )
                        .await
                };
                let expanded = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => {
                        state.discovery_execution.change(|s| s.stage = WorkspaceStage::Paused);
                        break;
                    }
                    _ = state.shutdown.cancelled() => {
                        state.discovery_execution.change(|s| s.stage = WorkspaceStage::Paused);
                        break;
                    }
                    result = expansion => result,
                };
                match expanded {
                    Ok(true) => continue,
                    Ok(false) => {}
                    Err(code) => {
                        state.discovery_execution.change(|s| {
                            s.stage = if matches!(
                                code.as_str(),
                                "query_round_no_available_platform" | "query_round_no_new_queries"
                            ) {
                                WorkspaceStage::NeedsEvidence
                            } else {
                                WorkspaceStage::Error
                            };
                            let code = safe_failure_code(&code);
                            s.reason = Some(code.clone());
                            s.failure_phase = Some("execution".into());
                            s.failure_code = Some(code);
                        });
                        break;
                    }
                }
            }
            let (stage, reason) = match result.step {
                DiscoveryStep::TargetReached => (WorkspaceStage::Finished, "target_reached"),
                DiscoveryStep::BudgetExhausted => {
                    (WorkspaceStage::BudgetExhausted, "budget_exhausted")
                }
                DiscoveryStep::NeedsEvidence => (WorkspaceStage::NeedsEvidence, "needs_evidence"),
                DiscoveryStep::SearchExhausted => {
                    (WorkspaceStage::NeedsEvidence, "search_exhausted")
                }
                _ => (WorkspaceStage::Paused, "busy"),
            };
            state.discovery_execution.change(|s| {
                s.stage = stage;
                s.reason = Some(reason.into());
            });
            break;
        }
        if result.campaign.used_calls <= used {
            state.discovery_execution.change(|s| {
                s.stage = WorkspaceStage::Error;
                s.reason = Some("progress_unconfirmed".into());
                s.failure_phase = Some("readback".into());
            });
            break;
        }
        used = result.campaign.used_calls;
        state.discovery_execution.change(|s| {
            if !cancel.is_cancelled() {
                s.stage = WorkspaceStage::Searching;
            }
        });
    }
    let terminal_stage = state
        .discovery_execution
        .snapshot(credential)
        .map(|s| s.stage);
    state
        .discovery_execution
        .change(|s| s.stage = WorkspaceStage::Pausing);
    // Already admitted work owns its receipts even after pause. Drain scores
    // and the older projection before one final latest-state reconciliation.
    stop_queued_scores.cancel();
    for job in score_jobs {
        finish_score(
            job,
            state,
            &input,
            credential,
            &mut published_recommendations,
            &mut sync_lane,
        )
        .await;
    }
    sync_lane.drain().await;
    if account(state).await.as_deref() == Ok(credential) {
        sync_saved(state, &input, credential).await;
    }
    state.discovery_execution.change(|s| {
        if s.stage == WorkspaceStage::Pausing {
            s.stage = terminal_stage.unwrap_or(WorkspaceStage::Paused);
        }
    });
    record_timing(state, &input.campaign_id, "pipeline.wall", run_started);
    flush_metrics().await;
}

// A failed change lookup is reconciled at stop, never treated as a recommendation.
fn should_sync(advancing: bool, published: Option<&str>, current: Option<&str>) -> bool {
    !advancing || current.is_some_and(|value| Some(value) != published)
}

async fn recommendation_revision(state: &ServiceState, campaign: &str) -> Option<String> {
    let platform = state.platform().await.ok()?;
    let creators = platform.creators().ok()?;
    creators
        .discovery_recommendation_revision_async(campaign.into())
        .await
        .ok()
}

// Return whether the runner may continue to independent provider steps.
// A terminal transcript, including unavailable/source-local failure, changes
// the original material. Re-evaluate saved evidence before considering comments.
// Creator's persisted run state owns failure classification; do not duplicate
// its error-code policy here or score after an auth/risk/dependency stop.
fn transcript_ready_for_review(
    campaign: &onecrew_creator_runtime::DiscoveryCampaign,
    hit: &str,
    read: &onecrew_creator_runtime::DiscoveryTranscriptRead,
) -> bool {
    campaign.phase == "active"
        && read.search_hit_id == hit
        && matches!(read.status.as_str(), "completed" | "unavailable" | "failed")
        && read.call_number > 0
        && read.call_number <= campaign.used_calls
        && campaign.platform_runs.iter().any(|run| {
            run.platform == "youtube"
                && run.profile_ref == read.profile_ref
                && run.status == "completed"
                && run.stop_code.is_none()
        })
}

fn record_score_outcome<T>(
    supervisor: &DiscoveryExecutionSupervisor,
    hit: String,
    score: Result<Result<T, String>, String>,
) -> bool {
    let continue_search =
        matches!(&score, Ok(Ok(_))) || matches!(&score, Ok(Err(code)) if deferred(code));
    supervisor.change(|s| {
        if matches!(&score, Ok(Ok(_))) {
            s.deferred_hits.retain(|d| d.search_hit_id != hit);
        } else {
            let code = safe_failure(&score);
            if let Some(existing) = s.deferred_hits.iter_mut().find(|d| d.search_hit_id == hit) {
                existing.reason = code.clone();
            } else if s.deferred_hits.len() < 1000 {
                s.deferred_hits.push(DeferredScore {
                    search_hit_id: hit,
                    reason: code.clone(),
                });
            }
            if !continue_search {
                s.stage = WorkspaceStage::Error;
                s.reason = Some("request_failed".into());
                s.failure_phase = Some("score".into());
                s.failure_code = Some(code);
            }
        }
        s.deferred_scores = s.deferred_hits.len() as u32;
    });
    continue_search
}

async fn sync_saved(state: &Arc<ServiceState>, input: &ExecutionInput, credential: &str) {
    let started = std::time::Instant::now();
    let owner = state.clone();
    let id = input.campaign_id.clone();
    let expected_account = credential.to_owned();
    // Check queued work after admission: switching accounts must never publish
    // an earlier account's campaign through the new connection.
    let outcome = state
        .work
        .run(async move {
            if account(&owner).await? != expected_account {
                return Err("account_changed".into());
            }
            crate::opencli_discovery::sync_library(&owner, id).await
        })
        .await;
    let outcome = outcome.and_then(|result| result);
    if let Err(code) = outcome {
        state.discovery_execution.record_sync(
            credential,
            &input.campaign_id,
            input.expected_revision,
            &Err(code),
        );
    } else {
        record_duration(state, &input.campaign_id, "library.readback_completed", 0);
        if outcome.is_ok_and(|result| result.total_hits > 0) {
            record_duration(state, &input.campaign_id, "library.results_available", 0);
        }
    }
    record_timing(state, &input.campaign_id, "sync", started);
}

fn record_timing(state: &ServiceState, campaign: &str, phase: &str, started: std::time::Instant) {
    let elapsed = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    record_duration(state, campaign, phase, elapsed);
}

fn record_duration(state: &ServiceState, campaign: &str, phase: &str, elapsed: u64) {
    crate::discovery_metrics::enqueue(&state.data_dir, campaign, phase, elapsed, None);
    if std::env::var_os("ONECREW_LITE_TRACE_TIMING").as_deref() == Some(std::ffi::OsStr::new("1")) {
        eprintln!("onecrew_lite timing phase={phase} elapsed_ms={elapsed}");
    }
}

pub(crate) fn record_read_failure(
    state: &ServiceState,
    campaign: &str,
    step: Option<&DiscoveryStep>,
    code: &str,
    context: Option<(u32, Option<&str>)>,
) {
    let (kind, platform, target) = match step {
        Some(DiscoveryStep::Search { platform, .. }) => ("search", Some(platform.as_str()), None),
        Some(DiscoveryStep::ReadVideo { search_hit_id, .. }) => {
            ("video", Some("youtube"), Some(search_hit_id.as_str()))
        }
        Some(DiscoveryStep::ReadTranscript { search_hit_id, .. }) => {
            ("transcript", Some("youtube"), Some(search_hit_id.as_str()))
        }
        Some(DiscoveryStep::ReadIdentity { search_hit_id }) => {
            ("identity", Some("twitter"), Some(search_hit_id.as_str()))
        }
        Some(DiscoveryStep::ReadFacebookIdentity { search_hit_id }) => {
            ("identity", Some("facebook"), Some(search_hit_id.as_str()))
        }
        Some(DiscoveryStep::ReadSourceAuthor { search_hit_id }) => (
            "source_author",
            Some("douyin"),
            Some(search_hit_id.as_str()),
        ),
        Some(DiscoveryStep::ReadWebSource { search_hit_id }) => {
            ("web_source", Some("youtube"), Some(search_hit_id.as_str()))
        }
        Some(DiscoveryStep::ReadProfile { search_hit_id }) => {
            ("profile", None, Some(search_hit_id.as_str()))
        }
        Some(DiscoveryStep::ReadContent { search_hit_id }) => {
            ("content", None, Some(search_hit_id.as_str()))
        }
        Some(DiscoveryStep::ReadContentTargets { search_hit_id }) => (
            "content_targets",
            Some("facebook"),
            Some(search_hit_id.as_str()),
        ),
        Some(DiscoveryStep::ReadPublicPage { candidate_id, .. }) => {
            ("public_page", None, Some(candidate_id.as_str()))
        }
        Some(DiscoveryStep::ReadContactLookup { lookup }) => {
            ("contact_lookup", None, Some(lookup.candidate_id.as_str()))
        }
        _ => ("read", None, None),
    };
    let failure = crate::discovery_metrics::ReadFailure {
        recorded_at: now(),
        step: kind.into(),
        platform: platform
            .or_else(|| context.and_then(|c| c.1))
            .map(str::to_owned),
        target_id: target.map(str::to_owned),
        call_number: context.map(|c| c.0),
        code: safe_failure_code(code),
    };
    crate::discovery_metrics::enqueue_failure(&state.data_dir, campaign, failure);
}

async fn flush_metrics() {
    if !matches!(
        tokio::task::spawn_blocking(crate::discovery_metrics::flush_pending).await,
        Ok(Ok(()))
    ) {
        eprintln!("onecrew_lite timing persistence failed");
    }
}

async fn completed_contact_author(state: &ServiceState, campaign: &str, candidate: &str) -> bool {
    let Ok(platform) = state.platform().await else {
        return false;
    };
    let Ok(creators) = platform.creators() else {
        return false;
    };
    creators
        .discovery_candidate_is_qualified_async(campaign.into(), candidate.into())
        .await
        .unwrap_or(false)
}

fn safe_failure<T>(result: &Result<Result<T, String>, String>) -> String {
    let code = match result {
        Ok(Err(code)) | Err(code) => code.as_str(),
        _ => "execution_failed",
    };
    safe_failure_code(code)
}

fn safe_failure_code(code: &str) -> String {
    // Only internal protocol codes, never provider output, URLs or account data.
    if matches!(
        code,
        "score_queue_unavailable"
            | "query_round_authentication_required"
            | "query_round_campaign_changed"
            | "query_round_no_available_platform"
            | "query_round_no_new_queries"
            | "discovery_draft_codex_unavailable"
            | "discovery_draft_codex_login_required"
            | "discovery_draft_codex_failed"
            | "discovery_draft_codex_timeout"
            | "discovery_draft_plan_invalid"
            | "discovery_draft_login_required"
            | "discovery_draft_account_changed"
            | "review_submit_outcome_unknown"
            | "review_read_unavailable"
            | "initial_match_material_unavailable"
            | "host_busy"
            | "host_work_failed"
            | "discovery_dependency_unavailable"
            | "discovery_read_outcome_unknown"
            | "discovery_provider_read_stopped"
            | "discovery_read_operation_stopped"
            | "opencli_timeout"
            | "opencli_output_too_large"
            | "opencli_version_changed"
            | "opencli_daemon_unavailable"
            | "opencli_extension_unavailable"
            | "opencli_profile_unavailable"
            | "opencli_command_unavailable"
            | "opencli_invalid_output"
            | "opencli_invalid_input"
            | "opencli_unavailable"
            | "opencli_command_failed"
            | "opencli_risk_control"
            | "opencli_auth_required"
            | "search_completion_failed"
            | "language_sources_exhausted"
            | "contact_lookup_stale"
    ) || crate::discovery_execution::deferred(code)
        || matches!(
            code,
            "library_source_stale"
                | "library_busy"
                | "library_unavailable"
                | "library_identity_unavailable"
                | "library_account_changed"
                | "library_request_failed"
                | "library_request_invalid"
                | "library_sync_interrupted"
                | "library_table_schema_changed"
                | "library_row_identity_changed"
                | "library_write_outcome_unknown"
                | "library_account_mismatch"
                | "library_login_required"
                | "library_authorization_pending"
                | "library_auth_unavailable"
                | "library_instance_changed"
                | "library_invalid_response"
                | "library_projection_conflict"
                | "library_read_failed"
                | "library_readback_incomplete"
                | "library_request_unavailable"
                | "library_runtime_unavailable"
                | "library_service_unavailable"
                | "library_sync_upgrade_required"
                | "library_table_missing"
                | "library_unmanaged_table"
                | "library_write_failed"
                | "library_no_search_hits"
                | "library_capacity_exceeded"
        )
    {
        code.into()
    } else {
        "execution_failed".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_transcript_reassessment_uses_the_creators_persisted_gate() {
        use onecrew_creator_runtime::*;
        use serde_json::json;

        for (text, stop, expected) in [
            (true, None, true),
            (false, None, true),
            (false, Some("opencli_invalid_output"), true),
            (false, Some("opencli_timeout"), true),
            (false, Some("opencli_output_too_large"), true),
            (false, Some("opencli_command_failed"), true),
            (false, Some("opencli_auth_required"), false),
            (false, Some("opencli_risk_control"), false),
            (false, Some("opencli_extension_unavailable"), false),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
            owner
                .create_discovery_campaign(CreateDiscoveryCampaignInput {
                    id: "terminal-transcript".into(),
                    brief: "Original language first".into(),
                    opencli_version: "1.8.8".into(),
                    criteria: json!({"language":"日本語","min_followers":10000,
                    "requires_ai_tool_demo":true,"requires_public_tool_url":false}),
                    scopes: vec![DiscoveryPlatformScope {
                        platform: "youtube".into(),
                        profile_ref: "chrome".into(),
                        keywords: vec!["AI".into()],
                    }],
                    target_count: 2,
                    max_calls: Some(20),
                })
                .unwrap();
            owner
                .approve_discovery_campaign("terminal-transcript", 1)
                .unwrap();
            let search = owner
                .claim_discovery_search("terminal-transcript", 1, "youtube", "chrome")
                .unwrap();
            let source = "https://www.youtube.com/watch?v=LR04bU_yV5k";
            owner
                .finish_discovery_search(FinishDiscoverySearchInput {
                    claim: search,
                    status: "completed".into(),
                    stop_code: None,
                    search_hits: vec![DiscoverySearchHitInput {
                        source_url: source.into(),
                        title: "AI".into(),
                        author_label: "Maker".into(),
                        author_id: None,
                        author_url: None,
                    }],
                })
                .unwrap();
            let hit = owner
                .list_discovery_search_hits(ListDiscoverySearchHitsInput {
                    campaign_id: "terminal-transcript".into(),
                    after_id: None,
                    limit: 10,
                })
                .unwrap()
                .remove(0)
                .id;
            let video = owner
                .claim_discovery_video("terminal-transcript", 1, "chrome", &hit)
                .unwrap();
            owner.finish_discovery_video(FinishDiscoveryVideoInput {
                claim: video, stop_code: None,
                candidate: Some(UpsertDiscoveryCandidateInput {
                    campaign_id: "terminal-transcript".into(), expected_revision: 1,
                    platform: "youtube".into(), source_profile_ref: "chrome".into(),
                    platform_account_id: "UCaaaaaaaaaaaaaaaaaaaaaa".into(), display_name: "Maker".into(),
                    profile_url: "https://www.youtube.com/channel/UCaaaaaaaaaaaaaaaaaaaaaa".into(),
                    evidence: json!({"source_urls":[source],"subscribers_raw":"12.4K subscribers",
                        "subscribers_source_url":source,"captured_at":100,"video_description":"AI SSD RTX"}),
                }),
            }).unwrap();
            let claim = owner
                .claim_discovery_transcript("terminal-transcript", 1, "chrome", &hit)
                .unwrap();
            let read = owner
                .finish_discovery_transcript(FinishDiscoveryTranscriptInput {
                    claim,
                    truncated: false,
                    stop_code: stop.map(str::to_owned),
                    segments: if text {
                        vec![DiscoveryTranscriptSegment {
                            timestamp: "0:10".into(),
                            speaker: String::new(),
                            text: "AI SSD RTX".into(),
                        }]
                    } else {
                        vec![]
                    },
                })
                .unwrap();
            let campaign = owner.get_discovery_campaign("terminal-transcript").unwrap();
            assert_eq!(
                transcript_ready_for_review(&campaign, &hit, &read),
                expected,
                "{stop:?}"
            );
            assert_eq!(campaign.used_calls, 3);
            assert!(!transcript_ready_for_review(&campaign, "other-hit", &read));
            let mut wrong = read.clone();
            wrong.profile_ref = "other-browser".into();
            assert!(!transcript_ready_for_review(&campaign, &hit, &wrong));
            wrong = read.clone();
            wrong.status = "reading".into();
            assert!(!transcript_ready_for_review(&campaign, &hit, &wrong));
            drop(owner);
            let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
            let saved = reopened
                .get_discovery_transcript("terminal-transcript", &hit)
                .unwrap();
            let saved_campaign = reopened
                .get_discovery_campaign("terminal-transcript")
                .unwrap();
            assert_eq!(saved, read);
            assert_eq!(
                transcript_ready_for_review(&saved_campaign, &hit, &saved),
                expected
            );
            assert_eq!(saved_campaign.used_calls, 3);
        }
    }

    #[test]
    fn discovery_read_failure_keeps_the_safe_protocol_reason() {
        for code in [
            "discovery_dependency_unavailable",
            "discovery_read_outcome_unknown",
            "discovery_provider_read_stopped",
            "opencli_extension_unavailable",
            "opencli_invalid_output",
            "library_request_invalid",
            "library_sync_upgrade_required",
            "library_login_required",
            "library_authorization_pending",
            "library_auth_unavailable",
            "query_round_no_available_platform",
            "query_round_no_new_queries",
            "discovery_draft_codex_login_required",
            "discovery_draft_codex_timeout",
        ] {
            assert_eq!(safe_failure::<()>(&Ok(Err(code.into()))), code);
        }
    }
    fn input() -> ExecutionInput {
        ExecutionInput {
            campaign_id: "fixture".into(),
            expected_revision: 1,
            profile_ref: "chrome".into(),
            excluded_platforms: vec![],
        }
    }
    #[test]
    fn admission_is_single_flight_and_pause_does_not_finish_an_inflight_read() {
        let supervisor = DiscoveryExecutionSupervisor::default();
        let cancel = supervisor.admit(&input(), "account".into(), 12).unwrap();
        assert!(supervisor.blocks_backend_sync());
        assert!(supervisor.admit(&input(), "account".into(), 12).is_err());
        supervisor.cancel();
        assert!(supervisor.blocks_backend_sync());
        assert!(cancel.is_cancelled());
        assert_eq!(
            supervisor.snapshot("account").unwrap().stage,
            WorkspaceStage::Pausing
        );
        assert!(supervisor.snapshot("other-account").is_none());
        assert!(supervisor.admit(&input(), "account".into(), 12).is_err());
        supervisor.change(|s| s.stage = WorkspaceStage::Paused);
        assert!(!supervisor.blocks_backend_sync());
        assert!(supervisor.admit(&input(), "account".into(), 13).is_ok());
    }
    #[tokio::test]
    async fn backend_sync_waits_for_draining_tasks_even_after_terminal_status() {
        let supervisor = DiscoveryExecutionSupervisor::default();
        supervisor.admit(&input(), "account".into(), 0).unwrap();
        let (done, wait) = tokio::sync::oneshot::channel::<()>();
        supervisor
            .inner
            .lock()
            .unwrap()
            .tasks
            .push(tokio::spawn(async move {
                wait.await
                    .expect("test completion sender must remain available");
            }));
        supervisor.change(|s| s.stage = WorkspaceStage::Paused);
        assert!(supervisor.blocks_backend_sync());
        done.send(()).unwrap();
        let tasks = std::mem::take(&mut supervisor.inner.lock().unwrap().tasks);
        for task in tasks {
            task.await.unwrap();
        }
        assert!(!supervisor.blocks_backend_sync());
    }
    #[tokio::test]
    async fn disconnect_preserves_current_receipt_and_pause_rejects_next_read() {
        let work = Arc::new(crate::work::WorkOwner::default());
        let supervisor = DiscoveryExecutionSupervisor::default();
        let cancel = supervisor.admit(&input(), "account".into(), 0).unwrap();
        let (started, started_rx) = tokio::sync::oneshot::channel();
        let (release, release_rx) = tokio::sync::oneshot::channel();
        let (saved, saved_rx) = tokio::sync::oneshot::channel();
        let owner = work.clone();
        let token = cancel.clone();
        let caller = tokio::spawn(async move {
            owned_read(&owner, token, async move {
                started.send(()).unwrap();
                release_rx.await.unwrap();
                saved.send("durable receipt").unwrap();
                Ok::<_, String>(())
            })
            .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(2), started_rx)
            .await
            .unwrap()
            .unwrap();
        // The HTTP waiter disappearing cannot abandon its admitted provider read.
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        supervisor.cancel();
        release.send(()).unwrap();
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(2), saved_rx)
                .await
                .unwrap()
                .unwrap(),
            "durable receipt"
        );
        let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = reads.clone();
        let result = owned_read(&work, cancel, async move {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok::<_, String>(())
        })
        .await
        .unwrap();
        assert_eq!(result.unwrap_err(), "discovery_execution_paused");
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn uncertain_score_is_candidate_local_and_does_not_stop_other_reads() {
        let supervisor = DiscoveryExecutionSupervisor::default();
        let cancel = supervisor.admit(&input(), "account".into(), 12).unwrap();
        supervisor.change(|s| s.stage = WorkspaceStage::Verifying);
        for code in ["review_submit_outcome_unknown", "review_read_unavailable"] {
            assert!(record_score_outcome::<()>(
                &supervisor,
                "strayfe".into(),
                Ok(Err(code.into()))
            ));
            let status = supervisor.snapshot("account").unwrap();
            assert_eq!(status.stage, WorkspaceStage::Verifying);
            assert!(status.reason.is_none());
            assert_eq!(status.deferred_scores, 1);
            assert_eq!(status.deferred_hits[0].reason, code);
            assert!(!cancel.is_cancelled());
        }
        let work = Arc::new(crate::work::WorkOwner::default());
        let next_read = owned_read(&work, cancel, async { Ok::<_, String>(13) })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(next_read, 13);
        // The next independent candidate is scored without losing the quarantined one.
        assert!(record_score_outcome(
            &supervisor,
            "other".into(),
            Ok(Ok(()))
        ));
        assert_eq!(supervisor.snapshot("account").unwrap().deferred_scores, 1);
        // Only a confirmed receipt resolves the candidate's exception.
        assert!(record_score_outcome(
            &supervisor,
            "strayfe".into(),
            Ok(Ok(()))
        ));
        assert_eq!(supervisor.snapshot("account").unwrap().deferred_scores, 0);
        assert!(!record_score_outcome::<()>(
            &supervisor,
            "other".into(),
            Ok(Err("review_account_changed".into()))
        ));
        assert_eq!(
            supervisor.snapshot("account").unwrap().stage,
            WorkspaceStage::Error
        );
        assert!(deferred("review_submit_outcome_unknown"));
        assert!(deferred("review_read_unavailable"));
        // Account changes and admission failures must still stop the runner.
        assert!(!deferred("review_account_changed_after_submit"));
        assert!(!deferred("review_account_changed"));
        assert!(!deferred("host_work_failed"));
    }
    #[test]
    fn intermediate_evidence_batches_but_recommendations_and_stops_publish_immediately() {
        assert!(!should_sync(true, Some("[]"), Some("[]")));
        assert!(!should_sync(true, Some("author-a"), Some("author-a")));
        assert!(should_sync(true, Some("[]"), Some("author-a")));
        assert!(should_sync(
            true,
            Some("author-a"),
            Some("author-a-new-contact")
        ));
        assert!(should_sync(true, Some("author-a"), Some("[]")));
        assert!(!should_sync(true, Some("author-a"), None));
        assert!(should_sync(false, Some("author-a"), None));
    }

    #[test]
    fn missing_content_language_does_not_stop_independent_authors() {
        let supervisor = DiscoveryExecutionSupervisor::default();
        supervisor.admit(&input(), "account".into(), 5).unwrap();
        assert!(record_score_outcome::<()>(
            &supervisor,
            "missing-language".into(),
            Ok(Err("initial_match_content_language_unavailable".into()))
        ));
        let status = supervisor.snapshot("account").unwrap();
        assert_ne!(status.stage, WorkspaceStage::Error);
        assert_eq!(status.deferred_scores, 1);
        assert!(record_score_outcome(
            &supervisor,
            "next-author".into(),
            Ok(Ok(()))
        ));
    }

    #[test]
    fn missing_review_material_skips_the_candidate_without_stopping_search() {
        let supervisor = DiscoveryExecutionSupervisor::default();
        let cancel = supervisor.admit(&input(), "account".into(), 5).unwrap();
        assert!(record_score_outcome::<()>(
            &supervisor,
            "no-native-candidate".into(),
            Ok(Err("initial_match_material_unavailable".into()))
        ));
        assert_ne!(
            supervisor.snapshot("account").unwrap().stage,
            WorkspaceStage::Error
        );
        assert!(record_score_outcome(
            &supervisor,
            "next-author".into(),
            Ok(Ok(()))
        ));
        assert!(!cancel.is_cancelled());
    }

    #[test]
    fn known_candidate_score_errors_are_deferred() {
        assert!(deferred("initial_match_discovery_draft_codex_timeout"));
        assert!(deferred("initial_match_finding_invalid"));
        assert!(deferred("initial_match_required_count_unknown"));
        assert!(deferred("initial_match_audience_language_invalid"));
        assert!(deferred("initial_match_audience_sample_insufficient"));
        assert!(deferred("review_material_changed"));
        assert!(!deferred("review_submission_unknown"));
        assert!(deferred("initial_match_material_unavailable"));
    }
    #[test]
    fn sync_failure_and_recovery_are_bound_to_account_campaign_and_revision() {
        let supervisor = DiscoveryExecutionSupervisor::default();
        supervisor.admit(&input(), "account".into(), 5).unwrap();
        supervisor.record_sync("account", "fixture", 1, &Err("library_source_stale".into()));
        assert_eq!(
            supervisor
                .snapshot("account")
                .unwrap()
                .sync_error
                .as_deref(),
            Some("library_source_stale")
        );
        let receipt = Ok(onecrew_local_runtime::DiscoveryLibrarySyncResult {
            campaign_id: "fixture".into(),
            table_id: "table".into(),
            web_url: "http://127.0.0.1:8091/library/table".into(),
            total_hits: 5,
            inserted_rows: 2,
        });
        for (account, campaign, revision) in [
            ("other", "fixture", 1),
            ("account", "other", 1),
            ("account", "fixture", 2),
        ] {
            supervisor.record_sync(account, campaign, revision, &receipt);
            assert!(supervisor.snapshot("account").unwrap().sync_pending);
        }
        supervisor.record_sync("account", "fixture", 1, &receipt);
        let recovered = supervisor.snapshot("account").unwrap();
        assert!(!recovered.sync_pending);
        assert!(recovered.sync_error.is_none());
        assert!(recovered.synced_at.is_some());
        supervisor.change(|s| s.stage = WorkspaceStage::Paused);
        supervisor.admit(&input(), "account".into(), 5).unwrap();
        assert_eq!(
            supervisor.snapshot("account").unwrap().synced_at,
            recovered.synced_at
        );
        supervisor.record_sync(
            "account",
            "fixture",
            1,
            &Err("private http://secret.invalid/token".into()),
        );
        assert_eq!(
            supervisor
                .snapshot("account")
                .unwrap()
                .sync_error
                .as_deref(),
            Some("execution_failed")
        );
    }
}
