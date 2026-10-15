//! Local aggregate timings; no page text, credentials or contact details.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

static WRITE: Mutex<()> = Mutex::new(());
static FLUSH: Mutex<()> = Mutex::new(());
static CLOCK: std::sync::LazyLock<std::time::Instant> =
    std::sync::LazyLock::new(std::time::Instant::now);
static PENDING: Mutex<Vec<(PathBuf, String, Pending)>> = Mutex::new(Vec::new());
enum Pending {
    Duration(TraceEvent),
    Failure(ReadFailure),
}

#[derive(Serialize, Deserialize)]
struct TraceEvent {
    phase: String,
    start_ms: u64,
    end_ms: u64,
    relative_start_ms: u64,
    relative_end_ms: u64,
    process_id: u32,
    target_hash: Option<String>,
}

pub(crate) fn enqueue(
    dir: &Path,
    campaign: &str,
    phase: &str,
    elapsed_ms: u64,
    target: Option<&str>,
) {
    let end_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let relative_end_ms = CLOCK.elapsed().as_millis() as u64;
    let event = TraceEvent {
        phase: phase.into(),
        start_ms: end_ms.saturating_sub(elapsed_ms),
        end_ms,
        relative_start_ms: relative_end_ms.saturating_sub(elapsed_ms),
        relative_end_ms,
        process_id: std::process::id(),
        target_hash: target.map(|id| {
            format!(
                "{:x}",
                Sha256::digest(format!("{campaign}:{id}").as_bytes())
            )
        }),
    };
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).push((
        dir.into(),
        campaign.into(),
        Pending::Duration(event),
    ));
}

pub(crate) fn enqueue_failure(dir: &Path, campaign: &str, failure: ReadFailure) {
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).push((
        dir.into(),
        campaign.into(),
        Pending::Failure(failure),
    ));
}

