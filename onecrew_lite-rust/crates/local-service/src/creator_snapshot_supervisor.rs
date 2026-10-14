//! Service-owned lifecycle for best-effort Creator snapshot tasks.

use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use onecrew_creator_runtime::{CreatorBundle, CreatorRuntime, ListCreatorsInput};
use onecrew_creator_sync::{CreatorSnapshotSyncResult, CreatorSnapshotSyncState, CreatorSyncIssue};
use onecrew_local_runtime::LocalPlatform;
use tokio::{
    sync::{Notify, oneshot},
    task::{JoinError, JoinHandle},
};

const APPLICATION_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

/// Pause between an admitted trigger and the moment its runner starts.
///
/// MCP start/stop edges usually arrive in pairs (a toggle, a restart, a
/// recovery) and every edge is a trigger. Starting the export at once would
/// put SQLite and encoding work on the blocking pool while the page is still
/// reacting to the toggle, so each trigger first waits this long and lets the
/// page respond. The pause also lets a paired edge collapse: the later trigger
/// reaches the runtime while the earlier flight holds the single-flight slot,
/// sees `AlreadyRunning`, and is folded into the one follow-up run described
/// on `supervise_snapshot`. A cancel during the pause wins immediately, so
/// shutdown never waits for the delay.
const SNAPSHOT_START_DELAY: Duration = Duration::from_secs(5);

pub(super) trait CreatorSnapshotRunner: Send + Sync {
    fn run(
        self: Arc<Self>,
    ) -> Pin<Box<dyn Future<Output = CreatorSnapshotSyncResult> + Send + 'static>>;
}

pub(super) struct HostedSnapshotRunner {
    pub(super) platform: Arc<LocalPlatform>,
    pub(super) work: Arc<crate::work::WorkOwner>,
}

impl CreatorSnapshotRunner for HostedSnapshotRunner {
    fn run(
        self: Arc<Self>,
    ) -> Pin<Box<dyn Future<Output = CreatorSnapshotSyncResult> + Send + 'static>> {
        Box::pin(async move {
            let platform = self.platform.clone();
            self.work
                .run(async move { sync_creator_snapshot(&platform).await })
                .await
                .unwrap_or(CreatorSnapshotSyncResult {
                    state: CreatorSnapshotSyncState::TemporarilyUnavailable,
                    creator_count: None,
                    content_sha256: None,
                    snapshot_id: None,
                    replayed: false,
                    issue: None,
                })
        })
    }
}

/// Exports the Creator bundle and hands it to Creator Sync.
///
/// Admission is judged first through `CreatorSyncRuntime::can_sync_now`:
/// an unconfigured, unpaired, or not-yet-validated runtime, or a sync that
/// is already in flight, returns its typed result before the Creator store is
/// touched, so those callers never pay for an export they cannot upload.
/// An admitted run then takes one limit-1 Creator list through the Creator runtime's
/// bounded read-only pool. An empty store returns `NoChanges` without
/// entering the composite bundle export or taking the writer lease.
/// `sync_creator_bundle` keeps the single-flight CAS as the admission
/// authority; a trigger that slips between the two checks costs at most
/// one extra export.
///
/// The export itself is SQLite work plus canonical encoding, so it runs on
/// the blocking pool (HOST-01, HOST-03): this async body only awaits, the
/// tokio worker stays free for commands while the bundle is built, and the
/// Creator lock is held by exactly the one repository call inside the closure.
/// A join failure (panic on the blocking pool) maps to the same
/// `RecoveryRequired` + `VerificationFailed` result as an export error.
async fn sync_creator_snapshot(platform: &LocalPlatform) -> CreatorSnapshotSyncResult {
    let creator_sync = platform.creator_sync();
    if let Err(result) = creator_sync.can_sync_now().await {
        return result;
    }
    let Ok(creators) = platform.creators() else {
        return creator_snapshot_recovery_result();
    };
    match load_creator_snapshot_bundle(creators, |creators| creators.export_creator_bundle()).await
    {
        Ok(Some(bundle)) => creator_sync.sync_creator_bundle(Arc::new(bundle)).await,
        Ok(None) => empty_creator_snapshot_result(),
        Err(result) => result,
    }
}

async fn load_creator_snapshot_bundle<F>(
    creators: CreatorRuntime,
    export: F,
) -> Result<Option<CreatorBundle>, CreatorSnapshotSyncResult>
where
    F: FnOnce(CreatorRuntime) -> Result<CreatorBundle, String> + Send + 'static,
{
    let precheck = creators
        .list_creators_async(ListCreatorsInput {
            limit: Some(1),
            include_archived: true,
            ..ListCreatorsInput::default()
        })
        .await
        .map_err(|_| creator_snapshot_recovery_result())?;
    if precheck.items.is_empty() {
        return Ok(None);
    }
    tokio::task::spawn_blocking(move || export(creators))
        .await
        .map_err(|_| creator_snapshot_recovery_result())?
        .map(Some)
        .map_err(|_| creator_snapshot_recovery_result())
}

fn empty_creator_snapshot_result() -> CreatorSnapshotSyncResult {
    CreatorSnapshotSyncResult {
        state: CreatorSnapshotSyncState::NoChanges,
        creator_count: Some(0),
        content_sha256: None,
        snapshot_id: None,
        replayed: false,
        issue: None,
    }
}

fn creator_snapshot_recovery_result() -> CreatorSnapshotSyncResult {
    CreatorSnapshotSyncResult {
        state: CreatorSnapshotSyncState::RecoveryRequired,
        creator_count: None,
        content_sha256: None,
        snapshot_id: None,
        replayed: false,
        issue: Some(CreatorSyncIssue::VerificationFailed),
    }
}

