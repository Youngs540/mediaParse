use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use mediaparse_core::{PlanItem, PlanStatus, RenamePlan};

use crate::db::DbError;
use crate::journal::{EntryStatus, Journal, JournalEntry, JournalKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyReport {
    pub run_id: i64,
    pub renamed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoReport {
    pub run_id: i64,
    pub restored: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorProgress {
    pub done: usize,
    pub total: usize,
    pub current: String,
}

#[derive(Clone)]
pub struct Executor {
    journal: Arc<Journal>,
    verify_copies: bool,
}

impl Executor {
    pub fn new(journal: Arc<Journal>) -> Self {
        Self {
            journal,
            verify_copies: true,
        }
    }

    pub fn without_copy_verification(journal: Arc<Journal>) -> Self {
        Self {
            journal,
            verify_copies: false,
        }
    }

    pub fn apply(
        &self,
        plan: &RenamePlan,
        selected: &[String],
        progress: impl Fn(ExecutorProgress) + Send + Sync,
    ) -> Result<ApplyReport, ExecutorError> {
        let mut selected: HashSet<String> = selected.iter().cloned().collect();
        // Selecting a video also selects its sidecars.
        for item in &plan.items {
            if let Some(parent) = &item.parent_id {
                if selected.contains(parent) {
                    selected.insert(item.id.clone());
                }
            }
        }

        let candidates: Vec<&PlanItem> = plan
            .items
            .iter()
            .filter(|item| selected.contains(&item.id))
            .collect();
        let actionable: Vec<&PlanItem> = candidates
            .iter()
            .copied()
            .filter(|item| matches!(item.status, PlanStatus::Ready | PlanStatus::Review))
            .collect();
        let mut skipped = candidates.len() - actionable.len();

        let run_id = self.journal.begin_run(&plan.root, actionable.len())?;
        let mut renamed = 0usize;
        let mut failed = 0usize;
        let mut errors = Vec::new();
        let mut seq = 0i64;
        let total = actionable.len();

        for (index, item) in actionable.iter().enumerate() {
            let result = self.apply_item(run_id, &mut seq, item);
            match result {
                Ok(ApplyOutcome::Renamed) => renamed += 1,
                Ok(ApplyOutcome::Skipped(reason)) => {
                    skipped += 1;
                    if let Some(reason) = reason {
                        errors.push(format!("{}: {reason}", item.source.display()));
                    }
                }
                Err(error) => {
                    failed += 1;
                    errors.push(format!("{}: {error}", item.source.display()));
                }
            }
            progress(ExecutorProgress {
                done: index + 1,
                total,
                current: item.target.display().to_string(),
            });
        }

        let status = if failed == 0 { "completed" } else { "partial" };
        self.journal.finish_run(run_id, status, renamed, failed)?;
        Ok(ApplyReport {
            run_id,
            renamed,
            failed,
            skipped,
            errors,
        })
    }

    fn apply_item(
        &self,
        run_id: i64,
        seq: &mut i64,
        item: &PlanItem,
    ) -> Result<ApplyOutcome, ExecutorError> {
        if !item.source.exists() {
            return Err(ExecutorError::Message(format!(
                "source does not exist: {}",
                item.source.display()
            )));
        }
        if item.source == item.target {
            return Ok(ApplyOutcome::Skipped(Some(
                "already correctly named".to_string(),
            )));
        }

        let target_exists = item.target.exists();
        if target_exists && !item.case_only {
            return Err(ExecutorError::Message(format!(
                "target already exists: {}",
                item.target.display()
            )));
        }
        if item.target.is_dir() {
            return Err(ExecutorError::Message(format!(
                "target is a directory: {}",
                item.target.display()
            )));
        }

        let parent = item
            .target
            .parent()
            .ok_or_else(|| ExecutorError::Message("target has no parent".to_string()))?;
        self.ensure_dirs(run_id, seq, parent)?;

        if item.case_only {
            self.case_rename(run_id, seq, &item.source, &item.target)?;
            return Ok(ApplyOutcome::Renamed);
        }

        match fs::rename(&item.source, &item.target) {
            Ok(()) => {
                let entry = self.journal.record_intent(
                    run_id,
                    *seq,
                    JournalKind::Rename,
                    &item.source,
                    &item.target,
                    None,
                )?;
                *seq += 1;
                self.journal.mark_done(entry)?;
                Ok(ApplyOutcome::Renamed)
            }
            Err(error) if is_cross_device(&error) => {
                self.copy_verify_delete(run_id, seq, &item.source, &item.target)?;
                Ok(ApplyOutcome::Renamed)
            }
            Err(error) if is_permission_denied(&error) => {
                make_writable(&item.source);
                fs::rename(&item.source, &item.target).map_err(ExecutorError::Io)?;
                let entry = self.journal.record_intent(
                    run_id,
                    *seq,
                    JournalKind::Rename,
                    &item.source,
                    &item.target,
                    None,
                )?;
                *seq += 1;
                self.journal.mark_done(entry)?;
                Ok(ApplyOutcome::Renamed)
            }
            Err(error) => Err(ExecutorError::Io(error)),
        }
    }

    fn ensure_dirs(&self, run_id: i64, seq: &mut i64, parent: &Path) -> Result<(), ExecutorError> {
        let mut missing = Vec::new();
        let mut current = Some(parent);
        while let Some(dir) = current {
            if dir.exists() {
                break;
            }
            missing.push(dir.to_path_buf());
            current = dir.parent();
        }
        for dir in missing.into_iter().rev() {
            fs::create_dir(&dir)?;
            let entry =
                self.journal
                    .record_intent(run_id, *seq, JournalKind::Mkdir, &dir, &dir, None)?;
            *seq += 1;
            self.journal.mark_done(entry)?;
        }
        Ok(())
    }

    fn case_rename(
        &self,
        run_id: i64,
        seq: &mut i64,
        source: &Path,
        target: &Path,
    ) -> Result<(), ExecutorError> {
        let temp = unique_temp_path(target);
        let entry = self.journal.record_intent(
            run_id,
            *seq,
            JournalKind::CaseRename,
            source,
            target,
            Some(&temp),
        )?;
        *seq += 1;
        if let Err(error) = fs::rename(source, &temp) {
            let _ = self.journal.mark_failed(entry, &error.to_string());
            return Err(ExecutorError::Io(error));
        }
        if let Err(error) = fs::rename(&temp, target) {
            // Best-effort rollback so the source is not left under a temp name.
            let _ = fs::rename(&temp, source);
            let _ = self.journal.mark_failed(entry, &error.to_string());
            return Err(ExecutorError::Io(error));
        }
        self.journal.mark_done(entry)?;
        Ok(())
    }

    fn copy_verify_delete(
        &self,
        run_id: i64,
        seq: &mut i64,
        source: &Path,
        target: &Path,
    ) -> Result<(), ExecutorError> {
        let temp = unique_temp_path(target);
        let entry = self.journal.record_intent(
            run_id,
            *seq,
            JournalKind::CopyDelete,
            source,
            target,
            Some(&temp),
        )?;
        *seq += 1;

        let result = copy_verify_delete_raw(source, target, &temp, self.verify_copies);
        match result {
            Ok(()) => {
                self.journal.mark_done(entry)?;
                Ok(())
            }
            Err(error) => {
                let _ = fs::remove_file(&temp);
                let _ = self.journal.mark_failed(entry, &error.to_string());
                Err(ExecutorError::Io(error))
            }
        }
    }

    pub fn undo_last(&self) -> Result<UndoReport, ExecutorError> {
        let run = self
            .journal
            .last_undoable_run()?
            .ok_or(ExecutorError::NoUndoableRun)?;
        self.undo_run(run.id)
    }

    pub fn undo_run(&self, run_id: i64) -> Result<UndoReport, ExecutorError> {
        let entries = self.journal.entries_for_run(run_id)?;
        let mut restored = 0usize;
        let mut failed = 0usize;
        let mut errors = Vec::new();

        for entry in entries {
            if entry.status == EntryStatus::RolledBack {
                continue;
            }
            match self.undo_entry(&entry) {
                Ok(()) => {
                    if entry.kind != JournalKind::Mkdir {
                        restored += 1;
                    }
                    self.journal.mark_rolled_back(entry.id)?;
                }
                Err(error) => {
                    failed += 1;
                    errors.push(format!(
                        "{} -> {}: {error}",
                        entry.target.display(),
                        entry.source.display()
                    ));
                }
            }
        }

        let status = if failed == 0 {
            "undone"
        } else {
            "undo_partial"
        };
        self.journal.set_run_status(run_id, status)?;
        Ok(UndoReport {
            run_id,
            restored,
            failed,
            errors,
        })
    }

    fn undo_entry(&self, entry: &JournalEntry) -> Result<(), ExecutorError> {
        match entry.kind {
            JournalKind::Mkdir => {
                if entry.target.exists() {
                    fs::remove_dir(&entry.target)?;
                }
                Ok(())
            }
            JournalKind::Rename => {
                if entry.target.exists() {
                    move_path(&entry.target, &entry.source)?;
                } else if !entry.source.exists() {
                    return Err(ExecutorError::Message(
                        "neither target nor source exists".to_string(),
                    ));
                }
                Ok(())
            }
            JournalKind::CopyDelete => {
                if entry.target.exists() {
                    move_path(&entry.target, &entry.source)?;
                } else if !entry.source.exists() {
                    if let Some(temp) = &entry.temp {
                        let _ = fs::remove_file(temp);
                    }
                    return Err(ExecutorError::Message(
                        "neither target nor source exists".to_string(),
                    ));
                }
                Ok(())
            }
            JournalKind::CaseRename => {
                if entry.target.exists() {
                    case_move(&entry.target, &entry.source)?;
                } else if let Some(temp) = &entry.temp {
                    if temp.exists() && !entry.source.exists() {
                        fs::rename(temp, &entry.source)?;
                    } else if !entry.source.exists() {
                        return Err(ExecutorError::Message(
                            "case-rename temp file is missing".to_string(),
                        ));
                    }
                } else if !entry.source.exists() {
                    return Err(ExecutorError::Message(
                        "neither target nor source exists".to_string(),
                    ));
                }
                Ok(())
            }
        }
    }
}

enum ApplyOutcome {
    Renamed,
    Skipped(Option<String>),
}

fn copy_verify_delete_raw(
    source: &Path,
    target: &Path,
    temp: &Path,
    verify: bool,
) -> io::Result<()> {
    fs::copy(source, temp)?;
    let source_len = fs::metadata(source)?.len();
    let copied_len = fs::metadata(temp)?.len();
    if source_len != copied_len {
        return Err(io::Error::other(format!(
            "copy length mismatch: {source_len} != {copied_len}"
        )));
    }
    if verify {
        let source_hash = hash_file(source)?;
        let copied_hash = hash_file(temp)?;
        if source_hash != copied_hash {
            return Err(io::Error::other(
                "copy verification failed: content hash mismatch",
            ));
        }
    }
    fs::rename(temp, target)?;
    make_writable(source);
    fs::remove_file(source)?;
    Ok(())
}

fn move_path(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(error) if is_cross_device(&error) => {
            let temp = unique_temp_path(to);
            let result = copy_verify_delete_raw(from, to, &temp, true);
            if result.is_err() {
                let _ = fs::remove_file(&temp);
            }
            result
        }
        Err(error) if is_permission_denied(&error) => {
            make_writable(from);
            fs::rename(from, to)
        }
        Err(error) => Err(error),
    }
}

fn case_move(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = unique_temp_path(to);
    fs::rename(from, &temp)?;
    if let Err(error) = fs::rename(&temp, to) {
        let _ = fs::rename(&temp, from);
        return Err(error);
    }
    Ok(())
}

fn hash_file(path: &Path) -> io::Result<blake3::Hash> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize())
}