/// Called on the blocking pool, drained before the execution owner finishes.
pub(crate) fn flush_pending() -> std::io::Result<()> {
    let _flush_guard = FLUSH.lock().unwrap_or_else(|e| e.into_inner());
    let rows = std::mem::take(&mut *PENDING.lock().unwrap_or_else(|e| e.into_inner()));
    let mut batches = BTreeMap::<(PathBuf, String), Vec<Pending>>::new();
    for (dir, campaign, event) in rows {
        batches.entry((dir, campaign)).or_default().push(event);
    }
    let mut first_error = None;
    for ((dir, campaign), events) in batches {
        let result = (|| -> std::io::Result<()> {
            let mut durations = Vec::new();
            update(&dir, &campaign, |metrics| {
                for event in events {
                    match event {
                        Pending::Failure(failure) => {
                            metrics.failure_count = metrics
                                .failure_count
                                .max(metrics.failures.len())
                                .saturating_add(1);
                            if metrics.failures.len() == 200 {
                                metrics.failures.remove(0);
                            }
                            metrics.failures.push(failure);
                        }
                        Pending::Duration(event) => {
                            let entry = metrics.phases.entry(event.phase.clone()).or_default();
                            entry.calls = entry.calls.saturating_add(1);
                            entry.elapsed_ms = entry
                                .elapsed_ms
                                .saturating_add(event.end_ms.saturating_sub(event.start_ms));
                            durations.push(event);
                        }
                    }
                }
            })?;
            if !durations.is_empty() {
                let folder = dir.join("discovery-metrics");
                let path = folder.join(format!(
                    "{:x}.events.jsonl",
                    Sha256::digest(campaign.as_bytes())
                ));
                let _guard = WRITE.lock().unwrap_or_else(|e| e.into_inner());
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)?;
                use std::io::Write;
                for event in durations {
                    serde_json::to_writer(&mut file, &event)?;
                    file.write_all(b"\n")?;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

pub(crate) fn failure_count(dir: &Path, campaign: &str) -> std::io::Result<usize> {
    let path = dir
        .join("discovery-metrics")
        .join(format!("{:x}.json", Sha256::digest(campaign.as_bytes())));
    match std::fs::read(path) {
        Ok(bytes) => {
            let metrics: Metrics = serde_json::from_slice(&bytes)?;
            Ok(metrics.failure_count.max(metrics.failures.len()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(e),
    }
}
#[derive(Default, Serialize, Deserialize)]
struct Phase {
    calls: u64,
    elapsed_ms: u64,
}
#[derive(Default, Serialize, Deserialize)]
struct Metrics {
    campaign_id: String,
    phases: BTreeMap<String, Phase>,
    #[serde(default)]
    failures: Vec<ReadFailure>,
    #[serde(default)]
    failure_count: usize,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ReadFailure {
    pub recorded_at: u64,
    pub step: String,
    pub platform: Option<String>,
    pub target_id: Option<String>,
    pub call_number: Option<u32>,
    pub code: String,
}

#[cfg(test)]
pub(crate) fn record(
    dir: &Path,
    campaign: &str,
    phase: &str,
    elapsed_ms: u64,
) -> std::io::Result<()> {
    update(dir, campaign, |metrics| {
        let entry = metrics.phases.entry(phase.into()).or_default();
        entry.calls = entry.calls.saturating_add(1);
        entry.elapsed_ms = entry.elapsed_ms.saturating_add(elapsed_ms);
    })
}

#[cfg(test)]
pub(crate) fn record_failure(
    dir: &Path,
    campaign: &str,
    failure: ReadFailure,
) -> std::io::Result<()> {
    update(dir, campaign, |metrics| {
        metrics.failure_count = metrics
            .failure_count
            .max(metrics.failures.len())
            .saturating_add(1);
        if metrics.failures.len() == 200 {
            metrics.failures.remove(0);
        }
        metrics.failures.push(failure);
    })
}

fn update(dir: &Path, campaign: &str, apply: impl FnOnce(&mut Metrics)) -> std::io::Result<()> {
    let _guard = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let dir = dir.join("discovery-metrics");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{:x}.json", Sha256::digest(campaign.as_bytes())));
    let mut metrics: Metrics = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Metrics {
            campaign_id: campaign.into(),
            ..Default::default()
        },
        Err(e) => return Err(e),
    };
    apply(&mut metrics);
    let mut temp = tempfile::NamedTempFile::new_in(&dir)?;
    serde_json::to_writer(&mut temp, &metrics)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queued_intervals_preserve_durations_and_anonymize_targets() {
        let dir = tempfile::tempdir().unwrap();
        enqueue(dir.path(), "timeline", "pipeline.started", 0, None);
        enqueue(
            dir.path(),
            "timeline",
            "score.model",
            40,
            Some("author-private"),
        );
        enqueue(
            dir.path(),
            "timeline",
            "read.profile",
            20,
            Some("author-private"),
        );
        // Other parallel tests can enqueue diagnostics for already removed temp dirs.
        drop(flush_pending());
        let path = dir
            .path()
            .join("discovery-metrics")
            .join(format!("{:x}.events.jsonl", Sha256::digest(b"timeline")));
        let text = std::fs::read_to_string(path).unwrap();
        assert!(!text.contains("author-private"));
        let events: Vec<TraceEvent> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(events.len(), 3);
        assert_eq!(events[1].end_ms - events[1].start_ms, 40);
        assert_eq!(events[2].end_ms - events[2].start_ms, 20);
        assert_eq!(events[1].target_hash, events[2].target_hash);
    }
    #[test]
    fn failure_diagnostics_preserve_legacy_metrics_and_survive_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let metrics_dir = dir.path().join("discovery-metrics");
        std::fs::create_dir(&metrics_dir).unwrap();
        let path = metrics_dir.join(format!("{:x}.json", Sha256::digest(b"campaign")));
        std::fs::write(
            &path,
            br#"{"campaign_id":"campaign","phases":{"read":{"calls":3,"elapsed_ms":20}}}"#,
        )
        .unwrap();
        for number in 1..=205 {
            record_failure(
                dir.path(),
                "campaign",
                ReadFailure {
                    recorded_at: 100,
                    step: "profile".into(),
                    platform: Some("tiktok".into()),
                    target_id: Some("hit-id".into()),
                    call_number: Some(number),
                    code: "opencli_invalid_output".into(),
                },
            )
            .unwrap();
        }
        record(dir.path(), "campaign", "read", 10).unwrap();
        let m: Metrics = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(m.phases["read"].calls, 4);
        assert_eq!(m.failures.len(), 200);
        assert_eq!(m.failure_count, 205);
        assert_eq!(m.failures[0].call_number, Some(6));
        assert_eq!(m.failures[199].code, "opencli_invalid_output");
    }
    #[test]
    fn discovery_metrics_survive_reopen_and_keep_campaigns_separate() {
        let dir = tempfile::tempdir().unwrap();
        record(dir.path(), "../../campaign-a", "read", 20).unwrap();
        record(dir.path(), "../../campaign-a", "read", 30).unwrap();
        record(dir.path(), "campaign-b", "score", 50).unwrap();
        let files: Vec<_> = std::fs::read_dir(dir.path().join("discovery-metrics"))
            .unwrap()
            .collect();
        assert_eq!(files.len(), 2);
        for file in files {
            let m: Metrics =
                serde_json::from_slice(&std::fs::read(file.unwrap().path()).unwrap()).unwrap();
            if m.campaign_id == "../../campaign-a" {
                assert_eq!(m.phases["read"].calls, 2);
                assert_eq!(m.phases["read"].elapsed_ms, 50);
            } else {
                assert_eq!(m.phases["score"].calls, 1);
            }
        }
    }
}
