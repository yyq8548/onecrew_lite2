//! Resume only prepared, unstarted scores. Never persist evidence or credentials.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io,
    path::{Path, PathBuf},
    sync::Mutex,
};

static WRITE: Mutex<()> = Mutex::new(());
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct SavedScore {
    pub hit: String,
    pub deferred_platforms: Vec<String>,
}
pub(super) struct SavedScores(PathBuf);
impl SavedScores {
    pub(super) fn new(dir: &Path, account: &str, campaign: &str, revision: u32) -> Self {
        let key = serde_json::to_vec(&(account, campaign, revision)).expect("scalar tuple");
        Self(
            dir.join("discovery-score-queue")
                .join(format!("{:x}.json", Sha256::digest(key))),
        )
    }
    pub(super) fn load(&self) -> io::Result<Vec<SavedScore>> {
        let _guard = WRITE.lock().unwrap_or_else(|e| e.into_inner());
        self.read()
    }
    fn read(&self) -> io::Result<Vec<SavedScore>> {
        match std::fs::read(&self.0) {
            Ok(bytes) => {
                let rows: Vec<SavedScore> = serde_json::from_slice(&bytes)?;
                if rows.len() > 5 {
                    return Err(io::Error::other("score queue exceeds capacity"));
                }
                Ok(rows)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(vec![]),
            Err(e) => Err(e),
        }
    }
    fn update(&self, apply: impl FnOnce(&mut Vec<SavedScore>)) -> io::Result<()> {
        let _guard = WRITE.lock().unwrap_or_else(|e| e.into_inner());
        let mut rows = self.read()?;
        let previous = rows.clone();
        apply(&mut rows);
        if rows == previous {
            return Ok(());
        }
        if rows.len() > 5 {
            return Err(io::Error::other("score queue exceeds capacity"));
        }
        let dir = self.0.parent().expect("queue parent");
        std::fs::create_dir_all(dir)?;
        let mut temp = tempfile::NamedTempFile::new_in(dir)?;
        serde_json::to_writer(&mut temp, &rows)?;
        temp.as_file().sync_all()?;
        temp.persist(&self.0).map_err(|e| e.error)?;
        Ok(())
    }
    pub(super) fn add(&self, row: SavedScore) -> io::Result<()> {
        self.update(|rows| {
            rows.retain(|old| old.hit != row.hit);
            rows.push(row);
        })
    }
    pub(super) fn remove(&self, hit: &str) -> io::Result<()> {
        self.update(|rows| rows.retain(|row| row.hit != hit))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resume_keeps_only_unstarted_work_isolated_by_account_and_revision() {
        let dir = tempfile::tempdir().unwrap();
        let queue = SavedScores::new(dir.path(), "a", "campaign", 1);
        for hit in ["first", "second"] {
            queue
                .add(SavedScore {
                    hit: hit.into(),
                    deferred_platforms: vec!["youtube".into()],
                })
                .unwrap();
        }
        queue.remove("first").unwrap(); // Mark before dispatch, never retry unknown work.
        let restored = SavedScores::new(dir.path(), "a", "campaign", 1)
            .load()
            .unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].hit, "second");
        assert!(
            SavedScores::new(dir.path(), "b", "campaign", 1)
                .load()
                .unwrap()
                .is_empty()
        );
        assert!(
            SavedScores::new(dir.path(), "a", "campaign", 2)
                .load()
                .unwrap()
                .is_empty()
        );
        queue.remove("second").unwrap();
        assert!(queue.load().unwrap().is_empty());
    }
    #[test]
    fn malformed_or_overfull_queue_fails_without_losing_saved_work() {
        let dir = tempfile::tempdir().unwrap();
        let queue = SavedScores::new(dir.path(), "a", "../campaign", 1);
        for i in 0..5 {
            queue
                .add(SavedScore {
                    hit: i.to_string(),
                    deferred_platforms: vec![],
                })
                .unwrap();
        }
        assert!(
            queue
                .add(SavedScore {
                    hit: "overflow".into(),
                    deferred_platforms: vec![]
                })
                .is_err()
        );
        assert_eq!(queue.load().unwrap().len(), 5);
        std::fs::write(&queue.0, "invalid").unwrap();
        assert!(queue.load().is_err());
    }
}