fn unique_temp_path(target: &Path) -> PathBuf {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let mut counter = 0u32;
    loop {
        let candidate = parent.join(format!(
            ".{name}.mediaparse-{}-{nanos}-{counter}.tmp",
            std::process::id()
        ));
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

fn make_writable(path: &Path) {
    #[cfg(windows)]
    {
        if let Ok(metadata) = fs::metadata(path) {
            let mut permissions = metadata.permissions();
            if permissions.readonly() {
                permissions.set_readonly(false);
                let _ = fs::set_permissions(path, permissions);
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
    }
}

#[cfg(unix)]
fn is_cross_device(error: &io::Error) -> bool {
    error.raw_os_error() == Some(18) // EXDEV
}

#[cfg(windows)]
fn is_cross_device(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(17) | Some(1004))
}

#[cfg(not(any(unix, windows)))]
fn is_cross_device(_error: &io::Error) -> bool {
    false
}

fn is_permission_denied(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem
    )
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("journal error: {0}")]
    Journal(#[from] DbError),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Message(String),
    #[error("no undoable run found")]
    NoUndoableRun,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::journal::Journal;
    use mediaparse_core::{ParsedMedia, PlanItem, PlanStatus, RenamePlan};
    use std::sync::Arc;

    fn executor() -> Executor {
        let db = Arc::new(Db::open_in_memory().unwrap());
        Executor::new(Arc::new(Journal::new(db)))
    }

    fn plan(root: &Path, items: Vec<PlanItem>) -> RenamePlan {
        let mut plan = RenamePlan {
            root: root.to_path_buf(),
            items,
            created_at: 1,
            summary: Default::default(),
        };
        plan.recompute_summary();
        plan
    }

    fn item(id: &str, source: PathBuf, target: PathBuf, case_only: bool) -> PlanItem {
        PlanItem {
            id: id.into(),
            parent_id: None,
            source,
            target,
            status: PlanStatus::Ready,
            score: Some(0.99),
            parsed: ParsedMedia::default(),
            candidate: None,
            sidecar: None,
            is_sidecar: false,
            case_only,
            reason: None,
        }
    }

    #[test]
    fn renames_and_undoes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("old.mkv");
        let target = dir.path().join("Movie (2020)").join("Movie (2020).mkv");
        fs::write(&source, b"movie bytes").unwrap();
        let executor = executor();
        let plan = plan(
            dir.path(),
            vec![item("1", source.clone(), target.clone(), false)],
        );

        let report = executor.apply(&plan, &["1".to_string()], |_| {}).unwrap();
        assert_eq!(report.renamed, 1);
        assert!(!source.exists());
        assert!(target.exists());
        assert_eq!(fs::read(&target).unwrap(), b"movie bytes");

        let undo = executor.undo_last().unwrap();
        assert_eq!(undo.restored, 1);
        assert!(source.exists());
        assert!(!target.exists());
    }

    #[test]
    fn refuses_existing_target() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("old.mkv");
        let target = dir.path().join("new.mkv");
        fs::write(&source, b"old").unwrap();
        fs::write(&target, b"new").unwrap();
        let executor = executor();
        let plan = plan(
            dir.path(),
            vec![item("1", source.clone(), target.clone(), false)],
        );
        let report = executor.apply(&plan, &["1".to_string()], |_| {}).unwrap();
        assert_eq!(report.renamed, 0);
        assert_eq!(report.failed, 1);
        assert!(source.exists());
        assert_eq!(fs::read(&target).unwrap(), b"new");
    }

    #[test]
    fn case_only_rename_uses_two_steps() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("movie.mkv");
        let target = dir.path().join("Movie.mkv");
        fs::write(&source, b"case").unwrap();
        let executor = executor();
        let plan = plan(
            dir.path(),
            vec![item("1", source.clone(), target.clone(), true)],
        );
        let report = executor.apply(&plan, &["1".to_string()], |_| {}).unwrap();
        assert_eq!(report.renamed, 1);
        assert!(target.exists());
        let undo = executor.undo_last().unwrap();
        assert_eq!(undo.restored, 1);
        assert!(source.exists());
    }

    #[test]
    fn creates_and_undoes_directories() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("old.mkv");
        let target = dir.path().join("A").join("B").join("new.mkv");
        fs::write(&source, b"x").unwrap();
        let executor = executor();
        let plan = plan(
            dir.path(),
            vec![item("1", source.clone(), target.clone(), false)],
        );
        executor.apply(&plan, &["1".to_string()], |_| {}).unwrap();
        assert!(target.exists());
        assert!(dir.path().join("A").join("B").exists());
        executor.undo_last().unwrap();
        assert!(!dir.path().join("A").join("B").exists());
        assert!(!dir.path().join("A").exists());
    }

    #[test]
    fn selecting_video_expands_to_sidecars() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("old.mkv");
        let subtitle = dir.path().join("old.en.srt");
        fs::write(&video, b"v").unwrap();
        fs::write(&subtitle, b"s").unwrap();
        let video_target = dir.path().join("Movie (2020).mkv");
        let sub_target = dir.path().join("Movie (2020).en.srt");
        let mut sub_item = item("2", subtitle.clone(), sub_target.clone(), false);
        sub_item.parent_id = Some("1".into());
        sub_item.is_sidecar = true;
        let plan = plan(
            dir.path(),
            vec![
                item("1", video.clone(), video_target.clone(), false),
                sub_item,
            ],
        );
        let executor = executor();
        let report = executor.apply(&plan, &["1".to_string()], |_| {}).unwrap();
        assert_eq!(report.renamed, 2);
        assert!(video_target.exists());
        assert!(sub_target.exists());
    }

    #[test]
    fn undo_recovers_intent_left_by_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("old.mkv");
        let target = dir.path().join("new.mkv");
        fs::write(&target, b"already moved").unwrap();
        let db = Arc::new(Db::open_in_memory().unwrap());
        let journal = Arc::new(Journal::new(db));
        let executor = Executor::new(Arc::clone(&journal));
        let run = journal.begin_run(dir.path(), 1).unwrap();
        let entry = journal
            .record_intent(run, 0, JournalKind::Rename, &source, &target, None)
            .unwrap();
        journal.mark_done(entry).unwrap();
        journal.finish_run(run, "completed", 1, 0).unwrap();

        let report = executor.undo_last().unwrap();
        assert_eq!(report.restored, 1);
        assert!(source.exists());
        assert!(!target.exists());
    }

    #[test]
    fn skips_non_actionable_items() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("old.mkv");
        fs::write(&source, b"x").unwrap();
        let mut conflict = item("1", source.clone(), dir.path().join("new.mkv"), false);
        conflict.status = PlanStatus::Conflict;
        let executor = executor();
        let plan = plan(dir.path(), vec![conflict]);
        let report = executor.apply(&plan, &["1".to_string()], |_| {}).unwrap();
        assert_eq!(report.renamed, 0);
        assert_eq!(report.skipped, 1);
        assert!(source.exists());
    }
}