pub(super) trait CreatorSnapshotLifecycleObserver: Send + Sync {
    fn creator_sync_credential_invalidated(
        self: Arc<Self>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

    /// Receives every diagnostic transition the supervisor records for a
    /// trigger: `Running` once the trigger is admitted, then, after the
    /// terminal outcome is recorded, that outcome. Outcomes the supervisor
    /// classifies itself while joining a reaper (`Panicked`, `JoinFailed`,
    /// `ShutdownTimedOut`) are reported the same way.
    fn creator_snapshot_state_changed(
        self: Arc<Self>,
        run_id: u64,
        outcome: CreatorSnapshotTaskOutcome,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", content = "result", rename_all = "snake_case")]
pub(super) enum CreatorSnapshotTaskOutcome {
    Running,
    Completed(CreatorSnapshotSyncResult),
    Cancelled,
    Panicked,
    JoinFailed,
    ShutdownTimedOut,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(super) struct CreatorSnapshotDiagnostic {
    pub(super) admission: CreatorSnapshotAdmissionDiagnostic,
    pub(super) run: Option<CreatorSnapshotRunDiagnostic>,
    pub(super) last_terminal: Option<CreatorSnapshotRunDiagnostic>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(super) struct CreatorSnapshotAdmissionDiagnostic {
    pub(super) trigger_id: u64,
    pub(super) outcome: CreatorSnapshotTrigger,
    pub(super) recorded_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(super) struct CreatorSnapshotRunDiagnostic {
    pub(super) run_id: u64,
    pub(super) started_at: u64,
    pub(super) finished_at: Option<u64>,
    pub(super) outcome: CreatorSnapshotTaskOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CreatorSnapshotTrigger {
    Started,
    AlreadyTriggered,
    IntakeClosed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CreatorSnapshotShutdownReport {
    pub(super) completed: usize,
    pub(super) cancelled: usize,
    pub(super) panicked: usize,
    pub(super) join_failed: usize,
    pub(super) timed_out: usize,
}

struct OwnedSnapshotTask {
    cancel: Option<oneshot::Sender<()>>,
    started_at: u64,
    /// Receives the terminal outcome when the supervisor, not the reaper,
    /// classifies it (reaper panic, join failure, shutdown timeout).
    observer: Weak<dyn CreatorSnapshotLifecycleObserver>,
    reaper: JoinHandle<CreatorSnapshotTaskOutcome>,
}

struct CreatorSnapshotSupervisorState {
    accepting: bool,
    last_triggered_run_id: u64,
    tasks: BTreeMap<u64, OwnedSnapshotTask>,
    diagnostic: Option<CreatorSnapshotDiagnostic>,
}

impl Default for CreatorSnapshotSupervisorState {
    fn default() -> Self {
        Self {
            accepting: true,
            last_triggered_run_id: 0,
            tasks: BTreeMap::new(),
            diagnostic: None,
        }
    }
}

/// Number of supervised runners currently inside `CreatorSnapshotRunner::run`.
///
/// The runtime's own single-flight slot is what decides `AlreadyRunning`;
/// this counter only lets a trigger that lost that race wait until the
/// supervised flight has returned before its one follow-up, instead of
/// polling the runtime. It is shared by `Arc` with every reaper so a reaper
/// that outlives the supervisor keeps a valid counter.
#[derive(Default)]
struct SnapshotFlights {
    in_flight: AtomicUsize,
    changed: Notify,
}

impl SnapshotFlights {
    fn enter(&self) -> SnapshotFlightGuard<'_> {
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        SnapshotFlightGuard { flights: self }
    }

    /// Resolves to `true` once no supervised runner is in flight and to
    /// `false` when `cancel` fires first. The `Notified` future is created
    /// before the count is read, so a decrement that lands between the read
    /// and the wait still wakes this task (`notify_waiters` reaches every
    /// `Notified` that already exists).
    async fn wait_idle(&self, cancel: &mut oneshot::Receiver<()>) -> bool {
        loop {
            let changed = self.changed.notified();
            if self.in_flight.load(Ordering::SeqCst) == 0 {
                return true;
            }
            let cancelled = tokio::select! {
                biased;
                _ = &mut *cancel => true,
                _ = changed => false
            };
            if cancelled {
                return false;
            }
        }
    }
}

struct SnapshotFlightGuard<'a> {
    flights: &'a SnapshotFlights,
}

impl Drop for SnapshotFlightGuard<'_> {
    fn drop(&mut self) {
        self.flights.in_flight.fetch_sub(1, Ordering::SeqCst);
        self.flights.changed.notify_waiters();
    }
}

pub(super) struct CreatorSnapshotSupervisor {
    state: Mutex<CreatorSnapshotSupervisorState>,
    flights: Arc<SnapshotFlights>,
    start_delay: Duration,
    shutdown_timeout: Duration,
}

impl CreatorSnapshotSupervisor {
    pub(super) fn new() -> Arc<Self> {
        Self::with_timings(SNAPSHOT_START_DELAY, APPLICATION_SHUTDOWN_TIMEOUT)
    }

    fn with_timings(start_delay: Duration, shutdown_timeout: Duration) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(CreatorSnapshotSupervisorState::default()),
            flights: Arc::new(SnapshotFlights::default()),
            start_delay,
            shutdown_timeout,
        })
    }

    /// Keeps the production shutdown timeout but starts the runner at once so
    /// tests never wait for `SNAPSHOT_START_DELAY`.
    #[cfg(test)]
    pub(super) fn without_start_delay_for_test() -> Arc<Self> {
        Self::with_timings(Duration::ZERO, APPLICATION_SHUTDOWN_TIMEOUT)
    }

    pub(super) async fn trigger(
        self: &Arc<Self>,
        run_id: u64,
        runner: Arc<dyn CreatorSnapshotRunner>,
        observer: Weak<dyn CreatorSnapshotLifecycleObserver>,
    ) -> CreatorSnapshotTrigger {
        self.reap_finished().await;

        let mut state = self.state();
        if !state.accepting {
            record_admission(&mut state, run_id, CreatorSnapshotTrigger::IntakeClosed);
            return CreatorSnapshotTrigger::IntakeClosed;
        }
        if run_id <= state.last_triggered_run_id {
            record_admission(&mut state, run_id, CreatorSnapshotTrigger::AlreadyTriggered);
            return CreatorSnapshotTrigger::AlreadyTriggered;
        }
        state.last_triggered_run_id = run_id;
        let recorded_at = epoch_millis();
        let last_terminal = state
            .diagnostic
            .as_ref()
            .and_then(|diagnostic| diagnostic.last_terminal.clone());
        state.diagnostic = Some(CreatorSnapshotDiagnostic {
            admission: CreatorSnapshotAdmissionDiagnostic {
                trigger_id: run_id,
                outcome: CreatorSnapshotTrigger::Started,
                recorded_at,
            },
            run: Some(CreatorSnapshotRunDiagnostic {
                run_id,
                started_at: recorded_at,
                finished_at: None,
                outcome: CreatorSnapshotTaskOutcome::Running,
            }),
            last_terminal,
        });

        let (cancel, cancel_receiver) = oneshot::channel();
        let supervisor = Arc::downgrade(self);
        let flights = Arc::clone(&self.flights);
        let start_delay = self.start_delay;
        let task_observer = observer.clone();
        let reaper = tokio::spawn(async move {
            // The reaper reports `Running` itself, so the terminal report
            // below can never overtake it whatever the runtime schedules first.
            if let Some(observer) = task_observer.upgrade() {
                observer
                    .creator_snapshot_state_changed(run_id, CreatorSnapshotTaskOutcome::Running)
                    .await;
            }
            let outcome =
                supervise_snapshot(run_id, runner, start_delay, flights, cancel_receiver).await;
            if let Some(supervisor) = supervisor.upgrade() {
                supervisor.record_terminal(run_id, recorded_at, outcome.clone());
            }
            log_snapshot_outcome(run_id, &outcome);
            if let Some(observer) = task_observer.upgrade() {
                Arc::clone(&observer)
                    .creator_snapshot_state_changed(run_id, outcome.clone())
                    .await;
                if credential_was_invalidated(&outcome) {
                    observer.creator_sync_credential_invalidated().await;
                }
            }
            outcome
        });
        state.tasks.insert(
            run_id,
            OwnedSnapshotTask {
                cancel: Some(cancel),
                started_at: recorded_at,
                observer,
                reaper,
            },
        );
        CreatorSnapshotTrigger::Started
    }

    pub(super) fn close_intake(&self) {
        self.state().accepting = false;
    }

    /// HOST-10 order: intake is closed, every owned task is cancelled, then
    /// each reaper is drained under `shutdown_timeout` and a late one is
    /// classified instead of awaited. A task still in `SNAPSHOT_START_DELAY`
    /// or waiting for its follow-up observes the cancel at once.
    pub(super) async fn shutdown(self: &Arc<Self>) -> CreatorSnapshotShutdownReport {
        let tasks = {
            let mut state = self.state();
            state.accepting = false;
            let mut tasks = std::mem::take(&mut state.tasks);
            for task in tasks.values_mut() {
                if let Some(cancel) = task.cancel.take()
                    && cancel.send(()).is_err()
                {
                    log::debug!("creator snapshot task finished before intake closed");
                }
            }
            tasks
        };

        let mut report = CreatorSnapshotShutdownReport::default();
        for (run_id, task) in tasks {
            let outcome = self.join_owned_task(run_id, task).await;
            report.observe(&outcome);
        }
        report
    }

    /// The recorded outcome is supervisor-internal: Creator snapshot sync is
    /// imperceptible to the user, so no status projection reads it and only
    /// this module's own tests assert on it.
    #[cfg(test)]
    pub(super) fn diagnostic(&self) -> Option<CreatorSnapshotDiagnostic> {
        self.state().diagnostic.clone()
    }

    #[cfg(test)]
    pub(super) fn has_task_for_run(&self, run_id: u64) -> bool {
        self.state().tasks.contains_key(&run_id)
    }

    fn state(&self) -> std::sync::MutexGuard<'_, CreatorSnapshotSupervisorState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn record_terminal(&self, run_id: u64, started_at: u64, outcome: CreatorSnapshotTaskOutcome) {
        let mut state = self.state();
        let finished_at = epoch_millis();
        if let Some(run) = state
            .diagnostic
            .as_mut()
            .and_then(|diagnostic| diagnostic.run.as_mut())
            .filter(|run| run.run_id == run_id)
        {
            run.outcome = outcome.clone();
            run.finished_at = Some(finished_at);
        }
        if let Some(diagnostic) = state.diagnostic.as_mut() {
            diagnostic.last_terminal = Some(CreatorSnapshotRunDiagnostic {
                run_id,
                started_at,
                finished_at: Some(finished_at),
                outcome,
            });
        }
    }

    async fn reap_finished(self: &Arc<Self>) {
        let finished = {
            let mut state = self.state();
            let run_ids = state
                .tasks
                .iter()
                .filter_map(|(run_id, task)| task.reaper.is_finished().then_some(*run_id))
                .collect::<Vec<_>>();
            run_ids
                .into_iter()
                .filter_map(|run_id| state.tasks.remove(&run_id).map(|task| (run_id, task)))
                .collect::<Vec<_>>()
        };
        for (run_id, task) in finished {
            let _ = self.join_owned_task(run_id, task).await;
        }
    }

    /// Joins one owned reaper. A reaper that returned its own outcome has
    /// already recorded and reported it; the supervisor records and reports
    /// only what it classifies here (reaper panic, join failure, timeout).
    ///
    /// Abort reaches the reaper and, through it, the async part of the
    /// runner. The runner's `spawn_blocking` segment (the Creator export) cannot
    /// be aborted: the blocking pool keeps running it, detached, until it
    /// returns. That is why the join is bounded and a late reaper becomes
    /// `ShutdownTimedOut` instead of being awaited (HOST-09, HOST-10).
    async fn join_owned_task(
        &self,
        run_id: u64,
        mut task: OwnedSnapshotTask,
    ) -> CreatorSnapshotTaskOutcome {
        let outcome = match tokio::time::timeout(self.shutdown_timeout, &mut task.reaper).await {
            Ok(Ok(outcome)) => return outcome,
            Ok(Err(error)) => classify_reaper_error(&error),
            Err(_) => {
                task.reaper.abort();
                // Abort is the expected way for the reaper to end here.
                if let Err(error) = task.reaper.await
                    && !error.is_cancelled()
                {
                    log::warn!("creator snapshot reaper ended abnormally: {error}");
                }
                CreatorSnapshotTaskOutcome::ShutdownTimedOut
            }
        };
        self.record_terminal(run_id, task.started_at, outcome.clone());
        log_snapshot_outcome(run_id, &outcome);
        if let Some(observer) = task.observer.upgrade() {
            observer
                .creator_snapshot_state_changed(run_id, outcome.clone())
                .await;
        }
        outcome
    }
}

fn record_admission(
    state: &mut CreatorSnapshotSupervisorState,
    trigger_id: u64,
    outcome: CreatorSnapshotTrigger,
) {
    let admission = CreatorSnapshotAdmissionDiagnostic {
        trigger_id,
        outcome,
        recorded_at: epoch_millis(),
    };
    match state.diagnostic.as_mut() {
        Some(diagnostic) => diagnostic.admission = admission,
        None => {
            state.diagnostic = Some(CreatorSnapshotDiagnostic {
                admission,
                run: None,
                last_terminal: None,
            });
        }
    }
}

fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

impl CreatorSnapshotShutdownReport {
    fn observe(&mut self, outcome: &CreatorSnapshotTaskOutcome) {
        match outcome {
            CreatorSnapshotTaskOutcome::Completed(_) => self.completed += 1,
            CreatorSnapshotTaskOutcome::Cancelled => self.cancelled += 1,
            CreatorSnapshotTaskOutcome::Panicked => self.panicked += 1,
            CreatorSnapshotTaskOutcome::JoinFailed => self.join_failed += 1,
            CreatorSnapshotTaskOutcome::ShutdownTimedOut => self.timed_out += 1,
            CreatorSnapshotTaskOutcome::Running => self.join_failed += 1,
        }
    }
}

/// Owns one admitted trigger from its start delay to its terminal outcome.
///
/// Order: wait `start_delay` (a cancel wins), run the runner once, and, only
/// when that run reported `AlreadyRunning`, wait for the supervised flight
/// that held the runtime's single-flight slot to return and run once more.
/// The follow-up exists because a trigger is a lifecycle edge whose data may
/// have changed after the earlier flight read the Creator store. It runs at most once
/// per trigger: a follow-up that meets `AlreadyRunning` again is terminal, so
/// no trigger can chain further runs. The follow-up needs no new trigger or
/// task; it reuses this reaper, its run id, and its cancel channel, so
/// `shutdown` drains it exactly like a first run.
async fn supervise_snapshot(
    run_id: u64,
    runner: Arc<dyn CreatorSnapshotRunner>,
    start_delay: Duration,
    flights: Arc<SnapshotFlights>,
    mut cancel: oneshot::Receiver<()>,
) -> CreatorSnapshotTaskOutcome {
    let cancelled = tokio::select! {
        biased;
        _ = &mut cancel => true,
        _ = tokio::time::sleep(start_delay) => false
    };
    if cancelled {
        return CreatorSnapshotTaskOutcome::Cancelled;
    }

    let outcome = run_snapshot_once(Arc::clone(&runner), &flights, &mut cancel).await;
    if !runtime_was_already_running(&outcome) {
        return outcome;
    }
    log::info!(
        "Creator snapshot task for lifecycle trigger {run_id} found a sync already running; \
         it runs once more after that flight returns"
    );
    if !flights.wait_idle(&mut cancel).await {
        return CreatorSnapshotTaskOutcome::Cancelled;
    }
    run_snapshot_once(runner, &flights, &mut cancel).await
}

/// Runs the runner once inside the flight counter and classifies the join.
/// A cancel aborts the child: its async part stops at the next await, while
/// a `spawn_blocking` export inside it is detached and finishes on the
/// blocking pool on its own; the reaper does not wait for it.
async fn run_snapshot_once(
    runner: Arc<dyn CreatorSnapshotRunner>,
    flights: &SnapshotFlights,
    cancel: &mut oneshot::Receiver<()>,
) -> CreatorSnapshotTaskOutcome {
    let _flight = flights.enter();
    let mut child = tokio::spawn(runner.run());
    tokio::select! {
        biased;
        joined = &mut child => classify_snapshot_join(joined),
        _ = &mut *cancel => {
            child.abort();
            match child.await {
                Ok(result) => CreatorSnapshotTaskOutcome::Completed(result),
                Err(error) if error.is_panic() => CreatorSnapshotTaskOutcome::Panicked,
                Err(error) if error.is_cancelled() => CreatorSnapshotTaskOutcome::Cancelled,
                Err(_) => CreatorSnapshotTaskOutcome::JoinFailed,
            }
        }
    }
}

fn classify_snapshot_join(
    joined: Result<CreatorSnapshotSyncResult, JoinError>,
) -> CreatorSnapshotTaskOutcome {
    match joined {
        Ok(result) => CreatorSnapshotTaskOutcome::Completed(result),
        Err(error) if error.is_panic() => CreatorSnapshotTaskOutcome::Panicked,
        Err(_) => CreatorSnapshotTaskOutcome::JoinFailed,
    }
}

fn classify_reaper_error(error: &JoinError) -> CreatorSnapshotTaskOutcome {
    if error.is_panic() {
        CreatorSnapshotTaskOutcome::Panicked
    } else {
        CreatorSnapshotTaskOutcome::JoinFailed
    }
}

fn runtime_was_already_running(outcome: &CreatorSnapshotTaskOutcome) -> bool {
    matches!(
        outcome,
        CreatorSnapshotTaskOutcome::Completed(CreatorSnapshotSyncResult {
            state: CreatorSnapshotSyncState::AlreadyRunning,
            ..
        })
    )
}

fn credential_was_invalidated(outcome: &CreatorSnapshotTaskOutcome) -> bool {
    matches!(
        outcome,
        CreatorSnapshotTaskOutcome::Completed(CreatorSnapshotSyncResult {
            state: CreatorSnapshotSyncState::Revoked | CreatorSnapshotSyncState::Unauthorized,
            ..
        })
    )
}

fn log_snapshot_outcome(run_id: u64, outcome: &CreatorSnapshotTaskOutcome) {
    match outcome {
        CreatorSnapshotTaskOutcome::Completed(result) => log::info!(
            "Creator snapshot task finished for lifecycle trigger {run_id}: state={:?}, issue={:?}",
            result.state,
            result.issue
        ),
        CreatorSnapshotTaskOutcome::Cancelled => {
            log::info!("Creator snapshot task was cancelled during application shutdown")
        }
        CreatorSnapshotTaskOutcome::Panicked => {
            log::warn!("Creator snapshot task panicked for lifecycle trigger {run_id}")
        }
        CreatorSnapshotTaskOutcome::JoinFailed => {
            log::warn!("Creator snapshot task join failed for lifecycle trigger {run_id}")
        }
        CreatorSnapshotTaskOutcome::ShutdownTimedOut => {
            log::warn!("Creator snapshot task timed out during application shutdown")
        }
        CreatorSnapshotTaskOutcome::Running => {}
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use onecrew_creator_sync::{CreatorSyncAdmissionState, CreatorSyncRuntime};
    use onecrew_local_runtime::LocalPlatformHostPaths;

    use super::*;

    /// Makes every Creator export fail so a test can tell "admission refused
    /// before the export" (typed admission result) apart from "export
    /// attempted" (`RecoveryRequired`).
    fn break_creator_export(data_dir: &std::path::Path) {
        let connection = rusqlite::Connection::open(data_dir.join("crm.db")).unwrap();
        connection
            .execute_batch("ALTER TABLE creators RENAME TO creators_hidden_for_test;")
            .unwrap();
    }

    fn open_platform(data_dir: &std::path::Path) -> (Arc<CreatorSyncRuntime>, Arc<LocalPlatform>) {
        let sync = Arc::new(CreatorSyncRuntime::open(data_dir).unwrap());
        let platform = LocalPlatform::open_with_host_paths_and_creator_sync(
            data_dir,
            LocalPlatformHostPaths::without_system_desktop(),
            sync.clone(),
        )
        .unwrap();
        (sync, Arc::new(platform))
    }

    #[tokio::test]
    async fn unadmitted_snapshot_sync_returns_the_admission_result_without_exporting() {
        let directory = tempfile::tempdir().unwrap();
        let (sync, platform) = open_platform(directory.path());
        break_creator_export(directory.path());
        assert!(
            platform
                .creators()
                .unwrap()
                .export_creator_bundle()
                .is_err()
        );

        let fresh = sync_creator_snapshot(&platform).await;
        assert_eq!(fresh.state, CreatorSnapshotSyncState::ValidationRequired);
        assert_eq!(
            fresh.issue,
            Some(CreatorSyncIssue::AdmissionValidationRequired)
        );
        assert!(fresh.creator_count.is_none());

        let status = sync.ensure_admission_validated().await;
        assert_eq!(status.state, CreatorSyncAdmissionState::PairingRequired);
        let unpaired = sync_creator_snapshot(&platform).await;
        assert_eq!(unpaired.state, CreatorSnapshotSyncState::ValidationRequired);
        assert_ne!(unpaired.issue, Some(CreatorSyncIssue::VerificationFailed));
    }

    #[tokio::test]
    async fn empty_creator_store_returns_without_calling_bundle_export_or_growing_the_wal() {
        let directory = tempfile::tempdir().unwrap();
        let (_sync, platform) = open_platform(directory.path());
        let wal_path = directory.path().join("crm.db-wal");
        let wal_len_before = std::fs::metadata(&wal_path).map(|value| value.len()).ok();
        let export_calls = Arc::new(AtomicUsize::new(0));
        let counted_calls = Arc::clone(&export_calls);

        let loaded = load_creator_snapshot_bundle(platform.creators().unwrap(), move |_| {
            counted_calls.fetch_add(1, Ordering::SeqCst);
            panic!("empty Creator precheck must not call export_creator_bundle")
        })
        .await
        .unwrap();

        assert!(loaded.is_none());
        assert_eq!(export_calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            std::fs::metadata(&wal_path).map(|value| value.len()).ok(),
            wal_len_before,
            "limit-1 precheck must stay on the query-only read pool"
        );
    }

    struct NoopObserver;

    impl CreatorSnapshotLifecycleObserver for NoopObserver {
        fn creator_sync_credential_invalidated(
            self: Arc<Self>,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
            Box::pin(async {})
        }

        fn creator_snapshot_state_changed(
            self: Arc<Self>,
            _run_id: u64,
            _outcome: CreatorSnapshotTaskOutcome,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
            Box::pin(async {})
        }
    }

    fn observer() -> Arc<dyn CreatorSnapshotLifecycleObserver> {
        Arc::new(NoopObserver)
    }

    #[derive(Default)]
    struct RecordingObserver {
        events: Mutex<Vec<(u64, CreatorSnapshotTaskOutcome)>>,
    }

    impl RecordingObserver {
        fn events(&self) -> Vec<(u64, CreatorSnapshotTaskOutcome)> {
            self.events.lock().unwrap().clone()
        }

        fn outcomes_for(&self, run_id: u64) -> Vec<CreatorSnapshotTaskOutcome> {
            self.events()
                .into_iter()
                .filter_map(|(id, outcome)| (id == run_id).then_some(outcome))
                .collect()
        }
    }

    impl CreatorSnapshotLifecycleObserver for RecordingObserver {
        fn creator_sync_credential_invalidated(
            self: Arc<Self>,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
            Box::pin(async {})
        }

        fn creator_snapshot_state_changed(
            self: Arc<Self>,
            run_id: u64,
            outcome: CreatorSnapshotTaskOutcome,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
            self.events.lock().unwrap().push((run_id, outcome));
            Box::pin(async {})
        }
    }

    fn weak_observer(
        observer: &Arc<RecordingObserver>,
    ) -> Weak<dyn CreatorSnapshotLifecycleObserver> {
        // Method-call clone keeps `Arc<RecordingObserver>`; the annotated binding
        // performs the unsized coercion, so the weak points at the same allocation.
        let observer: Arc<dyn CreatorSnapshotLifecycleObserver> = observer.clone();
        Arc::downgrade(&observer)
    }

    struct ImmediateRunner {
        calls: AtomicUsize,
        result: CreatorSnapshotSyncResult,
    }

    impl CreatorSnapshotRunner for ImmediateRunner {
        fn run(
            self: Arc<Self>,
        ) -> Pin<Box<dyn Future<Output = CreatorSnapshotSyncResult> + Send + 'static>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let result = self.result.clone();
            Box::pin(async move { result })
        }
    }

    /// Mirrors the runtime's single-flight slot: a concurrent second `run`
    /// reports `AlreadyRunning`, and every admitted run blocks until released.
    struct GatedRunner {
        active: AtomicBool,
        calls: AtomicUsize,
        release: Notify,
    }

    impl CreatorSnapshotRunner for GatedRunner {
        fn run(
            self: Arc<Self>,
        ) -> Pin<Box<dyn Future<Output = CreatorSnapshotSyncResult> + Send + 'static>> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                if self
                    .active
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
                {
                    return result(CreatorSnapshotSyncState::AlreadyRunning);
                }
                self.release.notified().await;
                self.active.store(false, Ordering::Release);
                result(CreatorSnapshotSyncState::Quarantined)
            })
        }
    }

    fn result(state: CreatorSnapshotSyncState) -> CreatorSnapshotSyncResult {
        CreatorSnapshotSyncResult {
            state,
            creator_count: None,
            content_sha256: None,
            snapshot_id: None,
            replayed: false,
            issue: Some(CreatorSyncIssue::RemoteUnavailable),
        }
    }

    async fn wait_until(predicate: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while !predicate() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("supervisor condition");
    }

    async fn settle() {
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
    }

    fn diagnostic_outcome(
        supervisor: &CreatorSnapshotSupervisor,
    ) -> Option<CreatorSnapshotTaskOutcome> {
        supervisor
            .diagnostic()
            .and_then(|diagnostic| diagnostic.run.map(|run| run.outcome))
    }

    #[tokio::test]
    async fn one_run_is_triggered_at_most_once() {
        let supervisor = CreatorSnapshotSupervisor::without_start_delay_for_test();
        let runner = Arc::new(ImmediateRunner {
            calls: AtomicUsize::new(0),
            result: result(CreatorSnapshotSyncState::Quarantined),
        });
        let observer = observer();
        assert_eq!(
            supervisor
                .trigger(7, runner.clone(), Arc::downgrade(&observer))
                .await,
            CreatorSnapshotTrigger::Started
        );
        let accepted = supervisor.diagnostic().expect("accepted diagnostic");
        assert_eq!(accepted.admission.trigger_id, 7);
        assert_eq!(accepted.admission.outcome, CreatorSnapshotTrigger::Started);
        assert!(accepted.admission.recorded_at > 0);
        assert_eq!(
            supervisor
                .trigger(7, runner.clone(), Arc::downgrade(&observer))
                .await,
            CreatorSnapshotTrigger::AlreadyTriggered
        );
        let duplicate = supervisor.diagnostic().expect("duplicate diagnostic");
        assert_eq!(duplicate.admission.trigger_id, 7);
        assert_eq!(
            duplicate.admission.outcome,
            CreatorSnapshotTrigger::AlreadyTriggered
        );
        wait_until(|| {
            !matches!(
                diagnostic_outcome(&supervisor),
                Some(CreatorSnapshotTaskOutcome::Running)
            )
        })
        .await;
        assert_eq!(runner.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn application_shutdown_closes_intake() {
        let supervisor = CreatorSnapshotSupervisor::new();
        supervisor.close_intake();
        let runner = Arc::new(ImmediateRunner {
            calls: AtomicUsize::new(0),
            result: result(CreatorSnapshotSyncState::Quarantined),
        });
        let observer = observer();
        assert_eq!(
            supervisor
                .trigger(1, runner.clone(), Arc::downgrade(&observer))
                .await,
            CreatorSnapshotTrigger::IntakeClosed
        );
        let diagnostic = supervisor.diagnostic().expect("closed diagnostic");
        assert_eq!(diagnostic.admission.trigger_id, 1);
        assert_eq!(
            diagnostic.admission.outcome,
            CreatorSnapshotTrigger::IntakeClosed
        );
        assert!(diagnostic.admission.recorded_at > 0);
        assert!(diagnostic.run.is_none());
        assert_eq!(runner.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn trigger_reports_running_and_then_the_recorded_terminal_outcome() {
        let supervisor = CreatorSnapshotSupervisor::without_start_delay_for_test();
        let runner = Arc::new(ImmediateRunner {
            calls: AtomicUsize::new(0),
            result: result(CreatorSnapshotSyncState::NoChanges),
        });
        let observer = Arc::new(RecordingObserver::default());
        assert_eq!(
            supervisor
                .trigger(4, runner.clone(), weak_observer(&observer))
                .await,
            CreatorSnapshotTrigger::Started
        );
        wait_until(|| observer.events().len() == 2).await;
        assert_eq!(
            observer.events(),
            vec![
                (4, CreatorSnapshotTaskOutcome::Running),
                (
                    4,
                    CreatorSnapshotTaskOutcome::Completed(result(
                        CreatorSnapshotSyncState::NoChanges
                    ))
                ),
            ]
        );
        assert_eq!(
            diagnostic_outcome(&supervisor),
            Some(CreatorSnapshotTaskOutcome::Completed(result(
                CreatorSnapshotSyncState::NoChanges
            )))
        );
        let run = supervisor
            .diagnostic()
            .and_then(|diagnostic| diagnostic.run)
            .expect("completed run");
        assert!(run.started_at > 0);
        assert!(
            run.finished_at
                .is_some_and(|finished| finished >= run.started_at)
        );
    }

    #[tokio::test]
    async fn cancel_during_the_start_delay_wins_before_the_runner_starts() {
        let supervisor = CreatorSnapshotSupervisor::with_timings(
            Duration::from_secs(60),
            Duration::from_secs(1),
        );
        let runner = Arc::new(ImmediateRunner {
            calls: AtomicUsize::new(0),
            result: result(CreatorSnapshotSyncState::Quarantined),
        });
        let observer = Arc::new(RecordingObserver::default());
        assert_eq!(
            supervisor
                .trigger(2, runner.clone(), weak_observer(&observer))
                .await,
            CreatorSnapshotTrigger::Started
        );
        let report = supervisor.shutdown().await;
        assert_eq!(report.cancelled, 1);
        assert_eq!(report.timed_out, 0);
        assert_eq!(runner.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            diagnostic_outcome(&supervisor),
            Some(CreatorSnapshotTaskOutcome::Cancelled)
        );
        assert_eq!(
            observer.outcomes_for(2),
            vec![
                CreatorSnapshotTaskOutcome::Running,
                CreatorSnapshotTaskOutcome::Cancelled
            ]
        );
    }

    #[tokio::test]
    async fn already_running_is_followed_up_once_after_the_supervised_flight_returns() {
        let supervisor = CreatorSnapshotSupervisor::without_start_delay_for_test();
        let runner = Arc::new(GatedRunner {
            active: AtomicBool::new(false),
            calls: AtomicUsize::new(0),
            release: Notify::new(),
        });
        let observer = Arc::new(RecordingObserver::default());

        assert_eq!(
            supervisor
                .trigger(1, runner.clone(), weak_observer(&observer))
                .await,
            CreatorSnapshotTrigger::Started
        );
        wait_until(|| runner.calls.load(Ordering::SeqCst) == 1).await;
        assert_eq!(
            supervisor
                .trigger(2, runner.clone(), weak_observer(&observer))
                .await,
            CreatorSnapshotTrigger::Started
        );
        // The second run meets the single-flight slot and waits for the first
        // flight instead of ending as `AlreadyRunning`.
        wait_until(|| runner.calls.load(Ordering::SeqCst) == 2).await;
        settle().await;
        assert_eq!(runner.calls.load(Ordering::SeqCst), 2);
        let diagnostic = supervisor.diagnostic().expect("second run diagnostic");
        assert_eq!(diagnostic.admission.trigger_id, 2);
        assert_eq!(
            diagnostic.admission.outcome,
            CreatorSnapshotTrigger::Started
        );
        assert_eq!(diagnostic.run.as_ref().map(|run| run.run_id), Some(2));
        assert_eq!(
            diagnostic.run.map(|run| run.outcome),
            Some(CreatorSnapshotTaskOutcome::Running)
        );
        assert!(supervisor.has_task_for_run(1));
        assert!(supervisor.has_task_for_run(2));

        runner.release.notify_one();
        wait_until(|| runner.calls.load(Ordering::SeqCst) == 3).await;
        wait_until(|| observer.outcomes_for(1).len() == 2).await;
        assert_eq!(
            observer.outcomes_for(1),
            vec![
                CreatorSnapshotTaskOutcome::Running,
                CreatorSnapshotTaskOutcome::Completed(result(
                    CreatorSnapshotSyncState::Quarantined
                ))
            ]
        );
        // The late terminal outcome of run 1 did not overwrite run 2.
        assert_eq!(
            diagnostic_outcome(&supervisor),
            Some(CreatorSnapshotTaskOutcome::Running)
        );

        runner.release.notify_one();
        wait_until(|| {
            !matches!(
                diagnostic_outcome(&supervisor),
                Some(CreatorSnapshotTaskOutcome::Running)
            )
        })
        .await;
        assert_eq!(runner.calls.load(Ordering::SeqCst), 3);
        let diagnostic = supervisor.diagnostic().expect("terminal diagnostic");
        let run = diagnostic.run.expect("terminal run");
        assert_eq!(run.run_id, 2);
        assert!(run.finished_at.is_some());
        assert_eq!(
            run.outcome,
            CreatorSnapshotTaskOutcome::Completed(result(CreatorSnapshotSyncState::Quarantined))
        );
        wait_until(|| observer.outcomes_for(2).len() == 2).await;
        assert_eq!(
            observer.outcomes_for(2),
            vec![
                CreatorSnapshotTaskOutcome::Running,
                CreatorSnapshotTaskOutcome::Completed(result(
                    CreatorSnapshotSyncState::Quarantined
                ))
            ]
        );
        let report = supervisor.shutdown().await;
        assert_eq!(report.completed, 2);
        assert_eq!(report.cancelled, 0);
    }

    #[tokio::test]
    async fn a_follow_up_that_still_meets_already_running_is_terminal() {
        let supervisor = CreatorSnapshotSupervisor::without_start_delay_for_test();
        let runner = Arc::new(ImmediateRunner {
            calls: AtomicUsize::new(0),
            result: result(CreatorSnapshotSyncState::AlreadyRunning),
        });
        let observer = Arc::new(RecordingObserver::default());
        assert_eq!(
            supervisor
                .trigger(3, runner.clone(), weak_observer(&observer))
                .await,
            CreatorSnapshotTrigger::Started
        );
        wait_until(|| observer.outcomes_for(3).len() == 2).await;
        settle().await;
        assert_eq!(runner.calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            diagnostic_outcome(&supervisor),
            Some(CreatorSnapshotTaskOutcome::Completed(result(
                CreatorSnapshotSyncState::AlreadyRunning
            )))
        );
    }

    #[tokio::test]
    async fn shutdown_timeout_aborts_and_classifies_an_unresponsive_reaper() {
        let supervisor =
            CreatorSnapshotSupervisor::with_timings(Duration::ZERO, Duration::from_millis(5));
        let observer = Arc::new(RecordingObserver::default());
        let (cancel, _cancel_receiver) = oneshot::channel();
        let reaper =
            tokio::spawn(async { std::future::pending::<CreatorSnapshotTaskOutcome>().await });
        supervisor.state().tasks.insert(
            3,
            OwnedSnapshotTask {
                cancel: Some(cancel),
                started_at: 1,
                observer: weak_observer(&observer),
                reaper,
            },
        );
        supervisor.state().diagnostic = Some(CreatorSnapshotDiagnostic {
            admission: CreatorSnapshotAdmissionDiagnostic {
                trigger_id: 3,
                outcome: CreatorSnapshotTrigger::Started,
                recorded_at: 1,
            },
            run: Some(CreatorSnapshotRunDiagnostic {
                run_id: 3,
                started_at: 1,
                finished_at: None,
                outcome: CreatorSnapshotTaskOutcome::Running,
            }),
            last_terminal: None,
        });

        let report = supervisor.shutdown().await;
        assert_eq!(report.timed_out, 1);
        assert_eq!(
            diagnostic_outcome(&supervisor),
            Some(CreatorSnapshotTaskOutcome::ShutdownTimedOut)
        );
        assert_eq!(
            observer.events(),
            vec![(3, CreatorSnapshotTaskOutcome::ShutdownTimedOut)]
        );
    }

    #[test]
    fn snapshot_start_is_debounced_and_cancel_wins_before_the_runner() {
        let source = include_str!("creator_snapshot_supervisor.rs").replace("\r\n", "\n");
        let production = &source[..source.find("#[cfg(test)]\nmod tests").unwrap()];
        assert!(
            production.contains("const SNAPSHOT_START_DELAY: Duration = Duration::from_secs(5);")
        );
        assert!(
            production
                .contains("Self::with_timings(SNAPSHOT_START_DELAY, APPLICATION_SHUTDOWN_TIMEOUT)")
        );

        let supervise = production.find("async fn supervise_snapshot(").unwrap();
        let body = &production[supervise..];
        let select = body.find("tokio::select!").unwrap();
        let cancel = body.find("_ = &mut cancel => true").unwrap();
        let sleep = body.find("tokio::time::sleep(start_delay)").unwrap();
        let first_run = body.find("run_snapshot_once(").unwrap();
        let follow_up_gate = body.find("runtime_was_already_running(&outcome)").unwrap();
        let idle = body.find("flights.wait_idle(&mut cancel)").unwrap();
        assert!(select < cancel && cancel < sleep && sleep < first_run);
        assert!(first_run < follow_up_gate && follow_up_gate < idle);
        assert_eq!(body.matches("run_snapshot_once(").count(), 3);

        let once = production.find("async fn run_snapshot_once(").unwrap();
        let once_body = &production[once..];
        assert!(
            once_body.find("flights.enter()").unwrap() < once_body.find("runner.run()").unwrap()
        );
    }

    #[test]
    fn lifecycle_observer_receives_running_and_every_recorded_terminal_outcome() {
        let source = include_str!("creator_snapshot_supervisor.rs").replace("\r\n", "\n");
        let production = &source[..source.find("#[cfg(test)]\nmod tests").unwrap()];
        let trait_start = production
            .find("pub(super) trait CreatorSnapshotLifecycleObserver")
            .unwrap();
        let trait_end = production[trait_start..]
            .find("pub(super) enum CreatorSnapshotTaskOutcome")
            .map(|offset| trait_start + offset)
            .unwrap();
        let observer = &production[trait_start..trait_end];
        assert!(observer.contains("fn creator_sync_credential_invalidated("));
        assert!(observer.contains("fn creator_snapshot_state_changed("));

        let trigger = production.find("pub(super) async fn trigger(").unwrap();
        let close = production.find("pub(super) fn close_intake(").unwrap();
        let body = &production[trigger..close];
        let running = body
            .find("creator_snapshot_state_changed(run_id, CreatorSnapshotTaskOutcome::Running)")
            .unwrap();
        let supervised = body.find("supervise_snapshot(").unwrap();
        let recorded = body
            .find("record_terminal(run_id, recorded_at, outcome.clone())")
            .unwrap();
        let reported = body
            .find("creator_snapshot_state_changed(run_id, outcome.clone())")
            .unwrap();
        let invalidated = body.find("creator_sync_credential_invalidated()").unwrap();
        assert!(running < supervised && supervised < recorded);
        assert!(recorded < reported && reported < invalidated);

        let join = production.find("async fn join_owned_task(").unwrap();
        let join_body = &production[join..];
        assert!(
            join_body
                .find("record_terminal(run_id, task.started_at, outcome.clone())")
                .unwrap()
                < join_body
                    .find("creator_snapshot_state_changed(run_id, outcome.clone())")
                    .unwrap()
        );
    }

    #[test]
    fn runtime_snapshot_sync_prechecks_empty_store_before_exporting_on_the_blocking_pool() {
        let source = include_str!("creator_snapshot_supervisor.rs");
        let start = source.find("async fn sync_creator_snapshot(").unwrap();
        let end = source[start..]
            .find("#[cfg(test)]")
            .map(|offset| start + offset)
            .unwrap();
        let body = &source[start..end];
        let admission = body.find("can_sync_now").unwrap();
        let export = body.find("export_creator_bundle").unwrap();
        assert!(admission < export);

        let helper = body.find("async fn load_creator_snapshot_bundle").unwrap();
        let helper_body = &body[helper..];
        let precheck = helper_body.find("list_creators_async").unwrap();
        let empty = helper_body.find("precheck.items.is_empty()").unwrap();
        let blocking = helper_body.find("spawn_blocking").unwrap();
        let export = helper_body.find("export(creators)").unwrap();
        assert!(precheck < empty && empty < blocking && blocking < export);
        assert!(helper_body.contains("limit: Some(1)"));
        assert!(helper_body.contains("include_archived: true"));
    }
}
