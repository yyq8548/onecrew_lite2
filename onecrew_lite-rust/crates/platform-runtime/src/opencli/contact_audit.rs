//! Append-only timing/navigation diagnostics. Never store page text or contacts.
use sha2::{Digest, Sha256};
use std::{io::Write, path::Path, sync::Mutex};
static WRITE: Mutex<()> = Mutex::new(());
pub(super) fn record(dir: &Path, campaign: &str, mut event: serde_json::Value) {
    let _guard = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let result = (|| -> std::io::Result<()> {
        let directory = dir.join("contact-enrichment-metrics");
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(format!("{:x}.jsonl", Sha256::digest(campaign.as_bytes())));
        event["recorded_at"] = serde_json::json!(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        serde_json::to_writer(&mut file, &event)?;
        file.write_all(b"\n")
    })();
    if result.is_err() {
        log::warn!("contact enrichment diagnostic write failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn append_metrics_keep_calls_without_contact_text_and_cannot_escape_directory() {
        let dir = tempfile::tempdir().unwrap();
        for n in 1..=2 {
            record(
                dir.path(),
                "../../campaign",
                serde_json::json!({"kind":"lookup","call_number":n,"method":"search","elapsed_ms":12,"raw_results":5,"kept_links":1}),
            );
        }
        let files = std::fs::read_dir(dir.path().join("contact-enrichment-metrics"))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(files.len(), 1);
        let text = std::fs::read_to_string(files[0].path()).unwrap();
        let rows = text
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1]["call_number"], 2);
        assert!(rows[0]["recorded_at"].as_u64().is_some());
    }
}
