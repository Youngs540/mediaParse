use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::db::{now_unix, Db, DbError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JournalKind {
    Rename,
    CopyDelete,
    CaseRename,
    Mkdir,
}

impl JournalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            JournalKind::Rename => "rename",
            JournalKind::CopyDelete => "copy_delete",
            JournalKind::CaseRename => "case_rename",
            JournalKind::Mkdir => "mkdir",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "rename" => Some(Self::Rename),
            "copy_delete" => Some(Self::CopyDelete),
            "case_rename" => Some(Self::CaseRename),
            "mkdir" => Some(Self::Mkdir),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryStatus {
    Intent,
    Done,
    Failed,
    RolledBack,
}

impl EntryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            EntryStatus::Intent => "intent",
            EntryStatus::Done => "done",
            EntryStatus::Failed => "failed",
            EntryStatus::RolledBack => "rolled_back",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "intent" => Some(Self::Intent),
            "done" => Some(Self::Done),
            "failed" => Some(Self::Failed),
            "rolled_back" => Some(Self::RolledBack),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub id: i64,
    pub run_id: i64,
    pub seq: i64,
    pub kind: JournalKind,
    pub source: PathBuf,
    pub target: PathBuf,
    pub temp: Option<PathBuf>,
    pub status: EntryStatus,
    pub error: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub id: i64,
    pub root: PathBuf,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub status: String,
    pub total: i64,
    pub renamed: i64,
    pub failed: i64,
}

#[derive(Clone)]
pub struct Journal {
    db: Arc<Db>,
}

