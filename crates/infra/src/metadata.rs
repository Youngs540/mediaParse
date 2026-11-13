use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use mediaparse_core::{MatchCandidate, MediaType};

use crate::db::DbError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaDetails {
    pub candidate: MatchCandidate,
    pub runtime_minutes: Option<u32>,
    pub imdb_id: Option<String>,
    pub overview: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EpisodeDetails {
    pub title: Option<String>,
    pub runtime_minutes: Option<u32>,
    pub air_date: Option<String>,
    pub overview: Option<String>,
}

#[async_trait]
pub trait MetadataProvider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn search(
        &self,
        query: &str,
        year: Option<u16>,
        media_type: MediaType,
    ) -> Result<Vec<MatchCandidate>, MetadataError>;

    async fn details(
        &self,
        id: &str,
        media_type: MediaType,
    ) -> Result<Option<MediaDetails>, MetadataError>;

    /// Episode-level details for TV. Providers without episode support return
    /// `None`, and the formatter falls back to the episode number only.
    async fn episode_details(
        &self,
        series_id: &str,
        season: u16,
        episode: u16,
    ) -> Result<Option<EpisodeDetails>, MetadataError> {
        let _ = (series_id, season, episode);
        Ok(None)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("metadata API error: {0}")]
    Api(String),
    #[error("offline mode: no cached metadata for this lookup")]
    Offline,
    #[error("cache error: {0}")]
    Cache(#[from] DbError),
    #[error("invalid metadata configuration: {0}")]
    Config(String),
    #[error("rate limited by metadata provider after retries")]
    RateLimited,
    #[error("could not decode metadata response: {0}")]
    Json(#[from] serde_json::Error),
}
