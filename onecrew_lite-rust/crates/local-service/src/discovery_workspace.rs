//! Workbench presence is a presentation lease, never Campaign authority.
//! Numbers and source exceptions are read from the existing Creator owner.
use crate::ServiceState;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceStage {
    Ready,
    Searching,
    Verifying,
    Pausing,
    Paused,
    Finished,
    BudgetExhausted,
    NeedsEvidence,
    Error,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspacePresence {
    session_id: uuid::Uuid,
    sequence: u64,
    campaign_id: Option<String>,
    stage: WorkspaceStage,
    activate: bool,
    #[serde(default)]
    stop: Option<WorkspaceStop>,
    #[serde(default)]
    excluded_platforms: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkspaceStopReason {
    UserPaused,
    SearchPageLeft,
    PageClosed,
    RequestFailed,
    RevisionChanged,
    ProgressUnconfirmed,
    Busy,
    NeedsEvidence,
    SearchExhausted,
    TargetReached,
    BudgetExhausted,
    ExecutionInterrupted,
}

/// Browser-reported display diagnostic, never execution or approval authority.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceStop {
    reason: WorkspaceStopReason,
    recorded_at: u64,
}

#[derive(Clone)]
struct Selection {
    session_id: uuid::Uuid,
    campaign_id: String,
    credential_id: String,
    stage: WorkspaceStage,
    seen: Instant,
    stop: Option<WorkspaceStop>,
}
#[derive(Default)]
struct PresenceState {
    selected: Option<Selection>,
    history: HashMap<String, Vec<WorkspaceActivity>>,
    sequences: HashMap<uuid::Uuid, (u64, Instant)>,
    activated_at: Option<Instant>,
}
#[derive(Default)]
pub(crate) struct WorkspaceActivityOwner {
    state: Mutex<PresenceState>,
}

impl WorkspaceActivityOwner {
    async fn select(
        &self,
        input: WorkspacePresence,
        credential_id: String,
        issued_at: Instant,
    ) -> Result<(), String> {
        let mut state = self.state.lock().await;
        let active = state.selected.as_ref().map(|s| s.session_id);
        state.sequences.retain(|id, (_, seen)| {
            Some(*id) == active || seen.elapsed() < Duration::from_secs(300)
        });
        if state
            .sequences
            .get(&input.session_id)
            .is_some_and(|(old, _)| *old >= input.sequence)
        {
            return Ok(());
        }
        // Bound open workbench streams. A late heartbeat cannot reactivate a
        // different tab; only an explicit selection/focus may take ownership.
        if !state.sequences.contains_key(&input.session_id) && state.sequences.len() >= 64 {
            return Err("workspace_presence_capacity".into());
        }
        state
            .sequences
            .insert(input.session_id, (input.sequence, Instant::now()));
        if input.activate {
            if state
                .activated_at
                .is_some_and(|current| current >= issued_at)
            {
                return Ok(());
            }
            state.activated_at = Some(issued_at);
        }
        if !input.activate
            && state
                .selected
                .as_ref()
                .is_none_or(|s| s.session_id != input.session_id)
        {
            return Ok(());
        }
        state.selected = input.campaign_id.map(|campaign_id| Selection {
            session_id: input.session_id,
            campaign_id,
            credential_id,
            stage: input.stage,
            seen: Instant::now(),
            stop: input.stop,
        });
        Ok(())
    }
}

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) struct WorkspaceActivity {
    pub(crate) campaign_id: String,
    brief: String,
    stage: WorkspaceStage,
    qualified: u32,
    pending: u32,
    target: u32,
    attempted: usize,
    total: usize,
    used: u32,
    max: Option<u32>,
    login_required: usize,
    read_errors: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_read_stops: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    read_operation_stops: Option<Vec<onecrew_creator_runtime::DiscoveryReadStop>>,
    restricted: usize,
    stale: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    library_sync_pending: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deferred_scores: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure_phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    library_sync_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    library_synced_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<WorkspaceStop>,
}

async fn credential(state: &ServiceState) -> Result<String, String> {
    let login = state.validate_login().await?;
    if login.status.state != onecrew_creator_sync::CreatorSyncAdmissionState::Active {
        return Err("workspace_login_required".into());
    }
    login
        .status
        .credential_id
        .ok_or_else(|| "workspace_login_required".into())
}