impl Journal {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub fn begin_run(&self, root: &Path, total: usize) -> Result<i64, DbError> {
        let root = root.to_string_lossy().to_string();
        let now = now_unix();
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO journal_runs (root, started_at, status, total)
                 VALUES (?1, ?2, 'running', ?3)",
                rusqlite::params![root, now, total as i64],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn finish_run(
        &self,
        run_id: i64,
        status: &str,
        renamed: usize,
        failed: usize,
    ) -> Result<(), DbError> {
        let now = now_unix();
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE journal_runs
                 SET finished_at = ?1, status = ?2, renamed = ?3, failed = ?4
                 WHERE id = ?5",
                rusqlite::params![now, status, renamed as i64, failed as i64, run_id],
            )?;
            Ok(())
        })
    }

    pub fn set_run_status(&self, run_id: i64, status: &str) -> Result<(), DbError> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE journal_runs SET status = ?1, finished_at = ?2 WHERE id = ?3",
                rusqlite::params![status, now_unix(), run_id],
            )?;
            Ok(())
        })
    }

    pub fn record_intent(
        &self,
        run_id: i64,
        seq: i64,
        kind: JournalKind,
        source: &Path,
        target: &Path,
        temp: Option<&Path>,
    ) -> Result<i64, DbError> {
        let source = source.to_string_lossy().to_string();
        let target = target.to_string_lossy().to_string();
        let temp = temp.map(|p| p.to_string_lossy().to_string());
        let now = now_unix();
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO journal_entries
                    (run_id, seq, kind, source, target, temp, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'intent', ?7)",
                rusqlite::params![run_id, seq, kind.as_str(), source, target, temp, now],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn mark_done(&self, entry_id: i64) -> Result<(), DbError> {
        self.update_entry_status(entry_id, EntryStatus::Done, None)
    }

    pub fn mark_failed(&self, entry_id: i64, error: &str) -> Result<(), DbError> {
        self.update_entry_status(entry_id, EntryStatus::Failed, Some(error))
    }

    pub fn mark_rolled_back(&self, entry_id: i64) -> Result<(), DbError> {
        self.update_entry_status(entry_id, EntryStatus::RolledBack, None)
    }

    fn update_entry_status(
        &self,
        entry_id: i64,
        status: EntryStatus,
        error: Option<&str>,
    ) -> Result<(), DbError> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE journal_entries SET status = ?1, error = ?2 WHERE id = ?3",
                rusqlite::params![status.as_str(), error, entry_id],
            )?;
            Ok(())
        })
    }

    pub fn last_undoable_run(&self) -> Result<Option<RunSummary>, DbError> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, root, started_at, finished_at, status, total, renamed, failed
                 FROM journal_runs
                 WHERE status IN ('completed', 'partial', 'running')
                 ORDER BY id DESC LIMIT 1",
            )?;
            let mut rows = stmt.query([])?;
            match rows.next()? {
                Some(row) => Ok(Some(row_to_run(row)?)),
                None => Ok(None),
            }
        })
    }

    pub fn get_run(&self, run_id: i64) -> Result<Option<RunSummary>, DbError> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, root, started_at, finished_at, status, total, renamed, failed
                 FROM journal_runs WHERE id = ?1",
            )?;
            let mut rows = stmt.query(rusqlite::params![run_id])?;
            match rows.next()? {
                Some(row) => Ok(Some(row_to_run(row)?)),
                None => Ok(None),
            }
        })
    }

    pub fn list_runs(&self, limit: usize) -> Result<Vec<RunSummary>, DbError> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, root, started_at, finished_at, status, total, renamed, failed
                 FROM journal_runs ORDER BY id DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(rusqlite::params![limit as i64], row_to_run)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
    }

    pub fn entries_for_run(&self, run_id: i64) -> Result<Vec<JournalEntry>, DbError> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, run_id, seq, kind, source, target, temp, status, error, created_at
                 FROM journal_entries WHERE run_id = ?1 ORDER BY seq DESC, id DESC",
            )?;
            let rows = stmt.query_map(rusqlite::params![run_id], |row| {
                let kind: String = row.get(3)?;
                let status: String = row.get(7)?;
                Ok(JournalEntry {
                    id: row.get(0)?,
                    run_id: row.get(1)?,
                    seq: row.get(2)?,
                    kind: JournalKind::parse(&kind).unwrap_or(JournalKind::Rename),
                    source: PathBuf::from(row.get::<_, String>(4)?),
                    target: PathBuf::from(row.get::<_, String>(5)?),
                    temp: row.get::<_, Option<String>>(6)?.map(PathBuf::from),
                    status: EntryStatus::parse(&status).unwrap_or(EntryStatus::Intent),
                    error: row.get(8)?,
                    created_at: row.get(9)?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
    }
}

fn row_to_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunSummary> {
    Ok(RunSummary {
        id: row.get(0)?,
        root: PathBuf::from(row.get::<_, String>(1)?),
        started_at: row.get(2)?,
        finished_at: row.get(3)?,
        status: row.get(4)?,
        total: row.get(5)?,
        renamed: row.get(6)?,
        failed: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn journal() -> Journal {
        Journal::new(Arc::new(Db::open_in_memory().unwrap()))
    }

    #[test]
    fn records_a_run_and_entries() {
        let journal = journal();
        let run = journal.begin_run(Path::new("/movies"), 2).unwrap();
        let entry = journal
            .record_intent(
                run,
                0,
                JournalKind::Rename,
                Path::new("/movies/a.mkv"),
                Path::new("/movies/A (2020)/A.mkv"),
                None,
            )
            .unwrap();
        journal.mark_done(entry).unwrap();
        journal.finish_run(run, "completed", 1, 0).unwrap();

        let entries = journal.entries_for_run(run).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, EntryStatus::Done);
        assert_eq!(entries[0].kind, JournalKind::Rename);

        let last = journal.last_undoable_run().unwrap().unwrap();
        assert_eq!(last.id, run);
        assert_eq!(last.status, "completed");
    }

    #[test]
    fn last_run_skips_undone_runs() {
        let journal = journal();
        let first = journal.begin_run(Path::new("/a"), 1).unwrap();
        journal.finish_run(first, "completed", 1, 0).unwrap();
        let second = journal.begin_run(Path::new("/b"), 1).unwrap();
        journal.finish_run(second, "undone", 0, 0).unwrap();
        let last = journal.last_undoable_run().unwrap().unwrap();
        assert_eq!(last.id, first);
    }

    #[test]
    fn lists_runs_newest_first() {
        let journal = journal();
        let first = journal.begin_run(Path::new("/a"), 1).unwrap();
        journal.finish_run(first, "completed", 1, 0).unwrap();
        let second = journal.begin_run(Path::new("/b"), 1).unwrap();
        journal.finish_run(second, "completed", 1, 0).unwrap();
        let runs = journal.list_runs(10).unwrap();
        assert_eq!(runs[0].id, second);
        assert_eq!(runs[1].id, first);
    }
}
