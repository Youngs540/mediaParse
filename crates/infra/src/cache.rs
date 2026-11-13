use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::db::{now_unix, Db, DbError};

#[derive(Clone)]
pub struct MetadataCache {
    db: Arc<Db>,
    ttl: Duration,
}

impl MetadataCache {
    pub fn new(db: Arc<Db>, ttl_days: u32) -> Self {
        Self {
            db,
            ttl: Duration::from_secs(u64::from(ttl_days.max(1)) * 24 * 60 * 60),
        }
    }

    pub fn get<T: DeserializeOwned>(
        &self,
        provider: &str,
        key: &str,
    ) -> Result<Option<T>, DbError> {
        let now = now_unix();
        let payload: Option<String> = self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT payload FROM metadata_cache
                 WHERE provider = ?1 AND cache_key = ?2 AND expires_at > ?3",
            )?;
            let mut rows = stmt.query(rusqlite::params![provider, key, now])?;
            match rows.next()? {
                Some(row) => Ok(Some(row.get(0)?)),
                None => Ok(None),
            }
        })?;

        match payload {
            Some(json) => serde_json::from_str(&json).map(Some).map_err(|err| {
                DbError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(err)))
            }),
            None => Ok(None),
        }
    }

    pub fn put<T: Serialize>(&self, provider: &str, key: &str, value: &T) -> Result<(), DbError> {
        let payload = serde_json::to_string(value).map_err(|err| {
            DbError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(err)))
        })?;
        let now = now_unix();
        let expires = now + self.ttl.as_secs() as i64;
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO metadata_cache (provider, cache_key, payload, fetched_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(provider, cache_key) DO UPDATE SET
                    payload = excluded.payload,
                    fetched_at = excluded.fetched_at,
                    expires_at = excluded.expires_at",
                rusqlite::params![provider, key, payload, now, expires],
            )?;
            Ok(())
        })
    }

    pub fn invalidate(&self, provider: &str, key: &str) -> Result<(), DbError> {
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM metadata_cache WHERE provider = ?1 AND cache_key = ?2",
                rusqlite::params![provider, key],
            )?;
            Ok(())
        })
    }

    pub fn purge_expired(&self) -> Result<usize, DbError> {
        let now = now_unix();
        self.db.with_conn(|conn| {
            let deleted = conn.execute(
                "DELETE FROM metadata_cache WHERE expires_at <= ?1",
                rusqlite::params![now],
            )?;
            Ok(deleted)
        })
    }

    pub fn count(&self) -> Result<i64, DbError> {
        self.db.with_conn(|conn| {
            conn.query_row("SELECT COUNT(*) FROM metadata_cache", [], |row| row.get(0))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_reads_values() {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let cache = MetadataCache::new(db, 30);
        let value = vec!["hello".to_string()];
        cache.put("tmdb", "search:test", &value).unwrap();
        let loaded: Option<Vec<String>> = cache.get("tmdb", "search:test").unwrap();
        assert_eq!(loaded, Some(value));
    }

    #[test]
    fn misses_unknown_keys() {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let cache = MetadataCache::new(db, 30);
        let loaded: Option<Vec<String>> = cache.get("tmdb", "missing").unwrap();
        assert_eq!(loaded, None);
    }

    #[test]
    fn invalidates_keys() {
        let db = Arc::new(Db::open_in_memory().unwrap());
        let cache = MetadataCache::new(db, 30);
        cache.put("tmdb", "key", &"value").unwrap();
        cache.invalidate("tmdb", "key").unwrap();
        let loaded: Option<String> = cache.get("tmdb", "key").unwrap();
        assert_eq!(loaded, None);
    }
}