pub(crate) async fn presence(state: &ServiceState, input: WorkspacePresence) -> Result<(), String> {
    if input.excluded_platforms.len() > 64
        || input.excluded_platforms.iter().any(|platform| {
            platform.is_empty()
                || platform.len() > 80
                || !platform
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
    {
        return Err("workspace_exclusions_invalid".into());
    }
    if input
        .stop
        .as_ref()
        .is_some_and(|stop| stop.recorded_at == 0 || stop.recorded_at > 8_640_000_000_000_000)
    {
        return Err("workspace_stop_invalid".into());
    }
    let issued_at = Instant::now();
    let account = credential(state).await?;
    if let Some(id) = &input.campaign_id {
        if id.is_empty() || id.len() > 100 {
            return Err("workspace_campaign_invalid".into());
        }
        state
            .platform()
            .await?
            .creators()?
            .get_discovery_campaign_async(id.clone())
            .await
            .map_err(|_| "workspace_campaign_unavailable")?;
    }
    if credential(state).await? != account {
        return Err("workspace_account_changed".into());
    }
    state
        .workspace_activity
        .select(input, account, issued_at)
        .await?;
    // Capture selection transitions even when the Host has unmounted every card.
    // This is display state only; a failed snapshot never changes the selection.
    drop(snapshot(state).await);
    Ok(())
}

pub(crate) async fn history(state: &ServiceState) -> Vec<WorkspaceActivity> {
    let Ok(account) = credential(state).await else {
        return Vec::new();
    };
    state
        .workspace_activity
        .state
        .lock()
        .await
        .history
        .get(&account)
        .cloned()
        .unwrap_or_default()
}

fn retain_activity(history: &mut Vec<WorkspaceActivity>, activity: WorkspaceActivity) {
    if let Some(previous) = history
        .iter_mut()
        .find(|item| item.campaign_id == activity.campaign_id)
    {
        *previous = activity;
    } else {
        history.push(activity);
    }
}

#[test]
fn workspace_history_keeps_missed_selection_and_isolates_accounts() {
    let first = WorkspaceActivity {
        campaign_id: "first".into(),
        brief: "旧任务".into(),
        stage: WorkspaceStage::Paused,
        qualified: 0,
        pending: 1,
        target: 4,
        attempted: 1,
        total: 2,
        used: 3,
        max: Some(20),
        login_required: 0,
        read_errors: 0,
        current_read_stops: None,
        read_operation_stops: None,
        restricted: 0,
        stale: false,
        updated_at: None,
        library_sync_pending: None,
        deferred_scores: None,
        failure_phase: None,
        failure_code: None,
        library_sync_error: None,
        library_synced_at: None,
        stop: None,
    };
    let mut state = PresenceState::default();
    let history = state.history.entry("account-a".into()).or_default();
    retain_activity(history, first.clone());
    let mut second = first.clone();
    second.campaign_id = "second".into();
    retain_activity(history, second);
    let mut returned = first;
    returned.used = 5;
    retain_activity(history, returned);
    assert_eq!(
        history
            .iter()
            .map(|item| item.campaign_id.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second"]
    );
    assert_eq!(history[0].used, 5);
    assert!(!state.history.contains_key("account-b"));
    assert!(
        state.selected.is_none(),
        "display history never selects or starts a Campaign"
    );
}

pub(crate) async fn snapshot(state: &ServiceState) -> Result<Option<WorkspaceActivity>, String> {
    let account = credential(state).await?;
    let selected = state.workspace_activity.state.lock().await.selected.clone();
    let execution = state.discovery_execution.snapshot(&account).filter(|run| {
        matches!(
            run.stage,
            WorkspaceStage::Searching | WorkspaceStage::Verifying | WorkspaceStage::Pausing
        ) || selected
            .as_ref()
            .is_none_or(|s| s.credential_id != account || s.campaign_id == run.campaign_id)
    });
    let selection = execution
        .as_ref()
        .map(|run| Selection {
            session_id: uuid::Uuid::nil(),
            campaign_id: run.campaign_id.clone(),
            credential_id: account.clone(),
            stage: run.stage,
            seen: Instant::now(),
            stop: run.reason.as_ref().and_then(|reason| {
                serde_json::from_value(serde_json::json!({
                    "reason": reason, "recorded_at": run.updated_at
                }))
                .ok()
            }),
        })
        .or(selected);
    let Some(selection) = selection else {
        return Ok(None);
    };
    if account != selection.credential_id {
        return Ok(None);
    }
    let platform = state.platform().await?;
    let owner = platform.creators()?;
    let campaign = owner
        .get_discovery_campaign_async(selection.campaign_id.clone())
        .await
        .map_err(|_| "workspace_campaign_unavailable")?;
    let counts = owner
        .count_discovery_candidates_async(selection.campaign_id.clone())
        .await
        .map_err(|_| "workspace_counts_unavailable")?;
    let metrics_dir = state.data_dir.clone();
    let operation_stops = owner
        .discovery_read_stops_async(campaign.id.clone())
        .await
        .map_err(|_| "workspace_read_stops_unavailable")?;
    let metrics_campaign = campaign.id.clone();
    let failure_events = tokio::task::spawn_blocking(move || {
        crate::discovery_metrics::failure_count(&metrics_dir, &metrics_campaign)
    })
    .await
    .map_err(|_| "workspace_metrics_unavailable")?
    .map_err(|_| "workspace_metrics_unavailable")?;
    if credential(state).await? != account {
        return Err("workspace_account_changed".into());
    }
    let mut selected = state.workspace_activity.state.lock().await;
    if execution.is_none()
        && selected.selected.as_ref().is_none_or(|current| {
            current.session_id != selection.session_id
                || current.campaign_id != selection.campaign_id
                || current.credential_id != account
        })
    {
        return Err("workspace_selection_changed".into());
    }
    let current = &selection;
    let stale = execution.is_none() && current.seen.elapsed() > Duration::from_secs(45);
    let reading = campaign
        .platform_runs
        .iter()
        .any(|run| run.status == "searching");
    let stage = if let Some(run) = &execution {
        run.stage
    } else if !reading && counts.qualified >= campaign.target_count {
        WorkspaceStage::Finished
    } else {
        activity_stage(
            current.stage,
            reading,
            stale,
            campaign
                .max_calls
                .is_some_and(|max| campaign.used_calls >= max),
        )
    };
    let count = |status: &str| {
        campaign
            .platform_runs
            .iter()
            .filter(|run| run.status == status)
            .count()
    };
    // Coverage describes the approved Campaign, including unavailable platforms.
    // Execution exclusions must not hide completed reads or persisted stops.
    let total = campaign.scopes.len();
    let attempted = campaign.platform_runs.len() - count("unsearched");
    let login_required = count("unauthorized");
    let current_read_stops = count("technical_stop");
    let read_errors = failure_events.max(counts.public_page_errors as usize);
    let restricted = count("risk_control_stop");
    let activity = WorkspaceActivity {
        campaign_id: campaign.id,
        brief: campaign.brief,
        stage,
        qualified: counts.qualified,
        pending: counts.pending,
        target: campaign.target_count,
        attempted,
        total,
        used: campaign.used_calls,
        max: campaign.max_calls,
        login_required,
        read_errors,
        current_read_stops: Some(current_read_stops),
        read_operation_stops: Some(operation_stops),
        restricted,
        stale,
        updated_at: execution.as_ref().map(|run| run.updated_at),
        library_sync_pending: execution.as_ref().and_then(|run| {
            (run.library.is_some() || run.sync_pending).then_some(run.sync_pending)
        }),
        deferred_scores: execution.as_ref().map(|run| run.deferred_scores),
        failure_phase: execution.as_ref().and_then(|run| run.failure_phase.clone()),
        failure_code: execution.as_ref().and_then(|run| run.failure_code.clone()),
        library_sync_error: execution.as_ref().and_then(|run| run.sync_error.clone()),
        library_synced_at: execution.and_then(|run| run.synced_at),
        stop: if matches!(stage, WorkspaceStage::Searching | WorkspaceStage::Verifying) {
            None
        } else {
            current.stop.clone()
        },
    };
    retain_activity(
        selected.history.entry(account).or_default(),
        activity.clone(),
    );
    Ok(Some(activity))
}

fn activity_stage(
    reported: WorkspaceStage,
    reading: bool,
    stale: bool,
    exhausted: bool,
) -> WorkspaceStage {
    if matches!(
        reported,
        WorkspaceStage::Pausing | WorkspaceStage::Paused | WorkspaceStage::Error
    ) {
        reported
    } else if reading {
        WorkspaceStage::Searching
    } else if stale {
        WorkspaceStage::Paused
    } else if exhausted {
        WorkspaceStage::BudgetExhausted
    } else {
        reported
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(
        session_id: uuid::Uuid,
        sequence: u64,
        id: Option<&str>,
        activate: bool,
    ) -> WorkspacePresence {
        WorkspacePresence {
            session_id,
            sequence,
            campaign_id: id.map(str::to_owned),
            stage: WorkspaceStage::Searching,
            activate,
            stop: None,
            excluded_platforms: vec![],
        }
    }
    #[tokio::test]
    async fn late_activation_cannot_replace_a_newer_tab() {
        let owner = WorkspaceActivityOwner::default();
        let earlier = Instant::now();
        let later = earlier + Duration::from_millis(1);
        owner
            .select(
                input(uuid::Uuid::new_v4(), 1, Some("new"), true),
                "account".into(),
                later,
            )
            .await
            .unwrap();
        owner
            .select(
                input(uuid::Uuid::new_v4(), 1, Some("old"), true),
                "account".into(),
                earlier,
            )
            .await
            .unwrap();
        assert_eq!(
            owner
                .state
                .lock()
                .await
                .selected
                .as_ref()
                .unwrap()
                .campaign_id,
            "new"
        );
    }
    #[test]
    fn uncertain_execution_is_not_projected_as_a_running_loop() {
        assert_eq!(
            activity_stage(WorkspaceStage::Paused, true, false, false),
            WorkspaceStage::Paused
        );
        assert_eq!(
            activity_stage(WorkspaceStage::Error, true, false, false),
            WorkspaceStage::Error
        );
        assert_eq!(
            activity_stage(WorkspaceStage::Pausing, true, false, true),
            WorkspaceStage::Pausing
        );
        assert_eq!(
            activity_stage(WorkspaceStage::Ready, true, false, false),
            WorkspaceStage::Searching
        );
        assert_eq!(
            activity_stage(WorkspaceStage::Ready, false, true, false),
            WorkspaceStage::Paused
        );
        assert!(
            serde_json::from_value::<WorkspaceStop>(
                serde_json::json!({ "reason": "password=secret", "recorded_at": 1 })
            )
            .is_err()
        );
    }
    #[tokio::test]
    async fn tied_activation_times_keep_the_selected_tab() {
        let owner = WorkspaceActivityOwner::default();
        let issued_at = Instant::now();
        owner
            .select(
                input(uuid::Uuid::new_v4(), 1, Some("phone"), true),
                "account".into(),
                issued_at,
            )
            .await
            .unwrap();
        owner
            .select(
                input(uuid::Uuid::new_v4(), 1, Some("water"), true),
                "account".into(),
                issued_at,
            )
            .await
            .unwrap();
        assert_eq!(
            owner
                .state
                .lock()
                .await
                .selected
                .as_ref()
                .unwrap()
                .campaign_id,
            "phone"
        );
    }
    #[tokio::test]
    async fn delayed_or_retired_heartbeats_cannot_replace_selected_campaign() {
        let owner = WorkspaceActivityOwner::default();
        let first = uuid::Uuid::new_v4();
        let second = uuid::Uuid::new_v4();
        owner
            .select(
                input(first, 2, Some("a"), true),
                "account".into(),
                Instant::now(),
            )
            .await
            .unwrap();
        owner
            .select(
                input(first, 1, Some("old"), true),
                "account".into(),
                Instant::now(),
            )
            .await
            .unwrap();
        assert_eq!(
            owner
                .state
                .lock()
                .await
                .selected
                .as_ref()
                .unwrap()
                .campaign_id,
            "a"
        );
        owner
            .select(
                input(second, 1, Some("b"), true),
                "account".into(),
                Instant::now(),
            )
            .await
            .unwrap();
        owner
            .select(
                input(first, 3, Some("a"), false),
                "account".into(),
                Instant::now(),
            )
            .await
            .unwrap();
        assert_eq!(
            owner
                .state
                .lock()
                .await
                .selected
                .as_ref()
                .unwrap()
                .campaign_id,
            "b"
        );
        owner
            .select(
                input(second, 2, None, false),
                "account".into(),
                Instant::now(),
            )
            .await
            .unwrap();
        assert!(owner.state.lock().await.selected.is_none());
    }
}
