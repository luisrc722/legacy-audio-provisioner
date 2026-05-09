use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use crate::crypto::compute_file_sha256;
use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

const JOURNAL_VERSION: u32 = 1;
const JOURNAL_FILENAME: &str = ".legacy_journal.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionStatus {
    Pending,
    InProgress,
    Committed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveTransaction {
    pub source_rel: PathBuf,
    pub target_rel: PathBuf,
    pub expected_hash: String,
    pub status: TransactionStatus,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyJournal {
    pub version: u32,
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
    pub transactions: Vec<MoveTransaction>,
}

#[derive(Debug, Clone, Default)]
pub struct JournalSummary {
    pub total: usize,
    pub committed: usize,
    pub pending: usize,
    pub failed: usize,
}

pub struct JournalManager {
    path: PathBuf,
    data: LegacyJournal,
}

fn write_json_atomically(path: &Path, data: &LegacyJournal) -> Result<()> {
    let tmp_path = path.with_extension("tmp");
    let serialized = serde_json::to_string_pretty(data)?;

    let mut tmp_file = File::create(&tmp_path)?;
    tmp_file.write_all(serialized.as_bytes())?;
    tmp_file.sync_all()?;

    fs::rename(&tmp_path, path)?;
    if let Some(parent) = path.parent() {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    Ok(())
}

impl JournalManager {
    pub fn load_or_create_at(path: &Path) -> Result<Self> {
        let path = path.to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let data: LegacyJournal = serde_json::from_str(&content)?;
            if data.version != JOURNAL_VERSION {
                return Err(anyhow!(
                    "Journal version mismatch: expected {}, got {}",
                    JOURNAL_VERSION,
                    data.version
                ));
            }
            return Ok(Self { path, data });
        }

        let data = LegacyJournal {
            version: JOURNAL_VERSION,
            session_id: deterministic_journal_session_id(&path),
            created_at: Utc::now(),
            last_updated: Utc::now(),
            transactions: Vec::new(),
        };

        let mgr = Self { path, data };
        mgr.save()?;
        Ok(mgr)
    }

    pub fn load_or_create(usb_mount: &Path) -> Result<Self> {
        let path = usb_mount.join(JOURNAL_FILENAME);
        Self::load_or_create_at(&path)
    }

    pub fn save(&self) -> Result<()> {
        let mut data = self.data.clone();
        data.last_updated = Utc::now();
        write_json_atomically(&self.path, &data)
    }

    pub fn clear_from_path(path: &Path) -> Result<()> {
        if path.exists() {
            fs::remove_file(path)?;
            if let Some(parent) = path.parent() {
                if let Ok(dir) = File::open(parent) {
                    let _ = dir.sync_all();
                }
            }
        }
        Ok(())
    }

    pub fn clear_from_usb(usb_mount: &Path) -> Result<()> {
        let path = usb_mount.join(JOURNAL_FILENAME);
        if path.exists() {
            fs::remove_file(&path)?;
            if let Ok(dir) = File::open(usb_mount) {
                let _ = dir.sync_all();
            }
        }
        Ok(())
    }

    pub fn summary(&self) -> JournalSummary {
        let mut summary = JournalSummary {
            total: self.data.transactions.len(),
            ..JournalSummary::default()
        };

        for tx in &self.data.transactions {
            match tx.status {
                TransactionStatus::Committed => summary.committed += 1,
                TransactionStatus::Failed => summary.failed += 1,
                _ => summary.pending += 1,
            }
        }

        summary
    }

    pub fn reconcile(&mut self, usb_mount: &Path) -> Result<JournalSummary> {
        for tx in &mut self.data.transactions {
            if tx.status == TransactionStatus::Committed {
                continue;
            }

            let abs_source = usb_mount.join(&tx.source_rel);
            let abs_target = usb_mount.join(&tx.target_rel);

            if abs_target.exists() {
                let target_hash = compute_file_sha256(&abs_target)?;
                if target_hash == tx.expected_hash {
                    tx.status = TransactionStatus::Committed;
                    tx.last_error = None;
                    continue;
                }
            }

            if abs_source.exists() {
                tx.status = TransactionStatus::Pending;
                continue;
            }

            tx.status = TransactionStatus::Failed;
            tx.last_error = Some("Source missing and target hash invalid".to_string());
        }

        self.save()?;
        Ok(self.summary())
    }

    pub fn ensure_move_transaction(
        &mut self,
        source_rel: PathBuf,
        target_rel: PathBuf,
        expected_hash: String,
    ) -> Result<()> {
        if let Some(existing) = self
            .data
            .transactions
            .iter_mut()
            .find(|t| t.target_rel == target_rel)
        {
            existing.source_rel = source_rel;
            existing.expected_hash = expected_hash;
            if existing.status != TransactionStatus::Committed {
                existing.status = TransactionStatus::Pending;
                existing.last_error = None;
            }
        } else {
            self.data.transactions.push(MoveTransaction {
                source_rel,
                target_rel,
                expected_hash,
                status: TransactionStatus::Pending,
                last_error: None,
            });
        }

        self.save()
    }

    pub fn status_for_target(&self, target_rel: &Path) -> Option<TransactionStatus> {
        self.data
            .transactions
            .iter()
            .find(|t| t.target_rel == target_rel)
            .map(|t| t.status.clone())
    }

    pub fn mark_in_progress(&mut self, target_rel: &Path) -> Result<()> {
        if let Some(tx) = self
            .data
            .transactions
            .iter_mut()
            .find(|t| t.target_rel == target_rel)
        {
            tx.status = TransactionStatus::InProgress;
            tx.last_error = None;
            return self.save();
        }
        Err(anyhow!(
            "Transaction not found for target {}",
            target_rel.display()
        ))
    }

    pub fn mark_committed(&mut self, target_rel: &Path) -> Result<()> {
        if let Some(tx) = self
            .data
            .transactions
            .iter_mut()
            .find(|t| t.target_rel == target_rel)
        {
            tx.status = TransactionStatus::Committed;
            tx.last_error = None;
            return self.save();
        }
        Err(anyhow!(
            "Transaction not found for target {}",
            target_rel.display()
        ))
    }

    pub fn mark_failed(&mut self, target_rel: &Path, message: String) -> Result<()> {
        if let Some(tx) = self
            .data
            .transactions
            .iter_mut()
            .find(|t| t.target_rel == target_rel)
        {
            tx.status = TransactionStatus::Failed;
            tx.last_error = Some(message);
            return self.save();
        }
        Err(anyhow!(
            "Transaction not found for target {}",
            target_rel.display()
        ))
    }

    pub fn all_committed(&self) -> bool {
        !self.data.transactions.is_empty()
            && self
                .data
                .transactions
                .iter()
                .all(|t| t.status == TransactionStatus::Committed)
    }
}

fn deterministic_journal_session_id(path: &Path) -> String {
    let key = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string();
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    format!("journal_{}", &hash[..12])
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_journal_transaction_lifecycle() -> Result<()> {
        let usb = TempDir::new()?;
        let journal_file = usb.path().join("journal.json");
        let source = usb.path().join("musica/raw.mp3");
        let target = usb.path().join("VOL_01/001_raw.mp3");

        fs::create_dir_all(source.parent().expect("source parent"))?;
        fs::write(&source, b"audio")?;

        let mut mgr = JournalManager::load_or_create_at(&journal_file)?;
        let expected_hash = compute_file_sha256(&source)?;
        mgr.ensure_move_transaction(
            PathBuf::from("musica/raw.mp3"),
            PathBuf::from("VOL_01/001_raw.mp3"),
            expected_hash,
        )?;

        mgr.mark_in_progress(Path::new("VOL_01/001_raw.mp3"))?;
        fs::create_dir_all(target.parent().expect("target parent"))?;
        fs::rename(&source, &target)?;
        mgr.mark_committed(Path::new("VOL_01/001_raw.mp3"))?;

        let summary = mgr.summary();
        assert_eq!(summary.total, 1);
        assert_eq!(summary.committed, 1);
        assert!(mgr.all_committed());

        Ok(())
    }

    #[test]
    fn test_journal_reconcile_marks_committed_from_target_hash() -> Result<()> {
        let usb = TempDir::new()?;
        let journal_file = usb.path().join("journal.json");
        let source = usb.path().join("musica/raw.mp3");
        let target = usb.path().join("VOL_01/001_raw.mp3");

        fs::create_dir_all(source.parent().expect("source parent"))?;
        fs::create_dir_all(target.parent().expect("target parent"))?;
        fs::write(&source, b"audio")?;

        let expected_hash = compute_file_sha256(&source)?;
        let mut mgr = JournalManager::load_or_create_at(&journal_file)?;
        mgr.ensure_move_transaction(
            PathBuf::from("musica/raw.mp3"),
            PathBuf::from("VOL_01/001_raw.mp3"),
            expected_hash,
        )?;

        fs::rename(&source, &target)?;

        let summary = mgr.reconcile(usb.path())?;
        assert_eq!(summary.committed, 1);
        assert!(mgr.all_committed());

        Ok(())
    }
}
